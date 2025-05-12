// src/main.rs

use pucksdata::{ingest, inspect};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pucksdata")]
#[command(about = "NHL Stats Engine CLI - A tool for fetching and caching NHL data")]
#[command(long_about = "A command-line tool for fetching, caching, and processing NHL data. 
Supports various data types including games, players, teams, and seasons.
All data is cached locally in the data/raw directory for offline access.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Game-related data operations
    /// 
    /// Fetch various types of game data including stories, boxscores, and play-by-play information.
    /// All game data is cached in data/raw/games/.
    Game {
        #[command(subcommand)]
        subcommand: GameCommands,
    },
    /// Player-related data operations
    /// 
    /// Fetch player statistics and information.
    /// All player data is cached in data/raw/players/.
    Player {
        #[command(subcommand)]
        subcommand: PlayerCommands,
    },
    /// Skater-related data operations
    Skater {
        #[command(subcommand)]
        subcommand: SkaterCommands,
    },
    /// Goalie-related data operations
    Goalie {
        #[command(subcommand)]
        subcommand: GoalieCommands,
    },
    /// Team-related data operations
    /// 
    /// Fetch team statistics and information.
    /// All team data is cached in data/raw/teams/.
    Team {
        #[command(subcommand)]
        subcommand: TeamCommands,
    },
    /// Schedule-related data operations
    Schedule {
        #[command(subcommand)]
        subcommand: ScheduleCommands,
    },
    /// Playoff-related data operations
    Playoff {
        #[command(subcommand)]
        subcommand: PlayoffCommands,
    },
    /// Season-related data operations
    /// 
    /// Fetch season statistics and information.
    /// All season data is cached in data/raw/seasons/.
    Season {
        #[command(subcommand)]
        subcommand: SeasonCommands,
    },
    /// Draft-related data operations
    /// 
    /// Fetch draft rankings and information.
    /// All draft data is cached in data/raw/draft/.
    Draft {
        #[command(subcommand)]
        subcommand: DraftCommands,
    },
    /// Miscellaneous data operations
    Misc {
        #[command(subcommand)]
        subcommand: MiscCommands,
    },
    /// Inspect API endpoints
    Inspect {
        /// The data type (games, players, etc.)
        data_type: String,
        /// The endpoint (e.g. story, boxscore)
        endpoint: String,
        /// A valid ID to call the API (e.g. game_id or player_id)
        id: String,
    },
}

#[derive(Subcommand)]
enum GameCommands {
    /// Fetch a game story by game ID
    /// 
    /// Example: pucksdata game story 2023020001
    /// 
    /// The game ID format is: YYYYTTGGGG where:
    /// - YYYY is the season year (e.g., 2023)
    /// - TT is the type (01 for regular season, 02 for playoffs)
    /// - GGGG is the game number (e.g., 0001)
    Story {
        /// The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)
        #[arg(value_name = "GAME_ID")]
        game_id: String,
    },
    /// Fetch a game boxscore by game ID
    /// 
    /// Example: pucksdata game boxscore 2023020001
    /// 
    /// The game ID format is: YYYYTTGGGG
    Boxscore {
        /// The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)
        #[arg(value_name = "GAME_ID")]
        game_id: String,
    },
    /// Fetch play-by-play data by game ID
    /// 
    /// Example: pucksdata game play-by-play 2023020001
    PlayByPlay {
        /// The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)
        #[arg(value_name = "GAME_ID")]
        game_id: String,
    },
    /// Fetch all games data
    /// 
    /// Example: pucksdata game all
    /// 
    /// This command fetches the complete list of games from the NHL API.
    All,
    /// Fetch game metadata
    /// 
    /// Example: pucksdata game metadata
    /// 
    /// This command fetches metadata about games, including available seasons and game types.
    Metadata,
    /// Fetch game content
    Content {
        /// The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)
        #[arg(value_name = "GAME_ID")]
        game_id: String,
    },
    /// Fetch goal replay
    GoalReplay {
        /// The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)
        #[arg(value_name = "GAME_ID")]
        game_id: String,
        /// The event ID for the goal (format: numeric, e.g., 401)
        #[arg(value_name = "EVENT_ID")]
        event_id: String,
    },
    /// Fetch game odds
    Odds {
        /// The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)
        #[arg(value_name = "GAME_ID")]
        game_id: String,
    },
    /// Fetch current scores
    ScoresNow,
    /// Fetch scores by date
    ScoresDate {
        /// The date (format: YYYY-MM-DD, e.g., 2024-02-15)
        #[arg(value_name = "DATE")]
        date: String,
    },
}

