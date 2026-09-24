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
    pub acknowledged_gap_games: i64,
    pub actionable_gap_games: i64,
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
    pub acknowledged_gap_games: i64,
    pub actionable_gap_games: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IngestionIssue {
    pub dataset: String,
    pub entity_key: String,
    pub outcome: String,
    pub last_attempt_at: time::OffsetDateTime,
    pub last_success_at: Option<time::OffsetDateTime>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub generated_at: time::OffsetDateTime,
    pub season_filter: Option<i32>,
    pub summary: DatasetSummary,
    pub seasons: Vec<SeasonReport>,
    pub ingestion_issues: Vec<IngestionIssue>,
}

impl HealthReport {
    pub fn is_healthy(&self) -> bool {
        !self.seasons.is_empty()
            && self.seasons.iter().all(|season| season.healthy)
            && self.ingestion_issues.is_empty()
    }
}

pub async fn collect_health(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<HealthReport, crate::AnyError> {
    let mut summary = sqlx::query_as::<_, DatasetSummary>(
        "SELECT last_sync_at, last_sync_games, latest_completed_game_date, latest_event_game_date,
                completed_games, games_with_events, missing_event_games, goals_missing_shots,
                backfill_failed, backfill_pending, backfill_skipped, healthy,
                acknowledged_gap_games, actionable_gap_games
         FROM observability.dataset_health_live",
    )
    .fetch_one(pool)
    .await?;

    let seasons = sqlx::query_as::<_, SeasonReport>(
        "SELECT season, completed_games, games_with_events, missing_event_games,
                event_coverage_pct, goals_missing_shots, backfill_done, backfill_failed,
                backfill_skipped, backfill_pending, healthy,
                acknowledged_gap_games, actionable_gap_games
         FROM observability.season_health_live
         WHERE ($1::integer IS NULL OR season = $1)
         ORDER BY season",
    )
    .bind(season_filter)
    .fetch_all(pool)
    .await?;

    let ingestion_issues = sqlx::query_as::<_, IngestionIssue>(
        "SELECT dataset, entity_key, outcome, last_attempt_at, last_success_at, error_message
         FROM observability.ingestion_freshness f
         WHERE (outcome IN ('failed','partial') OR (outcome='running' AND last_attempt_at < NOW() - interval '2 hours'))
           AND dataset IN ('sync','schedule','players_rosters','player_repair','events','official_games','shifts','derived')
           AND ($1::integer IS NULL OR (dataset IN ('events','official_games','shifts')
               AND EXISTS (SELECT 1 FROM games g WHERE g.game_id::text=f.entity_key AND g.season=$1)))
         ORDER BY last_attempt_at DESC LIMIT 100")
        .bind(season_filter).fetch_all(pool).await?;
    summary.healthy &= ingestion_issues.is_empty();

    Ok(HealthReport {
        generated_at: time::OffsetDateTime::now_utc(),
        season_filter,
        summary,
        seasons,
        ingestion_issues,
    })
}

/// Run diagnostic queries and optionally fix coverage gaps.
/// Returns true if all in-scope seasons have strict event coverage and consistency.
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
            .filter(|season| season.actionable_gap_games > 0)
            .map(|r| r.season)
            .collect();

        if seasons_to_fix.is_empty() {
            if goals_missing == 0 {
                println!("--fix: no actionable gaps found, nothing to do.");
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

    if fix {
        crate::process::analytics::refresh_derived(pool).await?;
        return Ok(collect_health(pool, season_filter).await?.is_healthy());
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
    let games: Vec<i64> = sqlx::query_scalar("SELECT DISTINCT e.game_id FROM goals g JOIN events e ON e.id = g.event_id
        WHERE ($1::integer IS NULL OR e.season = $1) AND NOT EXISTS (SELECT 1 FROM shots s WHERE s.event_id = e.id)")
        .bind(season_filter).fetch_all(pool).await?;
    let mut rows_inserted = 0;
    for game_id in games {
        let mut tx = pool.begin().await?;
        crate::loaders::history::lock_game(&mut tx, game_id).await?;
        rows_inserted += sqlx::query("INSERT INTO shots (event_id, shooting_player_id, goalie_in_net_id, shot_type)
            SELECT g.event_id, g.scorer_player_id, g.goalie_id, g.shot_type FROM goals g
            JOIN events e ON e.id = g.event_id WHERE e.game_id = $1 ON CONFLICT (event_id) DO NOTHING")
            .bind(game_id).execute(&mut *tx).await?.rows_affected();
        crate::loaders::history::events(&mut tx, game_id).await?;
        tx.commit().await?;
    }

    println!("--fix: inserted {rows_inserted} shots row(s) for previously orphaned goals.");
    Ok(())
}

/// Fetch game metadata and run backfill for a single season.
async fn fix_season(pool: &sqlx::PgPool, season: i32) -> Result<(), crate::AnyError> {
    let pb_fetch = crate::ui::make_progress_bar(0, "games fetched");
    let games = crate::fetchers::games::fetch_games_for_season_enriched(season, &pb_fetch).await?;
    let count = games.len();
    pb_fetch.finish_and_clear();

    let pb_upsert = crate::ui::make_progress_bar(count as u64, "games written");
    crate::loaders::games::upsert_games(pool, &games, &pb_upsert)
        .await
        .inspect_err(|_| pb_upsert.finish_and_clear())?;
    pb_upsert.finish_and_clear();

    crate::process::backfill::run_backfill(pool, Some(season)).await
}

/// Print the per-season health table to stdout.
fn print_status(report: &HealthReport, season_filter: Option<i32>) {
    for issue in &report.ingestion_issues {
        println!(
            "ingestion {} {}: {} ({})",
            issue.dataset,
            issue.entity_key,
            issue.outcome,
            issue
                .error_message
                .as_deref()
                .unwrap_or("unfinished for more than two hours")
        );
    }
    if report.seasons.is_empty() {
        if let Some(s) = season_filter {
            println!("No completed (OFF/OVER/FINAL) games found for season {s}.");
        } else {
            println!("No completed (OFF/OVER/FINAL) games found in any season.");
        }
        return;
    }

    println!(
        "{:<12}  {:>12}  {:>14}  {:>10}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}",
        "Season",
        "Games (OFF+)",
        "Events Loaded",
        "Coverage%",
        "BP Done",
        "BP Fail",
        "BP Skip",
        "BP Pend",
        "Known",
        "Open",
        "Healthy?"
    );
    println!("{}", "-".repeat(130));

    for r in &report.seasons {
        let healthy_marker = if r.healthy { "yes" } else { "NO" };
        println!(
            "{:<12}  {:>12}  {:>14}  {:>9.1}%  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}",
            r.season,
            r.completed_games,
            r.games_with_events,
            r.event_coverage_pct,
            r.backfill_done,
            r.backfill_failed,
            r.backfill_skipped,
            r.backfill_pending,
            r.acknowledged_gap_games,
            r.actionable_gap_games,
            healthy_marker
        );
    }

    println!("{}", "-".repeat(130));
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
