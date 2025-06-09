// PLAYER ENDPOINTS
// ----------------

// Player summary has season totals and lots of helpful bio stuff
pub const PLAYER_SUMMARY_API_URL: &str = "https://api-web.nhle.com/v1/player/{player_id}/landing";

// Player game log gets all game IDs for a player for a season and type (2 for reg, 3 for playoffs) - very handy
pub const PLAYER_GAME_LOG_API_URL: &str = "https://api-web.nhle.com/v1/player/{player_id}/game-log/{season}/{game_type}";

// Team endpoints
pub const TEAM_STATS_BY_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{team_code}/{season}/{game_type}";
pub const TEAM_STANDINGS_BY_DATE_API_URL: &str = "https://api-web.nhle.com/v1/standings/{date}";
pub const TEAM_STANDINGS_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/standings-season";
pub const TEAM_ROSTER_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/roster/{team_code}/{season}";
pub const TEAM_PROSPECTS_API_URL: &str = "https://api-web.nhle.com/v1/prospects/{team_code}";
pub const TEAM_SCHEDULE_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/club-schedule-season/{team_code}/{season}";
pub const TEAM_SCHEDULE_MONTH_API_URL: &str = "https://api-web.nhle.com/v1/club-schedule/{team_code}/month/{date}";

// League schedule endpoints
pub const SCHEDULE_NOW_API_URL: &str = "https://api-web.nhle.com/v1/schedule/now";
pub const SCHEDULE_DATE_API_URL: &str = "https://api-web.nhle.com/v1/schedule/{date}";

// Game endpoints
pub const GAME_STORY_API_URL: &str = "https://api-web.nhle.com/v1/wsc/game-story/{game_id}";
pub const GAME_BOXSCORE_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore";
pub const GAME_PLAY_BY_PLAY_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play";
pub const GAME_ALL_GAMES_API_URL: &str = "https://api.nhle.com/stats/rest/en/game";
pub const GAME_SCORES_DATE_API_URL: &str = "https://api-web.nhle.com/v1/score/{date}";
pub const GAME_CONTENT_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{game_id}/landing";
pub const GAME_GOAL_REPLAY_API_URL: &str = "https://api-web.nhle.com/v1/ppt-replay/goal/{game_id}/{event_id}";

// Playoff endpoints
pub const PLAYOFF_BRACKET_API_URL: &str = "https://api-web.nhle.com/v1/playoff-bracket/{year}";
pub const PLAYOFF_SERIES_SCHEDULE_API_URL: &str = "https://api-web.nhle.com/v1/schedule/playoff-series/{season}/{letter}";
pub const PLAYOFF_SERIES_CAROUSEL_API_URL: &str = "https://api-web.nhle.com/v1/playoff-series/carousel/{season}";
pub const PLAYOFF_SERIES_METADATA_API_URL: &str = "https://api-web.nhle.com/v1/meta/playoff-series/{year}/{letter}";

pub fn get_url_template(data_type: &str, endpoint: &str) -> Option<&'static str> {
    match (data_type, endpoint) {
        // Games
        ("games", "story") => Some(GAME_STORY_API_URL),
        ("games", "boxscore") => Some(GAME_BOXSCORE_API_URL),
        ("games", "playbyplay") => Some(GAME_PLAY_BY_PLAY_API_URL),
        ("games", "all") => Some(GAME_ALL_GAMES_API_URL),
        ("games", "content") => Some(GAME_CONTENT_API_URL),
        ("games", "goal_replay") => Some(GAME_GOAL_REPLAY_API_URL),
        ("games", "scores_date") => Some(GAME_SCORES_DATE_API_URL),
        
        // Players
        ("players", "summary") => Some(PLAYER_SUMMARY_API_URL),
        ("players", "game_log") => Some(PLAYER_GAME_LOG_API_URL),
        
        // Teams
        ("teams", "season_stats") => Some(TEAM_STATS_BY_SEASON_API_URL),
        ("teams", "standings_date") => Some(TEAM_STANDINGS_BY_DATE_API_URL),
        ("teams", "standings_season") => Some(TEAM_STANDINGS_SEASON_API_URL),
        ("teams", "roster_season") => Some(TEAM_ROSTER_SEASON_API_URL),
        ("teams", "prospects") => Some(TEAM_PROSPECTS_API_URL),
        ("teams", "schedule_season") => Some(TEAM_SCHEDULE_SEASON_API_URL),
        ("teams", "schedule_month") => Some(TEAM_SCHEDULE_MONTH_API_URL),
        
        // Schedule
        ("schedule", "now") => Some(SCHEDULE_NOW_API_URL),
        ("schedule", "date") => Some(SCHEDULE_DATE_API_URL),
        
        // Playoffs
        ("playoffs", "bracket") => Some(PLAYOFF_BRACKET_API_URL),
        ("playoffs", "series_schedule") => Some(PLAYOFF_SERIES_SCHEDULE_API_URL),
        ("playoffs", "series_carousel") => Some(PLAYOFF_SERIES_CAROUSEL_API_URL),
        ("playoffs", "series_metadata") => Some(PLAYOFF_SERIES_METADATA_API_URL),
        
        _ => None,
    }
} 