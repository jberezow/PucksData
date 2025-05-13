use crate::api;
use crate::cache;
use std::collections::HashMap;
use std::path::PathBuf;
use crate::api_urls;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataType {
    Games,
    Players,
    Skaters,
    Goalies,
    Teams,
    Schedule,
    Playoffs,
    Seasons,
    Draft,
}

impl DataType {
    fn as_str(&self) -> &'static str {
        match self {
            DataType::Games => "games",
            DataType::Players => "players",
            DataType::Skaters => "skaters",
            DataType::Goalies => "goalies",
            DataType::Teams => "teams",
            DataType::Schedule => "schedule",
            DataType::Playoffs => "playoffs",
            DataType::Seasons => "seasons",
            DataType::Draft => "draft",
        }
    }
}

#[derive(Default)]
pub struct ApiParams {
    params: HashMap<String, String>,
}

impl ApiParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn add_param(&mut self, key: &str, value: &str) -> &mut Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }
}

fn fetch_and_cache(data_type: DataType, endpoint: &str, url_template: &str, params: &ApiParams) -> Result<(), Box<dyn std::error::Error>> {
    let mut file_path = PathBuf::from("data/raw");
    file_path.push(data_type.as_str());

    // Create parameter-specific subdirectories based on endpoint type
    match (data_type, endpoint) {
        // Schedule endpoints
        (DataType::Schedule, "date") => {
            if let Some(date) = params.get_param("date") {
                file_path.push(date);
            }
        },
        (DataType::Schedule, "calendar_date") => {
            if let Some(date) = params.get_param("date") {
                file_path.push(date);
            }
        },

        // Game endpoints with dates
        (DataType::Games, "scores_date") => {
            if let Some(date) = params.get_param("date") {
                file_path.push(date);
            }
        },
        (DataType::Games, "tv_date") => {
            if let Some(date) = params.get_param("date") {
                file_path.push(date);
            }
        },

        // Team endpoints - always team_code first, then other parameters
        (DataType::Teams, _) => {
            // Always put team_code at the top level for team endpoints
            if let Some(team_code) = params.get_param("team_code") {
                file_path.push(team_code);
                
                // Then add season or date as a subdirectory if available
                match endpoint {
                    "roster_season" => {
                        if let Some(season) = params.get_param("season") {
                            file_path.push(season);
                        }
                    },
                    "standings_date" => {
                        if let Some(date) = params.get_param("date") {
                            file_path.push(date);
                        }
                    },
                    "standings_season" => {
                        if let Some(season) = params.get_param("season") {
                            file_path.push(season);
                        }
                    },
                    "schedule_season" => {
                        if let Some(season) = params.get_param("season") {
                            file_path.push(season);
                        }
                    },
                    "schedule_month" => {
                        if let Some(date) = params.get_param("date") {
                            file_path.push(date);
                        }
                    },
                    "season_stats" => {
                        if let Some(season) = params.get_param("season") {
                            file_path.push(season);
                            if let Some(game_type) = params.get_param("game_type") {
                                file_path.push(game_type);
                            }
                        }
                    },
                    _ => {}
                }
            } else if endpoint == "standings_now" || endpoint == "standings_date" || endpoint == "standings_season" {
                // Standings endpoints that don't have team_code
                match endpoint {
                    "standings_date" => {
                        if let Some(date) = params.get_param("date") {
                            file_path.push(date);
                        }
                    },
                    "standings_season" => {
                        if let Some(season) = params.get_param("season") {
                            file_path.push(season);
                        }
                    },
                    _ => {}
                }
            }
        },

        // Player endpoints with seasons
        (DataType::Players, "game_log") => {
            if let Some(player_id) = params.get_param("player_id") {
                file_path.push(player_id);
            }
            if let Some(season) = params.get_param("season") {
                file_path.push(season);
            }
            if let Some(game_type) = params.get_param("game_type") {
                file_path.push(game_type);
            }
        },

        // Skater and Goalie endpoints with seasons
        (DataType::Skaters, "leaders") | (DataType::Goalies, "leaders") => {
            if let Some(season) = params.get_param("season") {
                file_path.push(season);
                if let Some(game_type) = params.get_param("game_type") {
                    file_path.push(game_type);
                }
            }
        },

        // Draft endpoints with years
        (DataType::Draft, "picks") => {
            if let Some(year) = params.get_param("year") {
                file_path.push(year);
            }
        },

        // For IDs (game_id, player_id), create a subdirectory
        _ => {
            if let Some(id) = params.get_param("game_id")
                .or_else(|| params.get_param("player_id")) {
                file_path.push(id);
            }
        }
    }

    std::fs::create_dir_all(&file_path)?;
    file_path.push(format!("{}.json", endpoint));

    if let Some(_) = cache::read_from_cache(&file_path) {
        println!("✅ Found cached {} data at {:?}", endpoint, file_path);
        return Ok(());
    }

    println!("🌐 Fetching {} data from NHL API...", endpoint);

    let mut url = url_template.to_string();
    for (key, value) in &params.params {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    
    match api::fetch_api_json(&url) {
        Ok(json) => {
            cache::write_to_cache(&file_path, &json)?;
            println!("💾 Saved {} data to {:?}", endpoint, file_path);
            Ok(())
        }
        Err(api::ApiError::NotFound) => {
            // Clean up the parameter-specific directory if it exists and is empty
            if file_path.parent().map_or(false, |p| p.exists()) {
                if let Ok(entries) = std::fs::read_dir(file_path.parent().unwrap()) {
                    if entries.count() == 0 {
                        let _ = std::fs::remove_dir(file_path.parent().unwrap());
                    }
                }
            }
            println!("❌ Resource not found at {}", url);
            Err("Resource not found (404)".into())
        }
        Err(api::ApiError::NetworkError(e)) => {
            // Clean up the parameter-specific directory if it exists and is empty
            if file_path.parent().map_or(false, |p| p.exists()) {
                if let Ok(entries) = std::fs::read_dir(file_path.parent().unwrap()) {
                    if entries.count() == 0 {
                        let _ = std::fs::remove_dir(file_path.parent().unwrap());
                    }
                }
            }
            println!("❌ Network error while fetching {}: {}", url, e);
            Err(Box::new(e))
        }
        Err(api::ApiError::Other(code)) => {
            // Clean up the parameter-specific directory if it exists and is empty
            if file_path.parent().map_or(false, |p| p.exists()) {
                if let Ok(entries) = std::fs::read_dir(file_path.parent().unwrap()) {
                    if entries.count() == 0 {
                        let _ = std::fs::remove_dir(file_path.parent().unwrap());
                    }
                }
            }
            println!("❌ HTTP error {} while fetching {}", code, url);
            Err(format!("HTTP error: {}", code).into())
        }
    }
}

pub fn fetch_game_story(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "story", api_urls::GAME_STORY_API_URL, &params)
}

