use serde::Deserialize;

use crate::{api::{fetch_api_json, ApiError}, models::DbGame, AnyError};

// ── Stats API deserialization structs ────────────────────────────────────────

/// Generic paginated response wrapper for the NHL stats REST API.
#[derive(Deserialize)]
pub struct StatsApiResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
}

/// Stats /en/game endpoint record.
///
/// CRITICAL: NHL uses "visitingTeamId" / "visitingScore", NOT "awayTeamId" / "awayScore".
/// This struct is SEPARATE from BoxscoreGame — do not merge (Pitfall 5).
#[derive(Deserialize)]
pub struct StatsGameRecord {
    pub id: i64,
    pub season: i32,
    #[serde(rename = "gameDate")]
    pub game_date: String,            // "YYYY-MM-DD"
    #[serde(rename = "gameType")]
    pub game_type: i16,
    #[serde(rename = "homeTeamId")]
    pub home_team_id: i64,
    #[serde(rename = "visitingTeamId")]  // NOT "awayTeamId"
    pub away_team_id: i64,
    #[serde(rename = "homeScore")]
    pub home_score: Option<i16>,
    #[serde(rename = "visitingScore")]   // NOT "awayScore"
    pub away_score: Option<i16>,
}

// ── Boxscore API deserialization structs ─────────────────────────────────────

/// Localized name object used by the web API (e.g. venue, venueLocation).
#[derive(Deserialize)]
pub struct LocalizedName {
    pub default: String,
}

/// Team sub-object within a boxscore response.
#[derive(Deserialize)]
pub struct BoxscoreTeam {
    pub id: i64,
    pub score: Option<i16>,
}

/// Boxscore /v1/gamecenter/{id}/boxscore endpoint.
///
/// SEPARATE from StatsGameRecord — web API uses "awayTeam" / "homeTeam" nested objects
/// and "awayTeamId" does not appear at the top level here.
#[derive(Deserialize)]
pub struct BoxscoreGame {
    pub id: i64,
    #[serde(rename = "startTimeUTC")]
    pub start_time_utc: Option<String>,  // ISO 8601 UTC string
    #[serde(rename = "gameState")]
    pub game_state: Option<String>,
    pub venue: Option<LocalizedName>,
    #[serde(rename = "venueLocation")]
    pub venue_location: Option<LocalizedName>,
    #[serde(rename = "homeTeam")]
    pub home_team: BoxscoreTeam,
    #[serde(rename = "awayTeam")]
    pub away_team: BoxscoreTeam,
}

// ── Fetchers ─────────────────────────────────────────────────────────────────

/// Fetch all games for a season from the stats endpoint (paginates at limit=500).
pub async fn fetch_games_for_season(season_year: i32) -> Result<Vec<StatsGameRecord>, AnyError> {
    let mut all_games: Vec<StatsGameRecord> = Vec::new();
    let mut start: usize = 0;
    let limit: usize = 500;
    loop {
        let url = format!(
            "https://api.nhle.com/stats/rest/en/game?limit={limit}&start={start}&sort=gameId&dir=asc&cayenneExp=seasonId%3D{season_year}"
        );
        let json = fetch_api_json(&url).await?;
        let resp: StatsApiResponse<StatsGameRecord> = serde_json::from_str(&json)?;
        let batch_len = resp.data.len();
        all_games.extend(resp.data);
        if batch_len == 0 || all_games.len() >= resp.total as usize {
            break;
        }
        start += limit;
    }
    Ok(all_games)
}

/// Fetch the boxscore for a single game. On serde parse failure returns ApiError::Other(0).
pub async fn fetch_game_boxscore(game_id: i64) -> Result<BoxscoreGame, ApiError> {
    let url = format!("https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore");
    let json = fetch_api_json(&url).await?;
    serde_json::from_str(&json).map_err(|_e| ApiError::Other(0))
}

