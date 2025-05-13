// Player endpoints
pub const PLAYER_SUMMARY_API_URL: &str = "https://api-web.nhle.com/v1/player/{id}/landing";
pub const PLAYER_ALL_PLAYERS_API_URL: &str = "https://api.nhle.com/stats/rest/en/players";
pub const PLAYER_GAME_LOG_API_URL: &str = "https://api-web.nhle.com/v1/player/{id}/game-log/{season}";
pub const PLAYER_GAME_LOG_NOW_API_URL: &str = "https://api-web.nhle.com/v1/player/{id}/game-log/now";
pub const PLAYER_SPOTLIGHT_API_URL: &str = "https://api-web.nhle.com/v1/player-spotlight";
pub const SKATER_STATS_LEADERS_NOW_API_URL: &str = "https://api-web.nhle.com/v1/skater-stats-leaders/current";
pub const SKATER_STATS_LEADERS_API_URL: &str = "https://api-web.nhle.com/v1/skater-stats-leaders/{season}/{game_type}";
pub const GOALIE_STATS_LEADERS_NOW_API_URL: &str = "https://api-web.nhle.com/v1/goalie-stats-leaders/current";
pub const GOALIE_STATS_LEADERS_API_URL: &str = "https://api-web.nhle.com/v1/goalie-stats-leaders/{season}/{game_type}";

// Team endpoints
pub const TEAM_CURRENT_STATS_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{id}/now";
pub const TEAM_STATS_BY_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/club-stats/{id}/{season_id}/{game_type}";
pub const TEAM_STANDINGS_API_URL: &str = "https://api-web.nhle.com/v1/standings/now";
pub const TEAM_STANDINGS_BY_DATE_API_URL: &str = "https://api-web.nhle.com/v1/standings/{date}";
pub const TEAM_STANDINGS_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/standings/{season}";
pub const TEAM_ROSTER_NOW_API_URL: &str = "https://api-web.nhle.com/v1/roster/{team_code}/current";
pub const TEAM_ROSTER_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/roster/{team_code}/{season}";
pub const TEAM_PROSPECTS_API_URL: &str = "https://api-web.nhle.com/v1/roster-prospects/{team_code}";
pub const TEAM_SCHEDULE_NOW_API_URL: &str = "https://api-web.nhle.com/v1/club-schedule/{team_code}/week/now";
pub const TEAM_SCHEDULE_SEASON_API_URL: &str = "https://api-web.nhle.com/v1/club-schedule/{team_code}/season/{season}";
pub const TEAM_SCHEDULE_MONTH_API_URL: &str = "https://api-web.nhle.com/v1/club-schedule/{team_code}/month/{date}";

// League schedule endpoints
pub const SCHEDULE_NOW_API_URL: &str = "https://api-web.nhle.com/v1/schedule/now";
pub const SCHEDULE_DATE_API_URL: &str = "https://api-web.nhle.com/v1/schedule/{date}";
pub const SCHEDULE_CALENDAR_NOW_API_URL: &str = "https://api-web.nhle.com/v1/schedule-calendar/now";
pub const SCHEDULE_CALENDAR_DATE_API_URL: &str = "https://api-web.nhle.com/v1/schedule-calendar/{date}";

// Game endpoints
pub const GAME_STORY_API_URL: &str = "https://api-web.nhle.com/v1/wsc/game-story/{id}";
pub const GAME_BOXSCORE_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{id}/boxscore";
pub const GAME_PLAY_BY_PLAY_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{id}/play-by-play";
pub const GAME_ALL_GAMES_API_URL: &str = "https://api.nhle.com/stats/rest/en/game";
pub const GAME_ALL_METADATA_API_URL: &str = "https://api.nhle.com/stats/rest/en/game/meta";
pub const GAME_SCORES_NOW_API_URL: &str = "https://api-web.nhle.com/v1/score/now";
pub const GAME_SCORES_DATE_API_URL: &str = "https://api-web.nhle.com/v1/score/{date}";
pub const GAME_SCOREBOARD_API_URL: &str = "https://api-web.nhle.com/v1/scoreboard/{date}";
pub const GAME_TV_SCHEDULE_NOW_API_URL: &str = "https://api-web.nhle.com/v1/network-schedule/now";
pub const GAME_TV_SCHEDULE_DATE_API_URL: &str = "https://api-web.nhle.com/v1/network-schedule/{date}";
pub const GAME_ODDS_API_URL: &str = "https://api-web.nhle.com/v1/game-odds/{id}";
pub const GAME_CONTENT_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{id}/landing";
pub const GAME_GOAL_REPLAY_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{id}/replay/{event_id}";

// Playoff endpoints
pub const PLAYOFF_BRACKET_API_URL: &str = "https://api-web.nhle.com/v1/playoff-bracket";
pub const PLAYOFF_SERIES_SCHEDULE_API_URL: &str = "https://api-web.nhle.com/v1/playoff-series-schedule";

// Season endpoints
pub const SEASON_ALL_SEASONS_API_URL: &str = "https://api-web.nhle.com/v1/season";

