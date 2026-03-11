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

#[derive(serde::Deserialize)]
struct LocalizedAbbrev {
    default: String,
}

#[derive(serde::Deserialize)]
struct StandingsTeam {
    #[serde(rename = "teamAbbrev")]
    team_abbrev: LocalizedAbbrev,
}

#[derive(serde::Deserialize)]
struct StandingsResponse {
    standings: Vec<StandingsTeam>,
}

#[derive(serde::Deserialize)]
struct RosterPlayer {
    id: i64,
}

#[derive(serde::Deserialize)]
struct RosterResponse {
    forwards: Vec<RosterPlayer>,
    defensemen: Vec<RosterPlayer>,
    goalies: Vec<RosterPlayer>,
}

/// Fetch abbreviations for all currently active NHL teams.
///
/// Uses the standings endpoint (`/v1/standings/now`) which always reflects
/// the 32 teams currently competing in the NHL season. This avoids the
/// `/stats/rest/en/team?cayenneExp=active=1` filter (HTTP 400 — that endpoint
/// has no `active` column) and the `/franchise` list (includes 40 entries:
/// historical defunct franchises such as the Brooklyn Americans and Hamilton
/// Tigers, plus relocated teams like the Arizona Coyotes).
async fn fetch_active_team_abbrevs() -> Result<Vec<String>, AnyError> {
    let json = fetch_api_json("https://api-web.nhle.com/v1/standings/now").await?;
    let resp: StandingsResponse = serde_json::from_str(&json)?;
    Ok(resp.standings.into_iter().map(|s| s.team_abbrev.default).collect())
}

/// Fetch all player IDs on a team's current roster.
async fn fetch_roster_player_ids(abbrev: &str) -> Result<Vec<i64>, AnyError> {
    let url = format!("https://api-web.nhle.com/v1/roster/{}/current", abbrev);
    let json = fetch_api_json(&url).await?;
    let roster: RosterResponse = serde_json::from_str(&json)?;
    let ids = roster.forwards.into_iter()
        .chain(roster.defensemen)
        .chain(roster.goalies)
        .map(|p| p.id)
        .collect();
    Ok(ids)
}

/// Paginate a stats summary endpoint (skater or goalie) for a given game type
/// and collect all player IDs.
async fn fetch_stats_player_ids(entity: &str, game_type: u8) -> Result<HashSet<i64>, AnyError> {
    let mut all_ids: HashSet<i64> = HashSet::new();
    let base_url = format!(
        "https://api.nhle.com/stats/rest/en/{}/summary?limit=100&start={{}}&sort=playerId&dir=asc&cayenneExp=gameTypeId%3D{}",
        entity, game_type
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

    Ok(all_ids)
}

/// Fetch all player IDs from two complementary sources and deduplicate:
///
/// 1. Current team rosters — catches all active players, including those who
///    haven't yet appeared in a game (injured, LTIR, rookies awaiting debut).
/// 2. Stats summaries (skater + goalie, regular season + playoffs) — catches
///    any player who has appeared in a game record but is no longer rostered.
///
/// The old single-shot `/stats/rest/en/players` endpoint was intentionally
/// removed: it returned ~2,600 historical all-time players but silently omitted
/// most active modern players (e.g. Connor McDavid, player_id 8478402).
pub async fn enumerate_player_ids() -> Result<Vec<i64>, AnyError> {
    let mut all_ids: HashSet<i64> = HashSet::new();

    // Source 1: current team rosters
    match fetch_active_team_abbrevs().await {
        Ok(teams) => {
            for abbrev in &teams {
                match fetch_roster_player_ids(abbrev).await {
                    Ok(ids) => { all_ids.extend(ids); }
                    Err(e) => eprintln!("warn: roster fetch failed for {}: {}", abbrev, e),
                }
            }
        }
        Err(e) => eprintln!("warn: active team fetch failed, skipping roster source: {}", e),
    }

    // Source 2: stats summaries (regular season + playoffs)
    for entity in ["skater", "goalie"] {
        for game_type in [2u8, 3u8] {
            match fetch_stats_player_ids(entity, game_type).await {
                Ok(ids) => { all_ids.extend(ids); }
                Err(e) => eprintln!("warn: stats player ids failed for {} type {}: {}", entity, game_type, e),
            }
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
