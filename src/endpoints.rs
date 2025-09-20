use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Data type categorization for endpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Games,
    Players,
    Teams,
}

impl DataType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataType::Games => "games",
            DataType::Players => "players",
            DataType::Teams => "teams",
        }
    }

    pub fn as_entity_type(&self) -> &'static str {
        match self {
            DataType::Games => "game",
            DataType::Players => "player",
            DataType::Teams => "team",
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
    ALL_ENDPOINTS
        .iter()
        .filter(|e| e.data_type == data_type)
        .collect()
}

/// Get all available endpoints
pub fn get_all_endpoints() -> &'static [Endpoint] {
    &ALL_ENDPOINTS
}

/// Get all implemented endpoints
pub fn get_implemented_endpoints() -> Vec<&'static Endpoint> {
    ALL_ENDPOINTS.iter().filter(|e| e.implemented).collect()
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
            parameters: vec![Parameter {
                name: "game_id",
                description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                required: true,
                example: "2023020001",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata games story 2023020001",
        },
        Endpoint {
            name: "game_boxscore",
            url: "https://api-web.nhle.com/v1/gamecenter/{game_id}/boxscore",
            description: "Fetch a game boxscore by game ID",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![Parameter {
                name: "game_id",
                description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                required: true,
                example: "2023020001",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata games boxscore 2023020001",
        },
        Endpoint {
            name: "game_play_by_play",
            url: "https://api-web.nhle.com/v1/gamecenter/{game_id}/play-by-play",
            description: "Fetch play-by-play data by game ID",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![Parameter {
                name: "game_id",
                description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                required: true,
                example: "2023020001",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata games play-by-play 2023020001",
        },
        Endpoint {
            name: "games_all",
            url: "https://api.nhle.com/stats/rest/en/game",
            description: "Fetch all games data",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata games all",
        },
        Endpoint {
            name: "game_content",
            url: "https://api-web.nhle.com/v1/gamecenter/{game_id}/landing",
            description: "Fetch game content",
            data_type: DataType::Games,
            implemented: true,
            parameters: vec![Parameter {
                name: "game_id",
                description: "The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)",
                required: true,
                example: "2023020001",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("game_id", "2023020001");
                map
            },
            example: "pucksdata games content 2023020001",
        },
        // Player endpoints
        Endpoint {
            name: "player_summary",
            url: "https://api-web.nhle.com/v1/player/{player_id}/landing",
            description: "Fetch a player summary by player ID",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![Parameter {
                name: "player_id",
                description:
                    "The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)",
                required: true,
                example: "8478402",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("player_id", "8478402");
                map
            },
            example: "pucksdata players summary 8478402",
        },
        Endpoint {
            name: "players_all",
            url: "https://api.nhle.com/stats/rest/en/players",
            description: "Fetch all players data",
            data_type: DataType::Players,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata players all",
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
                    description:
                        "The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)",
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
            example: "pucksdata players game-log 8478402 20232024 2",
        },
        // Team endpoints
        Endpoint {
            name: "team_current_stats",
            url: "https://api-web.nhle.com/v1/club-stats/{team_code}/now",
            description: "Fetch current team statistics",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![Parameter {
                name: "team_code",
                description: "The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)",
                required: true,
                example: "EDM",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("team_code", "EDM");
                map
            },
            example: "pucksdata teams current-stats EDM",
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
            example: "pucksdata teams stats-by-season EDM 20232024 2",
        },
        Endpoint {
            name: "team_standings_date",
            url: "https://api-web.nhle.com/v1/standings/{date}",
            description: "Fetch standings by date",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![Parameter {
                name: "date",
                description: "The date (format: YYYY-MM-DD, e.g., 2024-02-15)",
                required: true,
                example: "2024-02-15",
            }],
            test_params: {
                let mut map = HashMap::new();
                map.insert("date", "2024-02-15");
                map
            },
            example: "pucksdata teams standings-by-date 2024-02-15",
        },
        Endpoint {
            name: "team_standings_season",
            url: "https://api-web.nhle.com/v1/standings-season",
            description: "Fetch standings for all seasons",
            data_type: DataType::Teams,
            implemented: true,
            parameters: vec![],
            test_params: HashMap::new(),
            example: "pucksdata teams standings-season",
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
            example: "pucksdata teams roster-season EDM 20232024",
        },
        Endpoint {
            name: "team_schedule_season",
            url: "https://api-web.nhle.com/v1/club-schedule-season/{team_code}/{season}",
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
            example: "pucksdata teams schedule-season EDM 20232024",
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
            example: "pucksdata teams schedule-month EDM 2024-02-15",
        },
    ]
});
