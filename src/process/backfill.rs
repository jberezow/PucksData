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