// Draft endpoints
pub const DRAFT_CURRENT_RANKINGS_API_URL: &str = "https://api-web.nhle.com/v1/draft/rankings/now";
pub const DRAFT_TRACKER_NOW_API_URL: &str = "https://api-web.nhle.com/v1/draft-tracker/picks/now";
pub const DRAFT_PICKS_NOW_API_URL: &str = "https://api-web.nhle.com/v1/draft/picks/now";
pub const DRAFT_PICKS_API_URL: &str = "https://api-web.nhle.com/v1/draft/picks/{year}/all";

pub fn get_url_template(data_type: &str, endpoint: &str) -> Option<&'static str> {
    match (data_type, endpoint) {
        // Games
        ("games", "story") => Some(GAME_STORY_API_URL),
        ("games", "boxscore") => Some(GAME_BOXSCORE_API_URL),
        ("games", "playbyplay") => Some(GAME_PLAY_BY_PLAY_API_URL),
        ("games", "all") => Some(GAME_ALL_GAMES_API_URL),
        ("games", "metadata") => Some(GAME_ALL_METADATA_API_URL),
        ("games", "content") => Some(GAME_CONTENT_API_URL),
        ("games", "goal_replay") => Some(GAME_GOAL_REPLAY_API_URL),
        ("games", "odds") => Some(GAME_ODDS_API_URL),
        ("games", "scores_now") => Some(GAME_SCORES_NOW_API_URL),
        ("games", "scores_date") => Some(GAME_SCORES_DATE_API_URL),
        ("games", "scoreboard") => Some(GAME_SCOREBOARD_API_URL),
        ("games", "tv_now") => Some(GAME_TV_SCHEDULE_NOW_API_URL),
        ("games", "tv_date") => Some(GAME_TV_SCHEDULE_DATE_API_URL),
        
        // Players
        ("players", "summary") => Some(PLAYER_SUMMARY_API_URL),
        ("players", "all") => Some(PLAYER_ALL_PLAYERS_API_URL),
        ("players", "game_log") => Some(PLAYER_GAME_LOG_API_URL),
        ("players", "game_log_now") => Some(PLAYER_GAME_LOG_NOW_API_URL),
        ("players", "spotlight") => Some(PLAYER_SPOTLIGHT_API_URL),
        
        // Skaters
        ("skaters", "leaders_now") => Some(SKATER_STATS_LEADERS_NOW_API_URL),
        ("skaters", "leaders") => Some(SKATER_STATS_LEADERS_API_URL),
        
        // Goalies
        ("goalies", "leaders_now") => Some(GOALIE_STATS_LEADERS_NOW_API_URL),
        ("goalies", "leaders") => Some(GOALIE_STATS_LEADERS_API_URL),
        
        // Teams
        ("teams", "current_stats") => Some(TEAM_CURRENT_STATS_API_URL),
        ("teams", "season_stats") => Some(TEAM_STATS_BY_SEASON_API_URL),
        ("teams", "standings_now") => Some(TEAM_STANDINGS_API_URL),
        ("teams", "standings_date") => Some(TEAM_STANDINGS_BY_DATE_API_URL),
        ("teams", "standings_season") => Some(TEAM_STANDINGS_SEASON_API_URL),
        ("teams", "roster_now") => Some(TEAM_ROSTER_NOW_API_URL),
        ("teams", "roster_season") => Some(TEAM_ROSTER_SEASON_API_URL),
        ("teams", "prospects") => Some(TEAM_PROSPECTS_API_URL),
        ("teams", "schedule_now") => Some(TEAM_SCHEDULE_NOW_API_URL),
        ("teams", "schedule_season") => Some(TEAM_SCHEDULE_SEASON_API_URL),
        ("teams", "schedule_month") => Some(TEAM_SCHEDULE_MONTH_API_URL),
        
        // Schedule
        ("schedule", "now") => Some(SCHEDULE_NOW_API_URL),
        ("schedule", "date") => Some(SCHEDULE_DATE_API_URL),
        ("schedule", "calendar_now") => Some(SCHEDULE_CALENDAR_NOW_API_URL),
        ("schedule", "calendar_date") => Some(SCHEDULE_CALENDAR_DATE_API_URL),
        
        // Playoffs
        ("playoffs", "bracket") => Some(PLAYOFF_BRACKET_API_URL),
        ("playoffs", "series_schedule") => Some(PLAYOFF_SERIES_SCHEDULE_API_URL),
        
        // Seasons
        ("seasons", "all") => Some(SEASON_ALL_SEASONS_API_URL),
        
        // Draft
        ("draft", "current_rankings") => Some(DRAFT_CURRENT_RANKINGS_API_URL),
        ("draft", "tracker_now") => Some(DRAFT_TRACKER_NOW_API_URL),
        ("draft", "picks_now") => Some(DRAFT_PICKS_NOW_API_URL),
        ("draft", "picks") => Some(DRAFT_PICKS_API_URL),
        
        _ => None,
    }
} 