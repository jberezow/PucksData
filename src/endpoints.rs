use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Data type categorization for endpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub fn as_str(&self) -> &'static str {
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

/// Definition of a parameter for an endpoint
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub example: &'static str,
}

/// Complete definition of an API endpoint
#[derive(Debug, Clone)]
pub struct Endpoint {
    // Basic endpoint info
    pub name: &'static str,
    pub url: &'static str,
    pub description: &'static str,
    pub data_type: DataType,
    pub implemented: bool,
    
    // Parameter definitions
    pub parameters: Vec<Parameter>,
    
    // For testing
    pub test_params: HashMap<&'static str, &'static str>,
    
    // For CLI documentation
    pub example: &'static str,
}

/// Get endpoint by name
pub fn get_endpoint(name: &str) -> Option<&'static Endpoint> {
    ALL_ENDPOINTS.iter().find(|e| e.name == name)
}

/// Get all endpoints for a specific data type
pub fn get_endpoints_by_type(data_type: DataType) -> Vec<&'static Endpoint> {
    ALL_ENDPOINTS.iter()
        .filter(|e| e.data_type == data_type)
        .collect()
}

/// Get all available endpoints
pub fn get_all_endpoints() -> &'static [Endpoint] {
    &ALL_ENDPOINTS
}

/// Get all implemented endpoints
pub fn get_implemented_endpoints() -> Vec<&'static Endpoint> {
    ALL_ENDPOINTS.iter()
        .filter(|e| e.implemented)
        .collect()
}

