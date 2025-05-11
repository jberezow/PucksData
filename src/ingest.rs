use crate::api;
use crate::cache;
use std::collections::HashMap;
use std::path::PathBuf;

const GAME_STORY_API_URL: &str = "https://api-web.nhle.com/v1/wsc/game-story/{game_id}";
const GAME_BOXSCORE_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore";
const GAME_PLAY_BY_PLAY_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play";
const GAME_ALL_GAMES_API_URL: &str = "https://api.nhle.com/stats/rest/en/game";
const GAME_ALL_METADATA_API_URL: &str = "https://api.nhle.com/stats/rest/en/game/meta";
const PLAYER_SUMMARY_API_URL: &str = "https://api-web.nhle.com/v1/player/{player_id}/landing";
const TEAM_CURRENT_STATS_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{team_code}/now";
const TEAM_STATS_BY_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{team_code}/{season_id}/{game_type}";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataType {
    Games,
    Players,
    Teams,
    // Seasons,
    // General,
}

impl DataType {
    fn as_str(&self) -> &'static str {
        match self {
            DataType::Games => "games",
            DataType::Players => "players",
            DataType::Teams => "teams",
            // DataType::Seasons => "seasons",
            // DataType::General => "general",
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

    // If we have an ID parameter, create a subdirectory for it
    if let Some(id) = params.get_param("game_id")
        .or_else(|| params.get_param("player_id"))
        .or_else(|| params.get_param("team_code"))
        .or_else(|| params.get_param("season_id")) {
        file_path.push(id);

        // For team season stats, create a seasons subdirectory
        if data_type == DataType::Teams && endpoint == "season_stats" {
            if let Some(season_id) = params.get_param("season_id") {
                file_path.push("seasons");
                file_path.push(season_id);
                file_path.set_extension("json");
                return handle_fetch_and_cache(file_path, endpoint, url_template, params);
            }
        }
    }

    file_path.push(endpoint);
    file_path.set_extension("json");

    handle_fetch_and_cache(file_path, endpoint, url_template, params)
}

fn handle_fetch_and_cache(file_path: PathBuf, endpoint: &str, url_template: &str, params: &ApiParams) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(_) = cache::read_from_cache(&file_path) {
        println!("✅ Found cached {} data at {:?}", endpoint, file_path);
        return Ok(());
    }

    println!("🌐 Fetching {} data from NHL API...", endpoint);

    let mut url = url_template.to_string();
    for (key, value) in &params.params {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    
    let json = api::fetch_api_json(&url)?;

    cache::write_to_cache(&file_path, &json)?;

    println!("💾 Saved {} data to {:?}", endpoint, file_path);

    Ok(())
}

pub fn fetch_game_story(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "story", GAME_STORY_API_URL, &params)
}

pub fn fetch_game_boxscore(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "boxscore", GAME_BOXSCORE_API_URL, &params)
}

pub fn fetch_game_play_by_play(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("game_id", game_id);
    fetch_and_cache(DataType::Games, "playbyplay", GAME_PLAY_BY_PLAY_API_URL, &params)
}

pub fn fetch_game_all_games() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Games, "all", GAME_ALL_GAMES_API_URL, &params)
}

pub fn fetch_game_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let params = ApiParams::new();
    fetch_and_cache(DataType::Games, "metadata", GAME_ALL_METADATA_API_URL, &params)
}

pub fn fetch_player_summary(player_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("player_id", player_id);
    fetch_and_cache(DataType::Players, "summary", PLAYER_SUMMARY_API_URL, &params)
}

pub fn fetch_team_current_stats(team_code: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("team_code", team_code);
    fetch_and_cache(DataType::Teams, "current_stats", TEAM_CURRENT_STATS_API_URL, &params)
}

pub fn fetch_team_stats_by_season(team_code: &str, season_id: &str, game_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut params = ApiParams::new();
    params.add_param("team_code", team_code);
    params.add_param("season_id", season_id);
    params.add_param("game_type", game_type);
    fetch_and_cache(DataType::Teams, "season_stats", TEAM_STATS_BY_SEASON_API_URL, &params)
}
