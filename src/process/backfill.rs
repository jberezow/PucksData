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

/// Update game status and store error message.
/// Use for 'failed' transitions. Keep update_progress_status for 'done'/'skipped'.
pub async fn update_progress_with_error(
    pool: &sqlx::PgPool,
    game_id: i64,
    status: &str,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backfill_progress
         SET status = $1, error_message = $2, updated_at = NOW()
         WHERE game_id = $3",
        status,
        error_message,
        game_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns true if the error represents a known API gap (404 Not Found).
/// These games are classified as 'skipped', not 'failed' — they will not be retried.
/// Uses downcast_ref — type-safe, not string matching on error messages.
pub fn is_api_gap_error(e: &crate::AnyError) -> bool {
    e.downcast_ref::<crate::api::ApiError>()
        .map(|api_err| matches!(api_err, crate::api::ApiError::NotFound))
        .unwrap_or(false)
}

/// Per-game metadata returned by query_pending_games.
/// Carries game_date, home_abbrev, and away_abbrev for log line emission.
pub struct PendingGame {
    pub game_id: i64,
    pub season: i32,
    pub game_date: time::Date,
    pub home_abbrev: String,
    pub away_abbrev: String,
}

/// Query all non-done games in scope (returns Vec<PendingGame> with joined metadata).
/// Used after seeding to build the work list for the current run.
/// Includes both 'pending' and 'failed' games (failed games are retried).
pub async fn query_pending_games(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<Vec<PendingGame>, sqlx::Error> {
    let rows = sqlx::query_as!(
        PendingGame,
        "SELECT bp.game_id, bp.season,
                g.game_date,
                ht.abbrev AS home_abbrev,
                at_.abbrev AS away_abbrev
         FROM backfill_progress bp
         JOIN games g ON g.game_id = bp.game_id
         JOIN teams ht ON ht.team_id = g.home_team_id
         JOIN teams at_ ON at_.team_id = g.away_team_id
         WHERE ($1::integer IS NULL OR bp.season = $1)
           AND bp.status NOT IN ('done', 'skipped')
         ORDER BY bp.season ASC, bp.game_id ASC",
        season_filter
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fetch, transform, and load all events for one game.
/// Called inside a JoinSet task — errors are captured, not propagated.
/// Returns the total event count (sum of all event type counts) on success.
pub async fn load_one_game(
    pool: &sqlx::PgPool,
    game_id: i64,
    team_id_map: &std::collections::HashMap<i64, i64>,
) -> Result<usize, crate::AnyError> {
    let pbp = crate::fetchers::events::fetch_play_by_play(game_id).await?;
    let (events, goals, shots, hits, blocks, penalties, faceoffs, skip_warnings) =
        crate::fetchers::events::transform_events(&pbp, team_id_map);
    // skip_warnings are swallowed in batch mode (volume too high for per-game warnings)
    let _ = skip_warnings;
    let (ec, gc, sc, hc, bc, pc, fc) = crate::loaders::events::upsert_game_events(
        pool, game_id,
        &events, &goals, &shots, &hits, &blocks, &penalties, &faceoffs,
    ).await?;
    Ok(ec + gc + sc + hc + bc + pc + fc)
}

/// Run the full (or season-scoped) backfill.
/// season_filter: None = all seasons, Some(year) = one 8-digit season ID (e.g. 20232024)
pub async fn run_backfill(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<(), crate::AnyError> {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::task::JoinSet;
    use indicatif::{ProgressBar, ProgressStyle};

    const MAX_CONCURRENT_GAMES: usize = 5;

    // Init spinner — wraps Steps 1-3
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner} {msg}")
            .unwrap()
            .tick_strings(&["\u{29fe}", "\u{29fd}", "\u{29fb}", "\u{23bf}", "\u{23bf}", "\u{29df}", "\u{29af}", "\u{29b7}", ""])
    );
    spinner.enable_steady_tick(Duration::from_millis(80));

    // Step 1: Fetch team_id_map once — shared across all spawned tasks via Arc
    spinner.set_message("Fetching entity data...");
    let team_id_map = Arc::new(
        crate::fetchers::games::fetch_team_id_to_franchise_id_map().await
            .inspect_err(|_| spinner.finish_and_clear())?
    );

    // Step 2: Seed backfill_progress with all in-scope games (ON CONFLICT DO NOTHING)
    spinner.set_message("Seeding backfill queue...");
    seed_backfill_progress(pool, season_filter).await
        .inspect_err(|_| spinner.finish_and_clear())?;

    // Step 3: Query pending games (status != 'done')
    spinner.set_message("Loading pending games...");
    let pending_games = query_pending_games(pool, season_filter).await
        .inspect_err(|_| spinner.finish_and_clear())?;
    let total = pending_games.len();
    spinner.finish_and_clear();

    if total == 0 {
        println!("Backfill complete: 0 games pending (all already done)");
        return Ok(());
    }

    // Step 4: Set up progress bar
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} \u{2022} {per_sec} \u{2022} ETA {eta}"
        )
        .unwrap()
        .progress_chars("\u{2588}\u{2589}\u{258a}\u{258b}\u{258c}\u{258d}\u{258e}\u{258f}  "),
    );

    // Step 5: Sliding window — pre-fill up to MAX_CONCURRENT_GAMES, then collect one + spawn one
    let mut join_set: JoinSet<(i64, i32, time::Date, String, String, Result<usize, crate::AnyError>)> = JoinSet::new();

    // Track current season for per-season summaries
    let mut season_done: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    let mut season_failed: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();

    let mut total_done = 0usize;
    let mut total_failed = 0usize;

    let mut games_iter = pending_games.into_iter();

    // Helper closure to spawn one game task
    macro_rules! spawn_game {
        ($game:expr) => {{
            let game = $game;
            let game_id = game.game_id;
            let season = game.season;
            let game_date = game.game_date;
            let home_abbrev = game.home_abbrev.clone();
            let away_abbrev = game.away_abbrev.clone();
            let pool_clone = pool.clone();
            let map = team_id_map.clone();
            join_set.spawn(async move {
                let result = load_one_game(&pool_clone, game_id, &map).await;
                (game_id, season, game_date, home_abbrev, away_abbrev, result)
            });
        }};
    }

    // Pre-fill up to MAX_CONCURRENT_GAMES
    for game in (&mut games_iter).take(MAX_CONCURRENT_GAMES) {
        spawn_game!(game);
    }

    // Sliding window: collect one result, spawn one more, repeat
    while let Some(outcome) = join_set.join_next().await {
        match outcome {
            Ok((game_id, season, game_date, home_abbrev, away_abbrev, Ok(_count))) => {
                pb.suspend(|| println!("{}  {}  {} vs {}", game_date, game_id, home_abbrev, away_abbrev));
                update_progress_status(pool, game_id, "done").await
                    .unwrap_or_else(|e| pb.suspend(|| eprintln!("warn: checkpoint update failed for game {}: {}", game_id, e)));
                *season_done.entry(season).or_insert(0) += 1;
                total_done += 1;
            }
            Ok((game_id, season, game_date, home_abbrev, away_abbrev, Err(e))) => {
                pb.suspend(|| println!("{}  {}  {} vs {}  [FAILED]", game_date, game_id, home_abbrev, away_abbrev));
                pb.suspend(|| eprintln!("warn: game {} (season {}) failed: {}", game_id, season, e));
                update_progress_status(pool, game_id, "failed").await
                    .unwrap_or_else(|e2| pb.suspend(|| eprintln!("warn: checkpoint update failed for game {}: {}", game_id, e2)));
                *season_failed.entry(season).or_insert(0) += 1;
                total_failed += 1;
            }
            Err(join_err) => {
                pb.suspend(|| eprintln!("warn: task join error: {}", join_err));
                total_failed += 1;
            }
        }
        pb.inc(1);
        // Spawn the next game now that a slot is free
        if let Some(game) = games_iter.next() {
            spawn_game!(game);
        }
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
