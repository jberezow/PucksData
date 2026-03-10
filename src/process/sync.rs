// src/process/sync.rs
// Sync orchestration: gap detection (completed games with no events) and the run_sync() orchestrator.
// Implements SYNC-01, SYNC-02, SYNC-03, QUAL-SYNC-01, DAEMON-04.

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

/// Gap detection query (SYNC-02): returns games where game_date < today and no events row exists.
/// from_date: optional floor on game_date (None = all historical games, Some(d) = >= d).
/// Returns (game_id, game_state) pairs — game_state filtering is done in Rust (QUAL-SYNC-01).
pub(crate) async fn query_sync_candidates(
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
    crate::loaders::teams::upsert_teams(pool, &teams).await?;
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
    if total == 0 {
        let elapsed = started_at.elapsed();
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        println!(
            "[{}] sync complete: 0 processed, 0 failed, elapsed {}s",
            ts, elapsed.as_secs()
        );
        return Ok(SyncSummary { processed: 0, failed: 0, elapsed });
    }

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

    let elapsed = started_at.elapsed();
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    println!(
        "[{}] sync complete: {} processed, {} failed, elapsed {}s",
        ts, processed, failed, elapsed.as_secs()
    );

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