#[derive(Subcommand)]
enum PlayerCommands {
    /// Fetch a player summary by player ID
    /// 
    /// Example: pucksdata player summary 8478402
    /// 
    Summary {
        /// The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)
        #[arg(value_name = "PLAYER_ID")]
        player_id: String,
    },
    /// Fetch all players data
    /// 
    /// Example: pucksdata player all
    /// 
    /// This command fetches the complete list of players from the NHL API.
    All,
    /// Fetch player game log
    GameLog {
        /// The NHL player ID (format: numeric, e.g., 8478402)
        #[arg(value_name = "PLAYER_ID")]
        player_id: String,
        /// The season (format: YYYYYYYY, e.g., 20232024)
        #[arg(value_name = "SEASON")]
        season: String,
    },
    /// Fetch current player game log
    GameLogNow {
        /// The NHL player ID (format: numeric, e.g., 8478402)
        #[arg(value_name = "PLAYER_ID")]
        player_id: String,
    },
    /// Fetch player spotlight
    Spotlight,
}

#[derive(Subcommand)]
enum SkaterCommands {
    /// Fetch current skater leaders
    LeadersNow,
    /// Fetch skater leaders by season
    Leaders {
        /// The season (format: YYYYYYYY, e.g., 20232024)
        #[arg(value_name = "SEASON")]
        season: String,
        /// The game type (2 for Regular Season, 3 for Playoffs)
        #[arg(value_name = "GAME_TYPE")]
        game_type: String,
    },
}

#[derive(Subcommand)]
enum GoalieCommands {
    /// Fetch current goalie leaders
    LeadersNow,
    /// Fetch goalie leaders by season
    Leaders {
        /// The season (format: YYYYYYYY, e.g., 20232024)
        #[arg(value_name = "SEASON")]
        season: String,
        /// The game type (2 for Regular Season, 3 for Playoffs)
        #[arg(value_name = "GAME_TYPE")]
        game_type: String,
    },
}

#[derive(Subcommand)]
enum TeamCommands {
    /// Fetch current team statistics
    /// 
    /// Example: pucksdata team current-stats TOR
    /// 
    /// Team codes are typically 3-letter abbreviations (e.g., TOR, BOS, NYR)
    CurrentStats {
        /// The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)
        #[arg(value_name = "TEAM_CODE")]
        team_code: String,
    },
    /// Fetch team statistics for a specific season
    /// 
    /// Example: pucksdata team season-stats TOR 20232024 2
    /// 
    /// Game types:
    /// - 2: Regular Season
    /// - 3: Playoffs
    SeasonStats {
        /// The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)
        #[arg(value_name = "TEAM_CODE")]
        team_code: String,
        /// The season ID (format: YYYYYYYY, e.g., 20232024)
        #[arg(value_name = "SEASON")]
        season_id: String,
        /// The game type (2 for Regular Season, 3 for Playoffs)
        #[arg(value_name = "GAME_TYPE")]
        game_type: String,
    },
    /// Fetch all teams data
    /// 
    /// Example: pucksdata team all
    /// 
    /// This command fetches the complete list of teams from the NHL API.
    All,
    /// Fetch current standings
    StandingsNow,
    /// Fetch standings by date
    StandingsDate {
        /// The date (format: YYYY-MM-DD, e.g., 2024-02-15)
        #[arg(value_name = "DATE")]
        date: String,
    },
    /// Fetch current roster
    RosterNow {
        /// The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)
        #[arg(value_name = "TEAM_CODE")]
        team_code: String,
    },
    /// Fetch roster by season
    RosterSeason {
        /// The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)
        #[arg(value_name = "TEAM_CODE")]
        team_code: String,
        /// The season (format: YYYYYYYY, e.g., 20232024)
        #[arg(value_name = "SEASON")]
        season: String,
    },
}

