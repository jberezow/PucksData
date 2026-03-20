//! Fetches the list of all season IDs from the NHL stats API.
use crate::{api::fetch_api_json, models::DbSeason, AnyError};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
#[allow(non_snake_case)]
struct StatsSeasonRecord {
    seasonId: i32,
    startDate: Option<String>,
    endDate: Option<String>,
    regularSeasonEndDate: Option<String>,
}

#[derive(serde::Deserialize)]
struct StatsSeasonResponse {
    data: Vec<StatsSeasonRecord>,
}

fn parse_date(s: &str) -> Option<time::Date> {
    use time::macros::format_description;
    let fmt = format_description!("[year]-[month]-[day]");
    time::Date::parse(s, fmt).ok()
}

/// Fetch all NHL season IDs from the stats API.
pub async fn fetch_seasons() -> Result<Vec<DbSeason>, AnyError> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Fetching NHL seasons...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Fetch the seasons list from web API
    let seasons_json = fetch_api_json("https://api-web.nhle.com/v1/season").await?;
    let season_years: Vec<i32> = serde_json::from_str(&seasons_json)?;

    pb.set_message("Fetching season date data...");

    // Attempt to fetch enriched date data from stats API
    let stats_map: HashMap<i32, StatsSeasonRecord> = match fetch_api_json(
        "https://api.nhle.com/stats/rest/en/season?limit=-1",
    )
    .await
    {
        Ok(json) => match serde_json::from_str::<StatsSeasonResponse>(&json) {
            Ok(resp) => resp.data.into_iter().map(|r| (r.seasonId, r)).collect(),
            Err(e) => {
                eprintln!("Warning: failed to parse stats season data: {e} — dates will be None");
                HashMap::new()
            }
        },
        Err(crate::api::ApiError::NotFound) => {
            eprintln!("Warning: stats season endpoint returned 404 — dates will be None");
            HashMap::new()
        }
        Err(e) => {
            eprintln!("Warning: stats season endpoint error: {e} — dates will be None");
            HashMap::new()
        }
    };

    // Build DbSeason records
    let mut seasons = Vec::new();
    for season_year in season_years {
        let stats = stats_map.get(&season_year);
        let start_date = stats.and_then(|s| s.startDate.as_deref()).and_then(parse_date);
        let end_date = stats.and_then(|s| s.endDate.as_deref()).and_then(parse_date);
        let regular_season_end_date = stats
            .and_then(|s| s.regularSeasonEndDate.as_deref())
            .and_then(parse_date);

        seasons.push(DbSeason {
            season_year,
            start_date,
            end_date,
            regular_season_end_date,
        });
    }

    let count = seasons.len();
    pb.finish_with_message(format!("Fetched {count} seasons"));

    Ok(seasons)
}