/// Fetch the list of all season year integers from the seasons endpoint (for --all mode).
pub async fn fetch_seasons_list() -> Result<Vec<i32>, AnyError> {
    let json = fetch_api_json("https://api-web.nhle.com/v1/season").await?;
    let years: Vec<i32> = serde_json::from_str(&json)?;
    Ok(years)
}

// ── Transform ─────────────────────────────────────────────────────────────────

/// Merge a stats record and an optional boxscore into a DbGame.
pub fn transform_game(stats: &StatsGameRecord, boxscore: Option<&BoxscoreGame>) -> Result<DbGame, AnyError> {
    use time::macros::format_description;
    let game_date = time::Date::parse(
        &stats.game_date,
        format_description!("[year]-[month]-[day]"),
    )?;

    let (start_time_utc, venue, venue_location, game_state) = match boxscore {
        Some(bs) => {
            let ts = bs.start_time_utc.as_deref()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
            let v = bs.venue.as_ref().map(|v| v.default.clone());
            let vl = bs.venue_location.as_ref().map(|v| v.default.clone());
            let gs = bs.game_state.clone();
            (ts, v, vl, gs)
        }
        None => (None, None, None, None),
    };

    Ok(DbGame {
        game_id: stats.id,
        season: stats.season,
        game_date,
        start_time_utc,
        home_team_id: stats.home_team_id,
        away_team_id: stats.away_team_id,
        game_type: stats.game_type,
        venue,
        venue_location,
        game_state,
        home_score: stats.home_score,
        away_score: stats.away_score,
    })
}

// ── Bulk enriched fetch ───────────────────────────────────────────────────────

const MAX_CONCURRENT_BOXSCORES: usize = 10;

/// Enumerate all games for a season, concurrently fetch their boxscores (10-permit semaphore),
/// transform and return DbGame records. Individual game errors skip + warn.
pub async fn fetch_games_for_season_enriched(
    season_year: i32,
    pb: &indicatif::ProgressBar,
) -> Vec<DbGame> {
    let stats_records = match fetch_games_for_season(season_year).await {
        Ok(r) => r,
        Err(e) => {
            pb.suspend(|| eprintln!("warn: failed to fetch games for season {season_year}: {e}"));
            return Vec::new();
        }
    };

    pb.set_length(stats_records.len() as u64);
    pb.set_message(format!("games (season {season_year:08})"));

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_BOXSCORES));
    let mut join_set: tokio::task::JoinSet<(StatsGameRecord, Option<BoxscoreGame>)> =
        tokio::task::JoinSet::new();

    for stats in stats_records {
        let sem = semaphore.clone();
        join_set.spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            let bs = match fetch_game_boxscore(stats.id).await {
                Ok(b) => Some(b),
                Err(e) => {
                    eprintln!("warn: boxscore fetch failed for game {}: {e:?}, using stats-only data", stats.id);
                    None
                }
            };
            (stats, bs)
        });
    }

    let mut games = Vec::new();
    while let Some(res) = join_set.join_next().await {
        pb.inc(1);
        match res {
            Ok((stats, bs)) => {
                match transform_game(&stats, bs.as_ref()) {
                    Ok(game) => games.push(game),
                    Err(e) => pb.suspend(|| eprintln!("warn: transform failed for game {}: {e}", stats.id)),
                }
            }
            Err(e) => pb.suspend(|| eprintln!("warn: task join error: {e}")),
        }
    }
    games
}

// ── Single game fetch ─────────────────────────────────────────────────────────

/// Fetch a single game by ID (for --game mode). Fetches stats + boxscore and transforms.
pub async fn fetch_single_game(game_id: i64) -> Result<DbGame, AnyError> {
    let url = format!(
        "https://api.nhle.com/stats/rest/en/game?cayenneExp=id%3D{game_id}"
    );
    let json = fetch_api_json(&url).await?;
    let resp: StatsApiResponse<StatsGameRecord> = serde_json::from_str(&json)?;
    let stats = resp.data.into_iter().next()
        .ok_or_else(|| format!("game {game_id} not found in stats API"))?;
    let bs = fetch_game_boxscore(game_id).await.ok();
    transform_game(&stats, bs.as_ref())
}