#[derive(Subcommand)]
enum ScheduleCommands {
    /// Fetch current schedule
    Now,
    /// Fetch schedule by date
    Date {
        /// The date (format: YYYY-MM-DD, e.g., 2024-02-15)
        #[arg(value_name = "DATE")]
        date: String,
    },
}

#[derive(Subcommand)]
enum PlayoffCommands {
    /// Fetch playoff bracket
    Bracket,
    /// Fetch playoff series schedule
    SeriesSchedule,
}

#[derive(Subcommand)]
enum SeasonCommands {
    /// Fetch all seasons data
    /// 
    /// Example: pucksdata season all
    /// 
    /// This command fetches the complete list of seasons from the NHL API.
    All,
}

#[derive(Subcommand)]
enum DraftCommands {
    /// Fetch current draft rankings
    /// 
    /// Example: pucksdata draft current-rankings
    /// 
    /// This command fetches the current NHL draft rankings.
    CurrentRankings,
    /// Fetch current draft tracker
    TrackerNow,
    /// Fetch current draft picks
    PicksNow,
    /// Fetch draft picks by year
    Picks {
        /// The draft year (format: YYYY, e.g., 2024)
        #[arg(value_name = "YEAR")]
        year: String,
    },
}

#[derive(Subcommand)]
enum MiscCommands {
    /// Fetch postal code information
    PostalCode {
        /// The postal/zip code (format: 5-6 characters, e.g., M5V2K4 or 10001)
        #[arg(value_name = "CODE")]
        code: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Game { subcommand } => match subcommand {
            GameCommands::Story { game_id } => {
                if let Err(e) = ingest::fetch_game_story(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::Boxscore { game_id } => {
                if let Err(e) = ingest::fetch_game_boxscore(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::PlayByPlay { game_id } => {
                if let Err(e) = ingest::fetch_game_play_by_play(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::All => {
                if let Err(e) = ingest::fetch_game_all_games() {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::Metadata => {
                if let Err(e) = ingest::fetch_game_metadata() {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::Content { game_id } => {
                if let Err(e) = ingest::fetch_game_content(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::GoalReplay { game_id, event_id } => {
                if let Err(e) = ingest::fetch_game_goal_replay(&game_id, &event_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::Odds { game_id } => {
                if let Err(e) = ingest::fetch_game_odds(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::ScoresNow => {
                if let Err(e) = ingest::fetch_game_scores_now() {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::ScoresDate { date } => {
                if let Err(e) = ingest::fetch_game_scores_date(&date) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Player { subcommand } => match subcommand {
            PlayerCommands::All => {
                if let Err(e) = ingest::fetch_player_all() {
                    eprintln!("Error: {}", e);
                }
            }
            PlayerCommands::Summary { player_id } => {
                if let Err(e) = ingest::fetch_player_summary(&player_id) {
                    eprintln!("Error: {}", e);
                }
            }
            PlayerCommands::GameLog { player_id, season } => {
                if let Err(e) = ingest::fetch_player_game_log(&player_id, &season) {
                    eprintln!("Error: {}", e);
                }
            }
            PlayerCommands::GameLogNow { player_id } => {
                if let Err(e) = ingest::fetch_player_game_log_now(&player_id) {
                    eprintln!("Error: {}", e);
                }
            }
            PlayerCommands::Spotlight => {
                if let Err(e) = ingest::fetch_player_spotlight() {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Skater { subcommand } => match subcommand {
            SkaterCommands::LeadersNow => {
                if let Err(e) = ingest::fetch_skater_leaders_now() {
                    eprintln!("Error: {}", e);
                }
            }
            SkaterCommands::Leaders { season, game_type } => {
                if let Err(e) = ingest::fetch_skater_leaders(&season, &game_type) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Goalie { subcommand } => match subcommand {
            GoalieCommands::LeadersNow => {
                if let Err(e) = ingest::fetch_goalie_leaders_now() {
                    eprintln!("Error: {}", e);
                }
            }
            GoalieCommands::Leaders { season, game_type } => {
                if let Err(e) = ingest::fetch_goalie_leaders(&season, &game_type) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Team { subcommand } => match subcommand {
            TeamCommands::All => {
                // TODO: Implement team data fetching
                println!("Team data fetching not yet implemented");
            }
            TeamCommands::CurrentStats { team_code } => {
                if let Err(e) = ingest::fetch_team_current_stats(&team_code) {
                    eprintln!("Error: {}", e);
                }
            }
            TeamCommands::SeasonStats { team_code, season_id, game_type } => {
                if let Err(e) = ingest::fetch_team_stats_by_season(&team_code, &season_id, &game_type) {
                    eprintln!("Error: {}", e);
                }
            }
            TeamCommands::StandingsNow => {
                if let Err(e) = ingest::fetch_team_standings_now() {
                    eprintln!("Error: {}", e);
                }
            }
            TeamCommands::StandingsDate { date } => {
                if let Err(e) = ingest::fetch_team_standings_date(&date) {
                    eprintln!("Error: {}", e);
                }
            }
            TeamCommands::RosterNow { team_code } => {
                if let Err(e) = ingest::fetch_team_roster_now(&team_code) {
                    eprintln!("Error: {}", e);
                }
            }
            TeamCommands::RosterSeason { team_code, season } => {
                if let Err(e) = ingest::fetch_team_roster_season(&team_code, &season) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Schedule { subcommand } => match subcommand {
            ScheduleCommands::Now => {
                if let Err(e) = ingest::fetch_schedule_now() {
                    eprintln!("Error: {}", e);
                }
            }
            ScheduleCommands::Date { date } => {
                if let Err(e) = ingest::fetch_schedule_date(&date) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Playoff { subcommand } => match subcommand {
            PlayoffCommands::Bracket => {
                if let Err(e) = ingest::fetch_playoff_bracket() {
                    eprintln!("Error: {}", e);
                }
            }
            PlayoffCommands::SeriesSchedule => {
                if let Err(e) = ingest::fetch_playoff_series_schedule() {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Season { subcommand } => match subcommand {
            SeasonCommands::All => {
                if let Err(e) = ingest::fetch_season_all() {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Draft { subcommand } => match subcommand {
            DraftCommands::CurrentRankings => {
                if let Err(e) = ingest::fetch_draft_current_rankings() {
                    eprintln!("Error: {}", e);
                }
            }
            DraftCommands::TrackerNow => {
                if let Err(e) = ingest::fetch_draft_tracker_now() {
                    eprintln!("Error: {}", e);
                }
            }
            DraftCommands::PicksNow => {
                if let Err(e) = ingest::fetch_draft_picks_now() {
                    eprintln!("Error: {}", e);
                }
            }
            DraftCommands::Picks { year } => {
                if let Err(e) = ingest::fetch_draft_picks(&year) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Misc { subcommand } => match subcommand {
            MiscCommands::PostalCode { code } => {
                if let Err(e) = ingest::fetch_postal_code(&code) {
                    eprintln!("Error: {}", e);
                }
            }
        },
        Commands::Inspect { data_type, endpoint, id } => {
            if let Err(e) = inspect::inspect_keys(&data_type, &endpoint, &id) {
                eprintln!("Error: {}", e);
            }
        }
    }
}