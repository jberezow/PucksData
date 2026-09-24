//! Incremental sync orchestrator — gap detection and event ingestion for completed games.
use chrono::Datelike;
use sqlx::postgres::PgAdvisoryLock;
use sqlx::Either;

/// Summary returned by run_sync() on every success path.
pub struct SyncSummary {
    pub processed: usize,
    pub failed: usize,
    pub elapsed: std::time::Duration,
    pub candidates: usize,
    pub events_written: usize,
}

/// Returns true if this gameState value indicates the game is definitively finished.
/// Accepted completed states: "OFF", "OVER", "FINAL".
/// Any other value is unknown — caller should log a warning and skip.
pub fn is_game_completed(state: &str) -> bool {
    matches!(state, "OFF" | "OVER" | "FINAL")
}

/// Derive the 8-digit NHL season ID for a given calendar month and year.
///
/// NHL seasons span two calendar years (e.g. 2025-2026 season ID = 20252026).
/// A new season starts in October. Months before October (1–9) belong to the
/// season that started the previous October.
///
/// Examples:
///   season_for_date(10, 2025) == 20252026  (Oct 2025 → start of 2025-26 season)
///   season_for_date(3,  2026) == 20252026  (Mar 2026 → mid 2025-26 season)
///   season_for_date(9,  2025) == 20242025  (Sep 2025 → offseason, 2024-25 was last)
pub fn season_for_date(month: u32, year: i32) -> i32 {
    if month >= 10 {
        // October onwards: new season starting this year
        year * 10_000 + (year + 1)
    } else {
        // January–September: season started last October
        (year - 1) * 10_000 + year
    }
}

/// Return the 8-digit NHL season ID for the current UTC date.
///
/// Calls `season_for_date` with the current UTC month and year.
pub fn current_season() -> i32 {
    let now = chrono::Utc::now();
    season_for_date(now.month(), now.year())
}

async fn refresh_current_season_games(pool: &sqlx::PgPool) -> Result<usize, crate::AnyError> {
    let mut seasons = vec![current_season()];
    let now = chrono::Utc::now();
    if now.month() == 9 {
        seasons.push(now.year() * 10_000 + now.year() + 1);
    }
    let mut count = 0;
    for season in seasons {
        let progress = indicatif::ProgressBar::hidden();
        let games =
            crate::fetchers::games::fetch_games_for_season_enriched(season, &progress).await?;
        count += crate::loaders::games::upsert_games(pool, &games, &progress).await?;
    }
    Ok(count)
}

/// Acquire the session-level advisory lock used to enforce a single daemon.
///
/// The caller must retain the guard for the daemon's lifetime; dropping it
/// releases the lock and returns its connection to the pool.
pub async fn acquire_daemon_lock(
    pool: &sqlx::PgPool,
) -> Result<
    sqlx::postgres::PgAdvisoryLockGuard<sqlx::pool::PoolConnection<sqlx::Postgres>>,
    crate::AnyError,
> {
    let lock = PgAdvisoryLock::new("pucksdata_daemon");
    let conn = pool.acquire().await?;
    match lock.try_acquire(conn).await? {
        Either::Left(guard) => Ok(guard),
        Either::Right(_conn) => Err(
            "pucksdata daemon is already running (advisory lock held by another instance)".into(),
        ),
    }
}

/// Return past games with no events, optionally bounded by a starting date.
///
/// Game-state filtering remains in Rust so unknown states can be reported.
pub async fn query_sync_candidates(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
) -> Result<Vec<(i64, Option<String>)>, sqlx::Error> {
    query_sync_candidates_with_window(pool, from_date, 0).await
}

