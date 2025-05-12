pub const GAME_STORY_API_URL: &str = "https://api-web.nhle.com/v1/wsc/game-story/{id}";
pub const GAME_BOXSCORE_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{id}/boxscore";
pub const GAME_PLAY_BY_PLAY_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{id}/play-by-play";
pub const GAME_ALL_GAMES_API_URL: &str = "https://api.nhle.com/stats/rest/en/game";
pub const GAME_ALL_METADATA_API_URL: &str = "https://api.nhle.com/stats/rest/en/game/meta";
pub const PLAYER_SUMMARY_API_URL: &str = "https://api-web.nhle.com/v1/player/{id}/landing";
pub const TEAM_CURRENT_STATS_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{id}/now";
pub const TEAM_STATS_BY_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{id}/{season_id}/{game_type}";

pub fn get_url_template(data_type: &str, endpoint: &str) -> Option<&'static str> {
    match (data_type, endpoint) {
        ("games", "story") => Some(GAME_STORY_API_URL),
        ("games", "boxscore") => Some(GAME_BOXSCORE_API_URL),
        ("games", "playbyplay") => Some(GAME_PLAY_BY_PLAY_API_URL),
        ("players", "summary") => Some(PLAYER_SUMMARY_API_URL),
        ("teams", "current_stats") => Some(TEAM_CURRENT_STATS_API_URL),
        _ => None,
    }
} 