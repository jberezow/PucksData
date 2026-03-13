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
    let teams = crate::fetchers::teams::fetch_teams().await?;
    crate::loaders::teams::upsert_teams(pool, &teams, &indicatif::ProgressBar::hidden()).await?;
    let players = crate::fetchers::players::fetch_players().await?;
    crate::loaders::players::upsert_players(pool, &players).await?;

    // Step 2: Fetch team_id_map once — shared across all spawned tasks via Arc
    let team_id_map = Arc::new(
        crate::fetchers::games::fetch_team_id_to_franchise_id_map().await?
    );

    // Step 3: Gap detection query (SYNC-02) — returns (game_id, game_state) pairs
    let candidates = query_sync_candidates(pool, from_date).await?;

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
    let (processed, failed) = if total == 0 {
        (0usize, 0usize)
    } else {
        // Step 5: Semaphore + JoinSet concurrency (same as run_backfill — MAX_CONCURRENT_GAMES = 5)
        const MAX_CONCURRENT_GAMES: usize = 5;
        let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_GAMES));
        let mut join_set: JoinSet<(i64, Result<(), crate::AnyError>)> = JoinSet::new();

        for (n, game_id) in games_to_process.iter().enumerate() {
            let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
            let game_id = *game_id;
            let pool_clone = pool.clone();
            let map = team_id_map.clone();
            println!("Processing game {} of {} (id={})", n + 1, total, game_id);

            join_set.spawn(async move {
                let _permit = permit; // released on drop
                let result = crate::process::backfill::load_one_game(&pool_clone, game_id, &map).await;
                (game_id, result)
            });
        }

        let mut processed = 0usize;
        let mut failed = 0usize;

        while let Some(outcome) = join_set.join_next().await {
            match outcome {
                Ok((_, Ok(()))) => processed += 1,
                Ok((game_id, Err(e))) => {
                    eprintln!("warn: game {} failed: {}", game_id, e);
                    failed += 1;
                }
                Err(join_err) => {
                    eprintln!("warn: task join error: {}", join_err);
                    failed += 1;
                }
            }
        }
        (processed, failed)
    };

    let elapsed = started_at.elapsed();
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    println!(
        "[{}] sync complete: {} processed, {} failed, elapsed {}s",
        ts, processed, failed, elapsed.as_secs()
    );

    // Step 6: Upsert sync_state metadata (SCHEMA-15).
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

    Ok(SyncSummary { processed, failed, elapsed })
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
}
