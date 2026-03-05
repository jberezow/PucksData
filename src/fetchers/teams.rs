use std::collections::HashMap;
use crate::{api::fetch_api_json, models::DbTeam, AnyError};
use indicatif::{ProgressBar, ProgressStyle};

#[derive(serde::Deserialize)]
struct FranchiseRecord {
    id: i64,
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(rename = "teamCommonName")]
    common_name: String,
    #[serde(rename = "teamPlaceName")]
    place_name: String,
}

#[derive(serde::Deserialize)]
struct TeamAbbrevRecord {
    id: i64,
    #[serde(rename = "franchiseId")]
    franchise_id: Option<i64>,  // null for placeholder entries (e.g. "TBD", "NHL")
    #[serde(rename = "triCode")]
    tri_code: String,
}

#[derive(serde::Deserialize)]
struct ApiResponse<T> {
    data: Vec<T>,
}

pub async fn fetch_teams() -> Result<Vec<DbTeam>, AnyError> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Fetching NHL teams...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Fetch franchise data
    let franchise_json = fetch_api_json(
        "https://api.nhle.com/stats/rest/en/franchise?limit=-1",
    )
    .await?;
    let franchise_resp: ApiResponse<FranchiseRecord> =
        serde_json::from_str(&franchise_json)?;

    pb.set_message("Fetching team abbreviations...");

    // Fetch team abbreviation data
    let abbrev_json = fetch_api_json(
        "https://api.nhle.com/stats/rest/en/team?limit=-1",
    )
    .await?;
    let abbrev_resp: ApiResponse<TeamAbbrevRecord> =
        serde_json::from_str(&abbrev_json)?;

    // Build franchiseId -> triCode map.
    // The /team endpoint has one row per franchise *era* (e.g. Quebec Nordiques and
    // Colorado Avalanche both share franchiseId=27). Keep the entry with the highest
    // team id, which corresponds to the most recent/current name and triCode.
    let mut abbrev_map: HashMap<i64, (i64, String)> = HashMap::new();
    for r in abbrev_resp.data {
        // Skip placeholder entries (e.g. "TBD", "NHL") that have no franchise association.
        let Some(franchise_id) = r.franchise_id else { continue };
        let entry = abbrev_map.entry(franchise_id).or_insert((i64::MIN, String::new()));
        if r.id > entry.0 {
            *entry = (r.id, r.tri_code);
        }
    }
    let abbrev_map: HashMap<i64, String> = abbrev_map
        .into_iter()
        .map(|(fid, (_, code))| (fid, code))
        .collect();

    // Join franchises with abbreviations
    let mut teams = Vec::new();
    for franchise in franchise_resp.data {
        match abbrev_map.get(&franchise.id) {
            Some(abbrev) => {
                teams.push(DbTeam {
                    team_id: franchise.id,
                    full_name: franchise.full_name,
                    common_name: franchise.common_name,
                    place_name: franchise.place_name,
                    abbrev: abbrev.clone(),
                });
            }
            None => {
                eprintln!(
                    "Warning: franchise id={} '{}' has no triCode match — skipping",
                    franchise.id, franchise.full_name
                );
            }
        }
    }

    let count = teams.len();
    pb.finish_with_message(format!("Fetched {} teams", count));

    Ok(teams)
}
