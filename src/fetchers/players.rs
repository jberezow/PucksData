use std::collections::HashSet;
use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::{api::{fetch_api_json, ApiError}, models::DbPlayer, AnyError};

const MAX_CONCURRENT_PLAYERS: usize = 20;

// ── Deserialization structs ──────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct LocalizedString {
    pub default: String,
}

#[derive(serde::Deserialize)]
pub struct DraftDetails {
    pub year: Option<i16>,
    #[serde(rename = "teamAbbrev")]
    pub team_abbrev: Option<String>,
    pub round: Option<i16>,
    #[serde(rename = "pickInRound")]
    pub pick_in_round: Option<i16>,
    #[serde(rename = "overallPick")]
    pub overall_pick: Option<i16>,
}

#[derive(serde::Deserialize)]
pub struct PlayerLanding {
    #[serde(rename = "playerId")]
    pub player_id: i64,
    #[serde(rename = "firstName")]
    pub first_name: LocalizedString,
    #[serde(rename = "lastName")]
    pub last_name: LocalizedString,
    pub position: Option<String>,
    #[serde(rename = "shootsCatches")]
    pub shoots_catches: Option<String>,
    #[serde(rename = "currentTeamAbbrev")]
    pub current_team_abbrev: Option<String>,
    #[serde(rename = "birthDate")]
    pub birth_date: Option<String>,
    #[serde(rename = "heightInCentimeters")]
    pub height_cm: Option<i16>,
    #[serde(rename = "weightInKilograms")]
    pub weight_kg: Option<i16>,
    #[serde(rename = "draftDetails")]
    pub draft_details: Option<DraftDetails>,
}

// ── ID enumeration helpers ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct PlayerIdRecord {
    #[serde(rename = "playerId")]
    player_id: i64,
}

#[derive(serde::Deserialize)]
struct ApiResponse<T> {
    data: Vec<T>,
    total: Option<i64>,
}

/// Fetch all player IDs from the NHL stats REST API.
///
/// First tries the `limit=-1` single-shot endpoint.  If that returns zero
/// records (the endpoint's behaviour is unconfirmed) it falls back to
/// paginated skater + goalie summary endpoints, deduplicates, and sorts.
pub async fn enumerate_player_ids() -> Result<Vec<i64>, AnyError> {
    // Try single-shot endpoint first
    let url = "https://api.nhle.com/stats/rest/en/players?limit=-1&sort=playerId&dir=asc";
    let json = fetch_api_json(url).await?;
    let resp: ApiResponse<PlayerIdRecord> = serde_json::from_str(&json)?;

    if !resp.data.is_empty() {
        let ids: Vec<i64> = resp.data.into_iter().map(|r| r.player_id).collect();
        return Ok(ids);
    }

    // Fallback: paginate skater + goalie summary endpoints
    let mut all_ids: HashSet<i64> = HashSet::new();

    for entity in ["skater", "goalie"] {
        let base_url = format!(
            "https://api.nhle.com/stats/rest/en/{}/summary?limit=100&start={{}}&sort=playerId&dir=asc&cayenneExp=gameTypeId%3D2",
            entity
        );

        let first_url = base_url.replace("{}", "0");
        let first_json = fetch_api_json(&first_url).await?;
        let first_resp: ApiResponse<PlayerIdRecord> = serde_json::from_str(&first_json)?;
        let total = first_resp.total.unwrap_or(0) as usize;

        for r in first_resp.data {
            all_ids.insert(r.player_id);
        }

        let mut offset = 100usize;
        while all_ids.len() < total && offset < total {
            let page_url = base_url.replace("{}", &offset.to_string());
            let page_json = fetch_api_json(&page_url).await?;
            let page_resp: ApiResponse<PlayerIdRecord> = serde_json::from_str(&page_json)?;
            if page_resp.data.is_empty() {
                break;
            }
            for r in page_resp.data {
                all_ids.insert(r.player_id);
            }
            offset += 100;
        }
    }

    let mut ids: Vec<i64> = all_ids.into_iter().collect();
    ids.sort();
    Ok(ids)
}

// ── Landing page fetch ───────────────────────────────────────────────────────

async fn fetch_player_landing(id: i64) -> Result<PlayerLanding, ApiError> {
    let url = format!("https://api-web.nhle.com/v1/player/{}/landing", id);
    let json = fetch_api_json(&url).await?;
    let landing: PlayerLanding = serde_json::from_str(&json)
        .map_err(|_e| ApiError::Other(500))?;
    Ok(landing)
}

fn landing_to_db(landing: PlayerLanding) -> DbPlayer {
    let birth_date = landing.birth_date.as_deref().and_then(|s| {
        let fmt = time::format_description::well_known::Iso8601::DEFAULT;
        match time::Date::parse(s, &fmt) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("warn: could not parse birth_date '{}': {}", s, e);
                None
            }
        }
    });

    let draft_year = landing.draft_details.as_ref().and_then(|d| d.year);
    let draft_round = landing.draft_details.as_ref().and_then(|d| d.round);
    let draft_pick = landing.draft_details.as_ref().and_then(|d| d.pick_in_round);
    let draft_team_abbrev = landing.draft_details.as_ref().and_then(|d| d.team_abbrev.clone());
    let draft_overall_pick = landing.draft_details.as_ref().and_then(|d| d.overall_pick);

    DbPlayer {
        player_id: landing.player_id,
        first_name: landing.first_name.default,
        last_name: landing.last_name.default,
        position: landing.position,
        shoots_catches: landing.shoots_catches,
        current_team_abbrev: landing.current_team_abbrev,
        birth_date,
        height_cm: landing.height_cm,
        weight_kg: landing.weight_kg,
        draft_year,
        draft_round,
        draft_pick,
        draft_team_abbrev,
        draft_overall_pick,
    }
}

/// Concurrently fetch all player landing pages (bounded at MAX_CONCURRENT_PLAYERS).
pub async fn fetch_all_players(player_ids: Vec<i64>, pb: &ProgressBar) -> Vec<DbPlayer> {
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_PLAYERS));
    let mut join_set: JoinSet<Option<DbPlayer>> = JoinSet::new();

    for id in player_ids {
        let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
        join_set.spawn(async move {
            let _permit = permit; // released when task completes
            match fetch_player_landing(id).await {
                Ok(landing) => Some(landing_to_db(landing)),
                Err(ApiError::NotFound) => {
                    eprintln!("warn: player {} not found, skipping", id);
                    None
                }
                Err(e) => {
                    eprintln!("warn: player {} error: {}, skipping", id, e);
                    None
                }
            }
        });
    }

    let mut results = Vec::new();
    while let Some(outcome) = join_set.join_next().await {
        pb.inc(1);
        if let Ok(Some(player)) = outcome {
            results.push(player);
        }
    }
    results
}

/// Public entry point: enumerate all player IDs, fetch all landing pages, return DbPlayer records.
pub async fn fetch_players() -> Result<Vec<DbPlayer>, AnyError> {
    let player_ids = enumerate_player_ids().await?;
    let total = player_ids.len() as u64;

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} players")
            .unwrap()
            .progress_chars("#>-"),
    );

    let records = fetch_all_players(player_ids, &pb).await;
    pb.finish_with_message(format!("Fetched {} players", records.len()));

    Ok(records)
}