pub fn fetch_game_boxscore(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "boxscore", api_urls::GAME_BOXSCORE_API_URL, &params)
}

pub fn fetch_game_play_by_play(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "playbyplay", api_urls::GAME_PLAY_BY_PLAY_API_URL, &params)
}

pub fn fetch_game_all_games() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Games, "all", api_urls::GAME_ALL_GAMES_API_URL, &params)
}

pub fn fetch_game_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Games, "metadata", api_urls::GAME_ALL_METADATA_API_URL, &params)
}

pub fn fetch_player_summary(player_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("player_id", player_id);
    fetch_and_cache(DataType::Players, "summary", api_urls::PLAYER_SUMMARY_API_URL, &params)
}

pub fn fetch_team_current_stats(team_code: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("team_code", team_code);
    fetch_and_cache(DataType::Teams, "current_stats", api_urls::TEAM_CURRENT_STATS_API_URL, &params)
}

pub fn fetch_team_stats_by_season(team_code: &str, season: &str, game_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("team_code", team_code);
    params.add_param("season", season);
    params.add_param("game_type", game_type);
    fetch_and_cache(DataType::Teams, "season_stats", api_urls::TEAM_STATS_BY_SEASON_API_URL, &params)
}

pub fn fetch_player_all() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Players, "all", api_urls::PLAYER_ALL_PLAYERS_API_URL, &params)
}

pub fn fetch_season_all() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Seasons, "all", api_urls::SEASON_ALL_SEASONS_API_URL, &params)
}

// Game functions
pub fn fetch_game_content(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "content", api_urls::GAME_CONTENT_API_URL, &params)
}