/// Include recent completed games and unsuccessful attempts even when rows exist.
pub async fn query_sync_candidates_with_window(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
    audit_days: i32,
) -> Result<Vec<(i64, Option<String>)>, sqlx::Error> {
    sqlx::query_as(r#"SELECT g.game_id, g.game_state FROM games g
        WHERE g.game_date <= (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date
          AND ($1::date IS NULL OR g.game_date >= $1)
          AND g.game_type IN (2,3)
          AND (
            ($1::date IS NOT NULL)
            OR ($2 > 0 AND g.game_date >= (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date - $2)
            OR (NOT EXISTS (SELECT 1 FROM events e WHERE e.game_id = g.game_id)
                AND NOT EXISTS (SELECT 1 FROM backfill_progress bp WHERE bp.game_id = g.game_id AND bp.status IN ('done','skipped')))
            OR (SELECT a.outcome FROM ingestion.attempts a WHERE a.dataset = 'events'
                AND a.entity_key = g.game_id::text ORDER BY a.attempt_id DESC LIMIT 1) IN ('failed','running')
          ) ORDER BY g.game_date, g.game_id"#)
        .bind(from_date).bind(audit_days).fetch_all(pool).await
}

/// Daily recent corrections, with a wider Sunday audit. Override explicitly for
/// older corrections; invalid configuration is never silently ignored.
pub fn correction_days() -> Result<i32, crate::AnyError> {
    let default = if chrono::Utc::now().weekday() == chrono::Weekday::Sun {
        14
    } else {
        3
    };
    let days = match std::env::var("PUCKSDATA_CORRECTION_DAYS") {
        Ok(value) => value.parse::<i32>()?,
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => return Err(error.into()),
    };
    if !(1..=366).contains(&days) {
        return Err("PUCKSDATA_CORRECTION_DAYS must be 1..366".into());
    }
    Ok(days)
}

/// A complete run alone advances the legacy last-success field. Attempts retain
/// partial/failure information separately, including interrupted runs.
pub async fn run_sync(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
) -> Result<SyncSummary, crate::AnyError> {
    super::attempts::exclusive(pool, run_sync_exclusive(pool, from_date)).await
}

async fn run_sync_exclusive(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
) -> Result<SyncSummary, crate::AnyError> {
    let id = super::attempts::start(pool, "sync", "singleton").await?;
    let result = crate::provenance::scope(
        crate::provenance::Context {
            pool: pool.clone(),
            attempt_id: id,
        },
        run_sync_inner(pool, from_date),
    )
    .await;
    record_sync_result(pool, id, &result).await?;
    match result {
        Ok(summary) if summary.failed > 0 => {
            Err(format!("{} game loads failed", summary.failed).into())
        }
        other => other,
    }
}

/// Finish a sync attempt and advance the success watermark atomically.
pub async fn record_sync_result(
    pool: &sqlx::PgPool,
    id: i64,
    result: &Result<SyncSummary, crate::AnyError>,
) -> Result<(), crate::AnyError> {
    let (outcome, error) = match result {
        Ok(summary) if summary.failed > 0 => (
            "partial",
            Some(format!("{} game loads failed", summary.failed)),
        ),
        Ok(_) => ("complete", None),
        Err(error) => ("failed", Some(error.to_string())),
    };
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE ingestion.attempts SET outcome=$2, error_message=$3, finished_at=clock_timestamp() WHERE attempt_id=$1")
        .bind(id).bind(outcome).bind(error).execute(&mut *tx).await?;
    if let Ok(summary) = result {
        if summary.failed == 0 {
            sqlx::query("INSERT INTO sync_state(key, last_sync_at, last_sync_games, updated_at)
                VALUES ('singleton', clock_timestamp(), $1, clock_timestamp())
                ON CONFLICT(key) DO UPDATE SET last_sync_at=EXCLUDED.last_sync_at, last_sync_games=EXCLUDED.last_sync_games, updated_at=EXCLUDED.updated_at")
                .bind(summary.processed as i32).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Refresh entities and ingest completed games that have no events.
async fn run_sync_inner(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
) -> Result<SyncSummary, crate::AnyError> {
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::Semaphore;
    use tokio::task::JoinSet;

    let started_at = Instant::now();
    let audit_days = correction_days()?;

    // Refresh teams before building the team-to-franchise map.
    println!("[sync 1/5] refreshing teams...");
    let teams = crate::fetchers::teams::fetch_teams().await?;
    crate::loaders::teams::upsert_teams(pool, &teams, &indicatif::ProgressBar::hidden()).await?;
    println!("[sync 1/5] {} teams upserted", teams.len());

    super::attempts::track(
        pool,
        "schedule",
        "current",
        refresh_current_season_games(pool),
    )
    .await?;

    println!("[sync 2/5] enumerating and refreshing players (rosters + stats pages — this takes ~30s)...");
    super::attempts::track(pool, "players_rosters", "current", async {
        let fetched_players = crate::fetchers::players::fetch_players(pool).await?;
        crate::loaders::players::upsert_players(pool, &fetched_players.players).await?;
        println!(
            "[sync 2/5] {} players upserted",
            fetched_players.players.len()
        );
        if let Some(rosters) = fetched_players.current_rosters {
            if rosters.is_complete() {
                let snapshot_id =
                    crate::loaders::rosters::insert_roster_snapshot(pool, &rosters).await?;
                println!(
                    "[sync 2/5] roster snapshot {snapshot_id} written ({} teams, {} memberships)",
                    rosters.fetched_team_count,
                    rosters.memberships.len()
                );
            } else {
                return Err("current roster observation incomplete; snapshot preserved".into());
            }
        } else {
            return Err("current roster observation unavailable".into());
        }
        Ok(())
    })
    .await?;

    let team_id_map = Arc::new(crate::fetchers::games::fetch_team_id_to_franchise_id_map().await?);

    println!("[sync 3/5] detecting games with missing events...");
    let candidates = query_sync_candidates_with_window(pool, from_date, audit_days).await?;
    let candidates_count = candidates.len(); // all gap-detected candidates, before game_state filter
    println!("[sync 3/5] {candidates_count} candidate games found (regular season + playoffs, gaps or correction audit)");

    let mut games_to_process: Vec<i64> = Vec::new();
    for (game_id, state) in &candidates {
        match state.as_deref() {
            Some(s) if is_game_completed(s) => games_to_process.push(*game_id),
            Some(s) => eprintln!("warn: unknown gameState {s:?} for game {game_id} — skipping"),
            None => {} // NULL game_state — not completed, skip silently
        }
    }

    let total = games_to_process.len();
    println!("[sync 4/5] {total} games ready to process (game_state OFF/OVER/FINAL)");

    let (processed, failed, events_written) = if total == 0 {
        println!("[sync 4/5] nothing to do");
        (0usize, 0usize, 0usize)
    } else {
        const MAX_CONCURRENT_GAMES: usize = 5;
        let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_GAMES));
        let mut join_set: JoinSet<(i64, Result<usize, crate::AnyError>)> = JoinSet::new();

        let pb = crate::ui::make_progress_bar(total as u64, "games");

        for game_id in games_to_process.iter() {
            let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
            let game_id = *game_id;
            let pool_clone = pool.clone();
            let map = team_id_map.clone();

            join_set.spawn(async move {
                let _permit = permit; // released on drop
                let result =
                    crate::process::backfill::load_one_game(&pool_clone, game_id, &map).await;
                (game_id, result)
            });
        }

        let mut processed = 0usize;
        let mut failed = 0usize;
        let mut events_written = 0usize;

        while let Some(outcome) = join_set.join_next().await {
            match outcome {
                Ok((_, Ok(count))) => {
                    events_written += count;
                    processed += 1;
                }
                Ok((game_id, Err(e))) => {
                    pb.suspend(|| eprintln!("warn: game {game_id} failed: {e}"));
                    failed += 1;
                }
                Err(join_err) => {
                    pb.suspend(|| eprintln!("warn: task join error: {join_err}"));
                    failed += 1;
                }
            }
            pb.inc(1);
        }
        pb.finish_and_clear();
        (processed, failed, events_written)
    };

    let elapsed = started_at.elapsed();
    let duration_secs = elapsed.as_secs_f64();
    println!(
        "[sync] event phase finished:\n  candidates:  {candidates_count}\n  processed:   {processed}\n  failed:      {failed}\n  events:      {events_written}\n  duration:    {duration_secs:.1}s",
    );

    // Repair player references after all new event rows are visible.
    let repair_result = super::attempts::track(
        pool,
        "player_repair",
        "archive",
        crate::fetchers::players::repair_missing_players(pool),
    )
    .await;

    // The daemon and workflow run precisely the same official correction policy.
    let audit_from =
        time::OffsetDateTime::now_utc().date() - time::Duration::days(audit_days as i64);
    let official_result =
        super::official_games::sync_official_games(pool, from_date.unwrap_or(audit_from)).await;
    let refresh_result = super::analytics::refresh_derived(pool).await;
    repair_result?;
    official_result?;
    refresh_result?;

    Ok(SyncSummary {
        processed,
        failed,
        elapsed: started_at.elapsed(),
        candidates: candidates_count,
        events_written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_game_completed() {
        assert!(is_game_completed("OFF"));
        assert!(is_game_completed("OVER"));
        assert!(is_game_completed("FINAL"));
        assert!(!is_game_completed("LIVE"));
        assert!(!is_game_completed("PPD"));
        assert!(!is_game_completed("FUT"));
        assert!(!is_game_completed(""));
    }

    #[test]
    fn test_sync_summary_fields() {
        let s = SyncSummary {
            processed: 3,
            failed: 1,
            elapsed: std::time::Duration::ZERO,
            candidates: 5,
            events_written: 120,
        };
        assert_eq!(s.candidates, 5);
        assert_eq!(s.events_written, 120);
        assert_eq!(s.processed, 3);
        assert_eq!(s.failed, 1);
    }
}
