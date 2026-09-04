//! Loads official NHL season totals for every season in the database.

use crate::fetchers::official_stats::{
    fetch_goalie_season, fetch_skater_season, OFFICIAL_GAME_TYPES,
};
use crate::loaders::official_stats::{upsert_goalie_seasons, upsert_skater_seasons};

/// Outcome of one official-stats run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OfficialStatsSummary {
    pub seasons: usize,
    pub skater_rows: usize,
    pub goalie_rows: usize,
    pub failures: usize,
}

/// Seasons to load, newest first so an interrupted run leaves the most
/// useful data in place.
pub async fn query_seasons(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT season_year FROM seasons
         WHERE ($1::integer IS NULL OR season_year = $1::integer)
         ORDER BY season_year DESC",
    )
    .bind(season_filter)
    .fetch_all(pool)
    .await
}

/// Fetch and store official skater and goalie totals.
///
/// One season and game type that fails is reported and skipped rather than
/// aborting the run; the stats API omits summaries for some historical
/// season and game-type combinations.
pub async fn run_official_stats(
    pool: &sqlx::PgPool,
    season_filter: Option<i32>,
) -> Result<OfficialStatsSummary, crate::AnyError> {
    let seasons = query_seasons(pool, season_filter).await?;
    let mut summary = OfficialStatsSummary {
        seasons: seasons.len(),
        ..Default::default()
    };

    let pb = crate::ui::make_progress_bar(
        (seasons.len() * OFFICIAL_GAME_TYPES.len()) as u64,
        "season/type",
    );

    for season in seasons {
        for game_type in OFFICIAL_GAME_TYPES {
            match fetch_skater_season(season, game_type).await {
                Ok(rows) => summary.skater_rows += upsert_skater_seasons(pool, &rows).await?,
                Err(error) => {
                    summary.failures += 1;
                    pb.suspend(|| {
                        eprintln!("warn: skater summary {season} type {game_type}: {error}")
                    });
                }
            }

            match fetch_goalie_season(season, game_type).await {
                Ok(rows) => summary.goalie_rows += upsert_goalie_seasons(pool, &rows).await?,
                Err(error) => {
                    summary.failures += 1;
                    pb.suspend(|| {
                        eprintln!("warn: goalie summary {season} type {game_type}: {error}")
                    });
                }
            }

            pb.inc(1);
        }
    }

    pb.finish_and_clear();
    println!(
        "Official stats: {} seasons, {} skater rows, {} goalie rows, {} failures",
        summary.seasons, summary.skater_rows, summary.goalie_rows, summary.failures
    );

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_starts_empty() {
        let summary = OfficialStatsSummary::default();
        assert_eq!(summary.skater_rows, 0);
        assert_eq!(summary.goalie_rows, 0);
        assert_eq!(summary.failures, 0);
    }

    #[test]
    fn official_game_types_cover_regular_season_and_playoffs() {
        assert_eq!(OFFICIAL_GAME_TYPES, [2, 3]);
    }
}
