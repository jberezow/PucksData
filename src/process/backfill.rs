// src/process/backfill.rs
// Backfill orchestration: checkpoint helpers and main orchestrator (Plan 02 adds run_backfill).

/// Seed backfill_progress for all games in scope.
/// INSERT ... ON CONFLICT DO NOTHING so existing rows (done/failed) survive unchanged.
/// season_filter: None = all seasons, Some(year) = restrict to one season.
pub async fn seed_backfill_progress(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO backfill_progress (game_id, season, status)
         SELECT game_id, season, 'pending'
         FROM games
         WHERE ($1::integer IS NULL OR season = $1)
         ON CONFLICT (game_id) DO NOTHING",
        season_filter
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Update a game's status in backfill_progress.
/// Call with "done" on success, "failed" on error.
pub async fn update_progress_status(
    pool: &sqlx::PgPool,
    game_id: i64,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backfill_progress
         SET status = $1, updated_at = NOW()
         WHERE game_id = $2",
        status,
        game_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Query all non-done games in scope (returns Vec<(game_id, season)>).
/// Used after seeding to build the work list for the current run.
/// Includes both 'pending' and 'failed' games (failed games are retried).
pub async fn query_pending_games(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<Vec<(i64, i32)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT game_id, season
         FROM backfill_progress
         WHERE ($1::integer IS NULL OR season = $1)
           AND status != 'done'
         ORDER BY season ASC, game_id ASC",
        season_filter
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.game_id, r.season)).collect())
}

/// Fetch, transform, and load all events for one game.
/// Called inside a JoinSet task — errors are captured, not propagated.
pub async fn load_one_game(
    pool: &sqlx::PgPool,
    game_id: i64,
    team_id_map: &std::collections::HashMap<i64, i64>,
) -> Result<(), crate::AnyError> {
    let pbp = crate::fetchers::events::fetch_play_by_play(game_id).await?;
    let (events, goals, shots, hits, blocks, penalties, faceoffs, skip_warnings) =
        crate::fetchers::events::transform_events(&pbp, team_id_map);
    // skip_warnings are swallowed in batch mode (volume too high for per-game warnings)
    let _ = skip_warnings;
    crate::loaders::events::upsert_game_events(
        pool, game_id,
        &events, &goals, &shots, &hits, &blocks, &penalties, &faceoffs,
    ).await?;
    Ok(())
}

/// Run the full (or season-scoped) backfill.
/// season_filter: None = all seasons, Some(year) = one 8-digit season ID (e.g. 20232024)
pub async fn run_backfill(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<(), crate::AnyError> {
    use std::sync::Arc;
    use tokio::sync::Semaphore;
    use tokio::task::JoinSet;
    use indicatif::{ProgressBar, ProgressStyle};

    const MAX_CONCURRENT_GAMES: usize = 5;

    // Step 1: Fetch team_id_map once — shared across all spawned tasks via Arc
    let team_id_map = Arc::new(
        crate::fetchers::games::fetch_team_id_to_franchise_id_map().await?
    );

    // Step 2: Seed backfill_progress with all in-scope games (ON CONFLICT DO NOTHING)
    seed_backfill_progress(pool, season_filter).await?;

    // Step 3: Query pending games (status != 'done')
    let pending_games = query_pending_games(pool, season_filter).await?;
    let total = pending_games.len();

    if total == 0 {
        println!("Backfill complete: 0 games pending (all already done)");
        return Ok(());
    }

    // Step 4: Set up progress bar
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}"
        )
        .unwrap()
        .progress_chars("=>-"),
    );

    // Step 5: Semaphore + JoinSet loop
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_GAMES));
    let mut join_set: JoinSet<(i64, i32, Result<(), crate::AnyError>)> = JoinSet::new();

    // Track current season for per-season summaries
    let mut season_done: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    let mut season_failed: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();

    // Spawn all tasks (acquire_owned blocks when all 5 permits held — natural backpressure)
    for (i, (game_id, season)) in pending_games.iter().enumerate() {
        let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
        let game_id = *game_id;
        let season = *season;
        let pool_clone = pool.clone();
        let map = team_id_map.clone();
        let n = i + 1;
        pb.println(format!("Processing game {} of {}, season {}", n, total, season));

        join_set.spawn(async move {
            let _permit = permit;
            let result = load_one_game(&pool_clone, game_id, &map).await;
            (game_id, season, result)
        });
    }

    // Collect results
    let mut total_done = 0usize;
    let mut total_failed = 0usize;

    while let Some(outcome) = join_set.join_next().await {
        match outcome {
            Ok((game_id, season, Ok(()))) => {
                update_progress_status(pool, game_id, "done").await
                    .unwrap_or_else(|e| pb.suspend(|| eprintln!("warn: checkpoint update failed for game {}: {}", game_id, e)));
                *season_done.entry(season).or_insert(0) += 1;
                total_done += 1;
            }
            Ok((game_id, season, Err(e))) => {
                update_progress_status(pool, game_id, "failed").await
                    .unwrap_or_else(|e2| pb.suspend(|| eprintln!("warn: checkpoint update failed for game {}: {}", game_id, e2)));
                pb.suspend(|| eprintln!("warn: game {} (season {}) failed: {}", game_id, season, e));
                *season_failed.entry(season).or_insert(0) += 1;
                total_failed += 1;
            }
            Err(join_err) => {
                pb.suspend(|| eprintln!("warn: task join error: {}", join_err));
                total_failed += 1;
            }
        }
        pb.inc(1);
    }

    pb.finish_and_clear();

    // Per-season summaries
    let mut all_seasons: std::collections::BTreeSet<i32> = std::collections::BTreeSet::new();
    all_seasons.extend(season_done.keys());
    all_seasons.extend(season_failed.keys());
    for season in &all_seasons {
        let done = season_done.get(season).copied().unwrap_or(0);
        let failed = season_failed.get(season).copied().unwrap_or(0);
        println!("Season {}: {} done, {} failed", season, done, failed);
    }

    // Final summary
    println!("Backfill complete: {} games processed, {} failed", total_done + total_failed, total_failed);

    Ok(())
}