// Registry of all endpoints - using Lazy for initialization
pub static ALL_ENDPOINTS: Lazy<Vec<Endpoint>> = Lazy::new(|| {
    vec![
        // Game endpoints
        Endpoint {
            name: "game_story",
            url: "https://api-web.nhle.com/v1/wsc/game-story/{game_id}",
            description: "Fetch a game story by game ID",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "game_id",
                    description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                    required: true,
                    example: "2023020001",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata game story 2023020001",
        },
        Endpoint {
            name: "game_boxscore",
            url: "https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore",
            description: "Fetch a game boxscore by game ID",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "game_id",
                    description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                    required: true,
                    example: "2023020001",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata game boxscore 2023020001",
        },
        Endpoint {
            name: "game_play_by_play",
            url: "https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play",
            description: "Fetch play-by-play data by game ID",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "game_id",
                    description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                    required: true,
                    example: "2023020001",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata game play-by-play 2023020001",
        },
        Endpoint {
            name: "game_all_games",
            url: "https://api.nhle.com/stats/rest/en/game",
            description: "Fetch all games data",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata game all",
        },
        Endpoint {
            name: "game_content",
            url: "https://api-web.nhle.com/v1/gamecenter/{game_id}/landing",
            description: "Fetch game content",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "game_id",
                    description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                    required: true,
                    example: "2023020001",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata game content 2023020001",
        },
        Endpoint {
            name: "game_goal_replay",
            url: "https://api-web.nhle.com/v1/ppt-replay/goal/{game_id}/{event_id}",
            description: "Fetch goal replay",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "game_id",
                    description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                    required: true,
                    example: "2023020001",
                },
                Parameter {
                    name: "event_id",
                    description: "The event ID for the goal (format: numeric, e.g., 401)",
                    required: true,
                    example: "401",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map.insert("event_id", "401");
                map
            },
            example: "pucksdata game goal-replay 2023020001 401",
        },
        Endpoint {
            name: "game_scores_now",
            url: "https://api-web.nhle.com/v1/score/now",
            description: "Fetch current scores",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata game scores-now",
        },
        Endpoint {
            name: "game_scores_date",
            url: "https://api-web.nhle.com/v1/score/{date}",
            description: "Fetch scores by date",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "date",
                    description: "The date (format: YYYY-MM-DD, e.g., 2024-02-15)",
                    required: true,
                    example: "2024-02-15",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("date", "2024-02-15");
                map
            },
            example: "pucksdata game scores-date 2024-02-15",
        },

        // Player endpoints
        Endpoint {
            name: "player_summary",
            url: "https://api-web.nhle.com/v1/player/{player_id}/landing",
            description: "Fetch a player summary by player ID",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "player_id",
                    description: "The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)",
                    required: true,
                    example: "8478402",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("player_id", "8478402");
                map
            },
            example: "pucksdata player summary 8478402",
        },
        Endpoint {
            name: "player_all",
            url: "https://api.nhle.com/stats/rest/en/players",
            description: "Fetch all players data",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata player all",
        },
        Endpoint {
            name: "player_game_log",
            url: "https://api-web.nhle.com/v1/player/{player_id}/game-log/{season}/{game_type}",
            description: "Fetch player game log for a specific season and game type",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "player_id",
                    description: "The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)",
                    required: true,
                    example: "8478402",
                },
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
                Parameter {
                    name: "game_type",
                    description: "The game type (e.g., 2 for regular season, 3 for playoffs)",
                    required: true,
                    example: "2",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("player_id", "8478402");
                map.insert("season", "20232024");
                map.insert("game_type", "2");
                map
            },
            example: "pucksdata player game-log 8478402 20232024 2",
        },
        Endpoint {
            name: "player_game_log_now",
            url: "https://api-web.nhle.com/v1/player/{player_id}/game-log/now",
            description: "Fetch player game log for the current season",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "player_id",
                    description: "The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)",
                    required: true,
                    example: "8478402",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("player_id", "8478402");
                map
            },
            example: "pucksdata player game-log-now 8478402",
        },
        Endpoint {
            name: "player_spotlight",
            url: "https://api-web.nhle.com/v1/player-spotlight",
            description: "Fetch player spotlight data",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata player spotlight",
        },
        
        // Skater statistics endpoints
        Endpoint {
            name: "skater_stats_leaders_now",
            url: "https://api-web.nhle.com/v1/skater-stats-leaders/current",
            description: "Fetch current skater stats leaders",
            data_type: DataType::Skaters,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata skaters leaders-now",
        },
        Endpoint {
            name: "skater_stats_leaders",
            url: "https://api-web.nhle.com/v1/skater-stats-leaders/{season}/{game_type}",
            description: "Fetch skater stats leaders for a specific season and game type",
            data_type: DataType::Skaters,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
                Parameter {
                    name: "game_type",
                    description: "The game type (e.g., 2 for regular season, 3 for playoffs)",
                    required: true,
                    example: "2",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("season", "20232024");
                map.insert("game_type", "2");
                map
            },
            example: "pucksdata skaters leaders 20232024 2",
        },
        
        // Goalie statistics endpoints
        Endpoint {
            name: "goalie_stats_leaders_now",
            url: "https://api-web.nhle.com/v1/goalie-stats-leaders/current",
            description: "Fetch current goalie stats leaders",
            data_type: DataType::Goalies,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata goalies leaders-now",
        },
        Endpoint {
            name: "goalie_stats_leaders",
            url: "https://api-web.nhle.com/v1/goalie-stats-leaders/{season}/{game_type}",
            description: "Fetch goalie stats leaders for a specific season and game type",
            data_type: DataType::Goalies,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
                Parameter {
                    name: "game_type",
                    description: "The game type (e.g., 2 for regular season, 3 for playoffs)",
                    required: true,
                    example: "2",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("season", "20232024");
                map.insert("game_type", "2");
                map
            },
            example: "pucksdata goalies leaders 20232024 2",
        },
        
        // Team endpoints
        Endpoint {
            name: "team_current_stats",
            url: "https://api-web.nhle.com/v1/club-stats/{team_code}/now",
            description: "Fetch current team statistics",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map
            },
            example: "pucksdata team current-stats EDM",
        },
        Endpoint {
            name: "team_stats_by_season",
            url: "https://api-web.nhle.com/v1/club-stats/{team_code}/{season}/{game_type}",
            description: "Fetch team statistics for a specific season and game type",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
                Parameter {
                    name: "game_type",
                    description: "The game type (e.g., 2 for regular season, 3 for playoffs)",
                    required: true,
                    example: "2",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map.insert("season", "20232024");
                map.insert("game_type", "2");
                map
            },
            example: "pucksdata team stats-by-season EDM 20232024 2",
        },
        Endpoint {
            name: "team_standings_now",
            url: "https://api-web.nhle.com/v1/standings/now",
            description: "Fetch current standings",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata team standings-now",
        },
        Endpoint {
            name: "team_standings_by_date",
            url: "https://api-web.nhle.com/v1/standings/{date}",
            description: "Fetch standings by date",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "date",
                    description: "The date (format: YYYY-MM-DD, e.g., 2024-02-15)",
                    required: true,
                    example: "2024-02-15",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("date", "2024-02-15");
                map
            },
            example: "pucksdata team standings-by-date 2024-02-15",
        },
        Endpoint {
            name: "team_standings_season",
            url: "https://api-web.nhle.com/v1/standings/{season}",
            description: "Fetch standings for a specific season",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("season", "20232024");
                map
            },
            example: "pucksdata team standings-season 20232024",
        },
        Endpoint {
            name: "team_roster_now",
            url: "https://api-web.nhle.com/v1/roster/{team_code}/current",
            description: "Fetch current team roster",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map
            },
            example: "pucksdata team roster-now EDM",
        },
        Endpoint {
            name: "team_roster_season",
            url: "https://api-web.nhle.com/v1/roster/{team_code}/{season}",
            description: "Fetch team roster for a specific season",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map.insert("season", "20232024");
                map
            },
            example: "pucksdata team roster-season EDM 20232024",
        },
        Endpoint {
            name: "team_prospects",
            url: "https://api-web.nhle.com/v1/roster-prospects/{team_code}",
            description: "Fetch team prospects",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map
            },
            example: "pucksdata team prospects EDM",
        },
        Endpoint {
            name: "team_schedule_now",
            url: "https://api-web.nhle.com/v1/club-schedule/{team_code}/week/now",
            description: "Fetch current team schedule",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map
            },
            example: "pucksdata team schedule-now EDM",
        },
        Endpoint {
            name: "team_schedule_season",
            url: "https://api-web.nhle.com/v1/club-schedule/{team_code}/season/{season}",
            description: "Fetch team schedule for a specific season",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20232024)",
                    required: true,
                    example: "20232024",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map.insert("season", "20232024");
                map
            },
            example: "pucksdata team schedule-season EDM 20232024",
        },
        Endpoint {
            name: "team_schedule_month",
            url: "https://api-web.nhle.com/v1/club-schedule/{team_code}/month/{date}",
            description: "Fetch team schedule for a specific month",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "team_code",
                    description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                    required: true,
                    example: "EDM",
                },
                Parameter {
                    name: "date",
                    description: "The date (format: YYYY-MM-DD, e.g., 2024-02-15)",
                    required: true,
                    example: "2024-02-15",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map.insert("date", "2024-02-15");
                map
            },
            example: "pucksdata team schedule-month EDM 2024-02-15",
        },
        
        // League schedule endpoints
        Endpoint {
            name: "schedule_now",
            url: "https://api-web.nhle.com/v1/schedule/now",
            description: "Fetch current schedule",
            data_type: DataType::Schedule,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata schedule now",
        },
        Endpoint {
            name: "schedule_date",
            url: "https://api-web.nhle.com/v1/schedule/{date}",
            description: "Fetch schedule by date",
            data_type: DataType::Schedule,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "date",
                    description: "The date (format: YYYY-MM-DD, e.g., 2024-02-15)",
                    required: true,
                    example: "2024-02-15",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("date", "2024-02-15");
                map
            },
            example: "pucksdata schedule date 2024-02-15",
        },

        // Playoff endpoints
        Endpoint {
            name: "playoff_bracket",
            url: "https://api-web.nhle.com/v1/playoff-bracket/{year}",
            description: "Fetch playoff bracket",
            data_type: DataType::Playoffs,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "year",
                    description: "The year (format: YYYY, e.g., 2023)",
                    required: true,
                    example: "2023",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("year", "2023");
                map
            },
            example: "pucksdata playoffs bracket 2023",
        },
        Endpoint {
            name: "playoff_series_schedule",
            url: "https://api-web.nhle.com/v1/schedule/playoff-series/{season}/{letter}",
            description: "Fetch playoff series schedule",
            data_type: DataType::Playoffs,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20222023)",
                    required: true,
                    example: "20222023",
                },
                Parameter {
                    name: "letter",
                    description: "The series letter (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P)",
                    required: true,
                    example: "A",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("season", "20222023");
                map.insert("letter", "A");
                map
            },
            example: "pucksdata playoffs series-schedule 20222023 A",
        },
        Endpoint {
            name: "playoff_series_carousel",
            url: "https://api-web.nhle.com/v1/playoff-series/carousel/{season}",
            description: "Fetch playoff series carousel",
            data_type: DataType::Playoffs,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "season",
                    description: "The season (format: YYYYYYYY, e.g., 20222023)",
                    required: true,
                    example: "20222023",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("season", "20222023");
                map
            },
            example: "pucksdata playoffs series-carousel 20222023",
        },
        Endpoint {
            name: "playoff_series_metadata",
            url: "https://api-web.nhle.com/v1/meta/playoff-series/{year}/{letter}",
            description: "Fetch playoff series metadata",
            data_type: DataType::Playoffs,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "year",
                    description: "The year (format: YYYY, e.g., 2023)",
                    required: true,
                    example: "2023",
                },
                Parameter {
                    name: "letter",
                    description: "The series letter (A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P)",
                    required: true,
                    example: "A",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("year", "2023");
                map.insert("letter", "A");
                map
            },
            example: "pucksdata playoffs series-metadata 2023 A",
        },

        // Season endpoints
        Endpoint {
            name: "season_all_seasons",
            url: "https://api-web.nhle.com/v1/season",
            description: "Fetch all seasons data",
            data_type: DataType::Seasons,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata seasons all",
        },

        // Draft endpoints
        Endpoint {
            name: "draft_current_rankings",
            url: "https://api-web.nhle.com/v1/draft/rankings/now",
            description: "Fetch current draft rankings",
            data_type: DataType::Draft,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata draft current-rankings",
        },
        Endpoint {
            name: "draft_tracker_now",
            url: "https://api-web.nhle.com/v1/draft-tracker/picks/now",
            description: "Fetch current draft tracker",
            data_type: DataType::Draft,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata draft tracker-now",
        },
        Endpoint {
            name: "draft_picks_now",
            url: "https://api-web.nhle.com/v1/draft/picks/now",
            description: "Fetch current draft picks",
            data_type: DataType::Draft,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata draft picks-now",
        },
        Endpoint {
            name: "draft_picks",
            url: "https://api-web.nhle.com/v1/draft/picks/{year}/all",
            description: "Fetch draft picks for a specific year",
            data_type: DataType::Draft,
            implemented: true,
            parameters: vec![
                Parameter {
                    name: "year",
                    description: "The year (format: YYYY, e.g., 2023)",
                    required: true,
                    example: "2023",
                },
            ],
            test_params: {
                let mut map = HashMap::new();
                map.insert("year", "2023");
                map
            },
            example: "pucksdata draft picks 2023",
        },
    ]
}); 