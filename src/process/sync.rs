// src/process/sync.rs
// Sync orchestration: gap detection (completed games with no events) and the run_sync() orchestrator.
// Implements SYNC-01, SYNC-02, SYNC-03, QUAL-SYNC-01, QUAL-SYNC-02, DAEMON-04, SCHEMA-15.

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

/// Acquire the pucksdata daemon advisory lock for single-instance enforcement (QUAL-SYNC-02).
/// Returns Ok(guard) if the lock was acquired — caller MUST hold the guard for the process lifetime.
/// The guard is RAII: dropping it releases the lock and returns the connection to the pool.
/// Returns Err with a clean message if another daemon instance is already running.
///
/// PITFALL 1: Do NOT drop the guard early. Hold it in a `let _lock = acquire_daemon_lock(...).await?`
///            binding at the top of the daemon arm in main.rs (Phase 8 task). Underscore prefix
///            keeps the binding alive without an "unused variable" warning.
/// PITFALL 2: try_acquire takes PoolConnection<Postgres>, not &PgPool. pool.acquire() is called first.
/// PITFALL 3: PgAdvisoryLockGuard<'lock, C> borrows from PgAdvisoryLock — Box::leak gives 'static lifetime
///            so the guard can be returned from this function without lifetime errors.
pub async fn acquire_daemon_lock(
    pool: &sqlx::PgPool,
) -> Result<sqlx::postgres::PgAdvisoryLockGuard<'static, sqlx::pool::PoolConnection<sqlx::Postgres>>, crate::AnyError> {
    // Box::leak gives the lock a 'static lifetime — valid for a daemon that holds the guard until exit.
    let lock: &'static PgAdvisoryLock = Box::leak(Box::new(PgAdvisoryLock::new("pucksdata_daemon")));
    let conn = pool.acquire().await?;
    match lock.try_acquire(conn).await? {
        Either::Left(guard) => Ok(guard),
        Either::Right(_conn) => {
            Err("pucksdata daemon is already running (advisory lock held by another instance)".into())
        }
    }
}

/// Gap detection query (SYNC-02): returns games where game_date < today and no events row exists.
/// from_date: optional floor on game_date (None = all historical games, Some(d) = >= d).
/// Returns (game_id, game_state) pairs — game_state filtering is done in Rust (QUAL-SYNC-01).
pub async fn query_sync_candidates(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
) -> Result<Vec<(i64, Option<String>)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT g.game_id, g.game_state
           FROM games g
           WHERE g.game_date < CURRENT_DATE
             AND ($1::date IS NULL OR g.game_date >= $1)
             AND g.game_type != 1
             AND NOT EXISTS (
               SELECT 1 FROM events e WHERE e.game_id = g.game_id
             )
           ORDER BY g.game_date ASC, g.game_id ASC"#,
        from_date
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.game_id, r.game_state)).collect())
}

