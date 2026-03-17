// src/process/status.rs
// Operator diagnostic: per-season health summary.
// run_status() returns true = healthy (no unprocessed OFF games), false = gaps exist.

/// Per-season health summary produced by the diagnostic queries.
pub struct SeasonReport {
    pub season: i32,
    pub total_off_games: i64,
    pub games_with_events: i64,
    pub coverage_pct: f64,
    pub goals_missing_shot: i64,
    pub bp_done: i64,
    pub bp_failed: i64,
    pub bp_skipped: i64,
    pub bp_pending: i64,
}

/// Run diagnostic queries and optionally fix coverage gaps.
/// Returns true if all in-scope seasons are healthy (no unprocessed OFF games).
/// `fix` is accepted here but orchestration logic is wired in Plan 02.
pub async fn run_status(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
    fix: bool,
) -> Result<bool, crate::AnyError> {
    // Query 1: game and event coverage per season
    let cov_rows = sqlx::query!(
        r#"
        SELECT
            g.season                                                        AS "season!: i32",
            COUNT(DISTINCT g.game_id)                                       AS "total_off_games!: i64",
            COUNT(DISTINCT e.game_id)                                       AS "games_with_events!: i64"
        FROM games g
        LEFT JOIN events e ON e.game_id = g.game_id
        WHERE g.game_state IN ('OFF', 'OVER', 'FINAL')
          AND ($1::integer IS NULL OR g.season = $1)
        GROUP BY g.season
        ORDER BY g.season
        "#,
        season_filter
    )
    .fetch_all(pool)
    .await?;

    // Query 2: backfill_progress status counts per season
    let bp_rows = sqlx::query!(
        r#"
        SELECT
            season                                                          AS "season!: i32",
            COUNT(*) FILTER (WHERE status = 'done')                        AS "bp_done!: i64",
            COUNT(*) FILTER (WHERE status = 'failed')                      AS "bp_failed!: i64",
            COUNT(*) FILTER (WHERE status = 'skipped')                     AS "bp_skipped!: i64",
            COUNT(*) FILTER (WHERE status = 'pending')                     AS "bp_pending!: i64"
        FROM backfill_progress
        WHERE ($1::integer IS NULL OR season = $1)
        GROUP BY season
        ORDER BY season
        "#,
        season_filter
    )
    .fetch_all(pool)
    .await?;

    // Query 3: goals missing a corresponding shots row (goals-in-shots coverage)
    let goals_missing: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)                                                     AS "count!: i64"
        FROM goals go
        JOIN events e ON e.id = go.event_id
        JOIN games g ON g.game_id = e.game_id
        WHERE NOT EXISTS (SELECT 1 FROM shots s WHERE s.event_id = go.event_id)
          AND ($1::integer IS NULL OR g.season = $1)
        "#,
        season_filter
    )
    .fetch_one(pool)
    .await?;

    // Merge cov_rows and bp_rows into SeasonReport vec (outer join on season)
    let reports: Vec<SeasonReport> = cov_rows.iter().map(|r| {
        let bp = bp_rows.iter().find(|b| b.season == r.season);
        let coverage_pct = if r.total_off_games > 0 {
            (r.games_with_events as f64 / r.total_off_games as f64) * 100.0
        } else {
            100.0
        };
        SeasonReport {
            season: r.season,
            total_off_games: r.total_off_games,
            games_with_events: r.games_with_events,
            coverage_pct,
            goals_missing_shot: 0, // filled below for the scoped or all-seasons row
            bp_done: bp.map(|b| b.bp_done).unwrap_or(0),
            bp_failed: bp.map(|b| b.bp_failed).unwrap_or(0),
            bp_skipped: bp.map(|b| b.bp_skipped).unwrap_or(0),
            bp_pending: bp.map(|b| b.bp_pending).unwrap_or(0),
        }
    }).collect();

    // Print the health table
    print_status(&reports, goals_missing, season_filter);

    // Determine health: any season with OFF games that have no events = unhealthy
    let healthy = reports.iter().all(|r| r.games_with_events >= r.total_off_games);

    if fix {
        // Collect seasons that need remediation
        let seasons_to_fix: Vec<i32> = reports.iter()
            .filter(|r| r.games_with_events < r.total_off_games)
            .map(|r| r.season)
            .collect();

        if seasons_to_fix.is_empty() {
            println!("--fix: all seasons already healthy, nothing to do.");
        } else {
            if season_filter.is_none() {
                eprintln!(
                    "warn: --fix without --season will remediate {} season(s) with gaps: {:?}",
                    seasons_to_fix.len(),
                    seasons_to_fix
                );
            }
            for season in &seasons_to_fix {
                println!("Fixing season {}...", season);
                fix_season(pool, *season).await?;
            }
        }
    }

    Ok(healthy)
}

/// Fetch game metadata and run backfill for a single season.
/// Delegates entirely to existing functions — no new fetch/load logic.
/// Called from run_status() when fix=true and the season has coverage gaps.
async fn fix_season(pool: &sqlx::PgPool, season: i32) -> Result<(), crate::AnyError> {
    // Step 1: Refresh game metadata for this season
    let pb_fetch = crate::ui::make_progress_bar(0, "games fetched");
    let games = crate::fetchers::games::fetch_games_for_season_enriched(season, &pb_fetch).await;
    let count = games.len();
    pb_fetch.finish_and_clear();

    // Step 2: Upsert the fetched games (ON CONFLICT — idempotent)
    let pb_upsert = crate::ui::make_progress_bar(count as u64, "games written");
    crate::loaders::games::upsert_games(pool, &games, &pb_upsert).await
        .inspect_err(|_| pb_upsert.finish_and_clear())?;
    pb_upsert.finish_and_clear();

    // Step 3: Backfill events for this season (seed + process pending)
    crate::process::backfill::run_backfill(pool, Some(season)).await
}

/// Print the per-season health table to stdout.
fn print_status(reports: &[SeasonReport], goals_missing_shot: i64, season_filter: Option<i32>) {
    if reports.is_empty() {
        if let Some(s) = season_filter {
            println!("No completed (OFF/OVER/FINAL) games found for season {}.", s);
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

    for r in reports {
        let healthy_marker = if r.games_with_events >= r.total_off_games { "yes" } else { "NO" };
        println!(
            "{:<12}  {:>12}  {:>14}  {:>9.1}%  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}",
            r.season,
            r.total_off_games,
            r.games_with_events,
            r.coverage_pct,
            r.bp_done,
            r.bp_failed,
            r.bp_skipped,
            r.bp_pending,
            healthy_marker
        );
    }

    println!("{}", "-".repeat(110));
    if goals_missing_shot > 0 {
        println!(
            "WARNING: {} goal(s) have no corresponding shots row. \
             Run `sqlx migrate run` if migration 0007 has not been applied.",
            goals_missing_shot
        );
    } else {
        println!("Goals-in-shots: OK (0 goals missing shots row)");
    }
}