pub fn fetch_game_goal_replay(game_id: &str, event_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    params.add_param("event_id", event_id);
    fetch_and_cache(DataType::Games, "goal_replay", api_urls::GAME_GOAL_REPLAY_API_URL, &params)
}

pub fn fetch_game_odds(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "odds", api_urls::GAME_ODDS_API_URL, &params)
}

pub fn fetch_game_scores_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Games, "scores_now", api_urls::GAME_SCORES_NOW_API_URL, &params)
}

pub fn fetch_game_scores_date(date: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("date", date);
    fetch_and_cache(DataType::Games, "scores_date", api_urls::GAME_SCORES_DATE_API_URL, &params)
}

// Player functions
pub fn fetch_player_game_log(player_id: &str, season: &str, game_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("player_id", player_id);
    params.add_param("season", season);
    params.add_param("game_type", game_type);
    fetch_and_cache(DataType::Players, "game_log", api_urls::PLAYER_GAME_LOG_API_URL, &params)
}

pub fn fetch_player_game_log_now(player_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("player_id", player_id);
    fetch_and_cache(DataType::Players, "game_log_now", api_urls::PLAYER_GAME_LOG_NOW_API_URL, &params)
}

pub fn fetch_player_spotlight() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Players, "spotlight", api_urls::PLAYER_SPOTLIGHT_API_URL, &params)
}

// Skater functions
pub fn fetch_skater_leaders_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Skaters, "leaders_now", api_urls::SKATER_STATS_LEADERS_NOW_API_URL, &params)
}

pub fn fetch_skater_leaders(season: &str, game_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("season", season);
    params.add_param("game_type", game_type);
    fetch_and_cache(DataType::Skaters, "leaders", api_urls::SKATER_STATS_LEADERS_API_URL, &params)
}

// Goalie functions
pub fn fetch_goalie_leaders_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Goalies, "leaders_now", api_urls::GOALIE_STATS_LEADERS_NOW_API_URL, &params)
}

pub fn fetch_goalie_leaders(season: &str, game_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("season", season);
    params.add_param("game_type", game_type);
    fetch_and_cache(DataType::Goalies, "leaders", api_urls::GOALIE_STATS_LEADERS_API_URL, &params)
}

// Team functions
pub fn fetch_team_standings_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Teams, "standings_now", api_urls::TEAM_STANDINGS_API_URL, &params)
}

pub fn fetch_team_standings_date(date: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("date", date);
    fetch_and_cache(DataType::Teams, "standings_date", api_urls::TEAM_STANDINGS_BY_DATE_API_URL, &params)
}

pub fn fetch_team_roster_now(team_code: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("team_code", team_code);
    fetch_and_cache(DataType::Teams, "roster_now", api_urls::TEAM_ROSTER_NOW_API_URL, &params)
}

pub fn fetch_team_roster_season(team_code: &str, season: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("team_code", team_code);
    params.add_param("season", season);
    fetch_and_cache(DataType::Teams, "roster_season", api_urls::TEAM_ROSTER_SEASON_API_URL, &params)
}

// Schedule functions
pub fn fetch_schedule_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Schedule, "now", api_urls::SCHEDULE_NOW_API_URL, &params)
}

pub fn fetch_schedule_date(date: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("date", date);
    fetch_and_cache(DataType::Schedule, "date", api_urls::SCHEDULE_DATE_API_URL, &params)
}

// Playoff functions
pub fn fetch_playoff_bracket() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Playoffs, "bracket", api_urls::PLAYOFF_BRACKET_API_URL, &params)
}

pub fn fetch_playoff_series_schedule() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Playoffs, "series_schedule", api_urls::PLAYOFF_SERIES_SCHEDULE_API_URL, &params)
}

// Draft functions
pub fn fetch_draft_current_rankings() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Draft, "current_rankings", api_urls::DRAFT_CURRENT_RANKINGS_API_URL, &params)
}

pub fn fetch_draft_tracker_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Draft, "tracker_now", api_urls::DRAFT_TRACKER_NOW_API_URL, &params)
}

pub fn fetch_draft_picks_now() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Draft, "picks_now", api_urls::DRAFT_PICKS_NOW_API_URL, &params)
}

pub fn fetch_draft_picks(year: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("year", year);
    fetch_and_cache(DataType::Draft, "picks", api_urls::DRAFT_PICKS_API_URL, &params)
}