/// Run the sync process: entity refresh → gap detection → event ingestion via load_one_game.
/// from_date: optional floor on game_date for candidate detection.
/// Returns Ok(SyncSummary) on all success paths (including zero candidates).
/// Single return point at bottom — sync_state upsert fires on BOTH zero-candidate and non-zero paths.
pub async fn run_sync(
    pool: &sqlx::PgPool,
    from_date: Option<time::Date>,
) -> Result<SyncSummary, crate::AnyError> {
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::Semaphore;
    use tokio::task::JoinSet;

    let started_at = Instant::now();

    // Step 1: Entity refresh — teams then players (SYNC-03)
    // Must happen before team_id_map fetch to avoid stale map (Pitfall 3 from RESEARCH.md)
    println!("[sync 1/5] refreshing teams...");
    let teams = crate::fetchers::teams::fetch_teams().await?;
    crate::loaders::teams::upsert_teams(pool, &teams, &indicatif::ProgressBar::hidden()).await?;
    println!("[sync 1/5] {} teams upserted", teams.len());

    println!("[sync 2/5] enumerating and refreshing players (rosters + stats pages — this takes ~30s)...");
    let players = crate::fetchers::players::fetch_players(pool).await?;
    crate::loaders::players::upsert_players(pool, &players).await?;
    println!("[sync 2/5] {} players upserted", players.len());

    // Step 2: Fetch team_id_map once — shared across all spawned tasks via Arc
    let team_id_map = Arc::new(
        crate::fetchers::games::fetch_team_id_to_franchise_id_map().await?
    );

    // Step 3: Gap detection query (SYNC-02) — returns (game_id, game_state) pairs
    println!("[sync 3/5] detecting games with missing events...");
    let candidates = query_sync_candidates(pool, from_date).await?;
    let candidates_count = candidates.len(); // all gap-detected candidates, before game_state filter
    println!("[sync 3/5] {} candidate games found (regular season + playoffs, no events yet)", candidates_count);

    // Step 4: Filter by is_game_completed(), warn on unknown states (QUAL-SYNC-01)
    // Do NOT filter in SQL — must log unknown states explicitly
    let mut games_to_process: Vec<i64> = Vec::new();
    for (game_id, state) in &candidates {
        match state.as_deref() {
            Some(s) if is_game_completed(s) => games_to_process.push(*game_id),
            Some(s) => eprintln!(
                "warn: unknown gameState {:?} for game {} — skipping",
                s, game_id
            ),
            None => {} // NULL game_state — not completed, skip silently
        }
    }

    let total = games_to_process.len();
    println!("[sync 4/5] {} games ready to process (game_state OFF/OVER/FINAL)", total);

    let (processed, failed, events_written) = if total == 0 {
        println!("[sync 4/5] nothing to do");
        (0usize, 0usize, 0usize)
    } else {
        // Step 5: Semaphore + JoinSet concurrency (same as run_backfill — MAX_CONCURRENT_GAMES = 5)
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
                let result = crate::process::backfill::load_one_game(&pool_clone, game_id, &map).await;
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
                    pb.suspend(|| eprintln!("warn: game {} failed: {}", game_id, e));
                    failed += 1;
                }
                Err(join_err) => {
                    pb.suspend(|| eprintln!("warn: task join error: {}", join_err));
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
        "[sync 5/5] complete:\n  candidates:  {}\n  processed:   {}\n  failed:      {}\n  events:      {}\n  duration:    {:.1}s",
        candidates_count,
        processed,
        failed,
        events_written,
        duration_secs,
    );

    // Step 6: Gap repair — fetch any player IDs present in event tables but absent from players.
    // Catches players that slipped through enumerate_player_ids (edge cases, future regressions,
    // or pre-existing gaps in historical data). Runs after event writes so all new event rows
    // are visible to the repair query.
    match crate::fetchers::players::repair_missing_players(pool).await {
        Ok(0) => {}
        Ok(n) => println!("repair: inserted {} previously-missing players", n),
        Err(e) => eprintln!("warn: repair_missing_players failed (non-fatal): {}", e),
    }

    // Step 7: Upsert sync_state metadata (SCHEMA-15).
    // Runs on BOTH zero-candidate and non-zero paths — single return point prevents Pitfall 3.
    // Informational only — failure here does not invalidate the sync; but we propagate Err
    // so the operator knows the metadata write failed.
    let now = time::OffsetDateTime::now_utc();
    sqlx::query!(
        r#"INSERT INTO sync_state (key, last_sync_at, last_sync_games, updated_at)
       VALUES ('singleton', $1, $2, $1)
       ON CONFLICT (key) DO UPDATE
         SET last_sync_at    = EXCLUDED.last_sync_at,
             last_sync_games = EXCLUDED.last_sync_games,
             updated_at      = EXCLUDED.updated_at"#,
        now,
        processed as i32
    )
    .execute(pool)
    .await?;

    Ok(SyncSummary { processed, failed, elapsed, candidates: candidates_count, events_written })
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
