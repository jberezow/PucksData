//! Diagnostic operator command — per-season health summary and optional gap repair.

use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SeasonReport {
    pub season: i32,
    pub completed_games: i64,
    pub games_with_events: i64,
    pub missing_event_games: i64,
    pub event_coverage_pct: f64,
    pub goals_missing_shots: i64,
    pub backfill_done: i64,
    pub backfill_failed: i64,
    pub backfill_skipped: i64,
    pub backfill_pending: i64,
    pub healthy: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DatasetSummary {
    pub last_sync_at: Option<time::OffsetDateTime>,
    pub last_sync_games: Option<i32>,
    pub latest_completed_game_date: Option<time::Date>,
    pub latest_event_game_date: Option<time::Date>,
    pub completed_games: i64,
    pub games_with_events: i64,
    pub missing_event_games: i64,
    pub goals_missing_shots: i64,
    pub backfill_failed: i64,
    pub backfill_pending: i64,
    pub backfill_skipped: i64,
    pub healthy: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub generated_at: time::OffsetDateTime,
    pub season_filter: Option<i32>,
    pub summary: DatasetSummary,
    pub seasons: Vec<SeasonReport>,
}

impl HealthReport {
    pub fn is_healthy(&self) -> bool {
        !self.seasons.is_empty() && self.seasons.iter().all(|season| season.healthy)
    }
}

pub async fn collect_health(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<HealthReport, crate::AnyError> {
    let summary = sqlx::query_as::<_, DatasetSummary>(
        "SELECT last_sync_at, last_sync_games, latest_completed_game_date, latest_event_game_date,
                completed_games, games_with_events, missing_event_games, goals_missing_shots,
                backfill_failed, backfill_pending, backfill_skipped, healthy
         FROM observability.dataset_health",
    )
    .fetch_one(pool)
    .await?;

    let seasons = sqlx::query_as::<_, SeasonReport>(
        "SELECT season, completed_games, games_with_events, missing_event_games,
                event_coverage_pct, goals_missing_shots, backfill_done, backfill_failed,
                backfill_skipped, backfill_pending, healthy
         FROM observability.season_health
         WHERE ($1::integer IS NULL OR season = $1)
         ORDER BY season",
    )
    .bind(season_filter)
    .fetch_all(pool)
    .await?;

    Ok(HealthReport {
        generated_at: time::OffsetDateTime::now_utc(),
        season_filter,
        summary,
        seasons,
    })
}

/// Run diagnostic queries and optionally fix coverage gaps.
/// Returns true if all in-scope seasons are healthy (no unprocessed OFF games).
pub async fn run_status(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
    fix: bool,
) -> Result<bool, crate::AnyError> {
    let report = collect_health(pool, season_filter).await?;
    print_status(&report, season_filter);
    let healthy = report.is_healthy();

    if fix {
        // Backfill goal events that predate their corresponding shots representation.
        let goals_missing: i64 = report
            .seasons
            .iter()
            .map(|season| season.goals_missing_shots)
            .sum();
        if goals_missing > 0 {
            println!("--fix: backfilling {goals_missing} goal(s) missing shots row...");
            backfill_goals_into_shots(pool, season_filter).await?;
        }

        // Re-backfill seasons where completed games have no events.
        let seasons_to_fix: Vec<i32> = report
            .seasons
            .iter()
            .filter(|season| season.missing_event_games > 0)
            .map(|r| r.season)
            .collect();

        if seasons_to_fix.is_empty() {
            if goals_missing == 0 {
                println!("--fix: all seasons already healthy, nothing to do.");
            }
        } else {
            if season_filter.is_none() {
                eprintln!(
                    "warn: --fix without --season will remediate {} season(s) with gaps: {:?}",
                    seasons_to_fix.len(),
                    seasons_to_fix
                );
            }
            for season in &seasons_to_fix {
                println!("Fixing season {season}...");
                fix_season(pool, *season).await?;
            }
        }
    }

    Ok(healthy)
}

pub async fn run_status_json(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<bool, crate::AnyError> {
    let report = collect_health(pool, season_filter).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report.is_healthy())
}

/// Insert a shots row for every goal that is missing one.
///
/// Safe to call repeatedly because conflicting event IDs are ignored.
async fn backfill_goals_into_shots(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<(), crate::AnyError> {
    let rows_inserted: u64 = sqlx::query!(
        r#"
        INSERT INTO shots (event_id, shooting_player_id, goalie_in_net_id, shot_type)
        SELECT go.event_id, go.scorer_player_id, go.goalie_id, go.shot_type
        FROM goals go
        JOIN events e ON e.id = go.event_id
        JOIN games g ON g.game_id = e.game_id
        WHERE NOT EXISTS (SELECT 1 FROM shots s WHERE s.event_id = go.event_id)
          AND ($1::integer IS NULL OR g.season = $1)
        ON CONFLICT (event_id) DO NOTHING
        "#,
        season_filter
    )
    .execute(pool)
    .await?
    .rows_affected();

    println!("--fix: inserted {rows_inserted} shots row(s) for previously orphaned goals.");
    Ok(())
}

/// Fetch game metadata and run backfill for a single season.
async fn fix_season(pool: &sqlx::PgPool, season: i32) -> Result<(), crate::AnyError> {
    let pb_fetch = crate::ui::make_progress_bar(0, "games fetched");
    let games = crate::fetchers::games::fetch_games_for_season_enriched(season, &pb_fetch).await;
    let count = games.len();
    pb_fetch.finish_and_clear();

    let pb_upsert = crate::ui::make_progress_bar(count as u64, "games written");
    crate::loaders::games::upsert_games(pool, &games, &pb_upsert)
        .await
        .inspect_err(|_| pb_upsert.finish_and_clear())?;
    pb_upsert.finish_and_clear();

    // Reset games that are marked done or skipped but still have no events.
    // seed_backfill_progress uses ON CONFLICT DO NOTHING, so stale 'done' rows
    // are never re-queued — force them back to 'pending' before backfill.
    // Excludes game_type = 1 (preseason) — preseason games have no API play-by-play
    // and would otherwise be reset and re-backfilled in an infinite no-op loop.
    sqlx::query!(
        "UPDATE backfill_progress
         SET status = 'pending', updated_at = NOW(), error_message = NULL
         WHERE season = $1
           AND status IN ('done', 'skipped')
           AND game_id IN (
               SELECT g.game_id
               FROM games g
               WHERE g.season = $1
                 AND g.game_type != 1
                 AND g.game_state NOT IN ('FUT', 'PRE')
                 AND NOT EXISTS (SELECT 1 FROM events e WHERE e.game_id = g.game_id)
           )",
        season
    )
    .execute(pool)
    .await?;

    crate::process::backfill::run_backfill(pool, Some(season)).await
}

/// Print the per-season health table to stdout.
fn print_status(report: &HealthReport, season_filter: Option<i32>) {
    if report.seasons.is_empty() {
        if let Some(s) = season_filter {
            println!("No completed (OFF/OVER/FINAL) games found for season {s}.");
        } else {
            println!("No completed (OFF/OVER/FINAL) games found in any season.");
        }
        return;
    }

    println!(
        "{:<12}  {:>12}  {:>14}  {:>10}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}",
        "Season",
        "Games (OFF+)",
        "Events Loaded",
        "Coverage%",
        "BP Done",
        "BP Fail",
        "BP Skip",
        "BP Pend",
        "Healthy?"
    );
    println!("{}", "-".repeat(110));

    for r in &report.seasons {
        let healthy_marker = if r.healthy { "yes" } else { "NO" };
        println!(
            "{:<12}  {:>12}  {:>14}  {:>9.1}%  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}",
            r.season,
            r.completed_games,
            r.games_with_events,
            r.event_coverage_pct,
            r.backfill_done,
            r.backfill_failed,
            r.backfill_skipped,
            r.backfill_pending,
            healthy_marker
        );
    }

    println!("{}", "-".repeat(110));
    let goals_missing_shots: i64 = report
        .seasons
        .iter()
        .map(|season| season.goals_missing_shots)
        .sum();
    if goals_missing_shots > 0 {
        println!(
            "WARNING: {goals_missing_shots} goal(s) have no corresponding shots row. \
             Run `sqlx migrate run` if migration 0007 has not been applied."
        );
    } else {
        println!("Goals-in-shots: OK (0 goals missing shots row)");
    }
}
