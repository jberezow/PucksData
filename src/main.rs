// src/main.rs

mod ingest;
mod api;
mod cache;

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
    /// Team-related data operations
    /// 
    /// Fetch team statistics and information.
    /// All team data is cached in data/raw/teams/.
    Team {
        #[command(subcommand)]
        subcommand: TeamCommands,
    },
    /// Season-related data operations
    /// 
    /// Fetch season statistics and information.
    /// All season data is cached in data/raw/seasons/.
    Season {
        #[command(subcommand)]
        subcommand: SeasonCommands,
    },
}

#[derive(Subcommand)]
enum GameCommands {
    /// Fetch a game story by game ID
    /// 
    /// Example: pucksdata game story 2023020001
    /// 
    /// The game ID format is typically: YYYYGGGGGG where:
    /// - YYYY is the season year
    /// - GGGGGG is the game number
    Story {
        /// The NHL game ID (e.g., 2023020001)
        game_id: String,
    },
    /// Fetch a game boxscore by game ID
    /// 
    /// Example: pucksdata game boxscore 2023020001
    Boxscore {
        /// The NHL game ID (e.g., 2023020001)
        game_id: String,
    },
    /// Fetch play-by-play data by game ID
    /// 
    /// Example: pucksdata game play-by-play 2023020001
    PlayByPlay {
        /// The NHL game ID (e.g., 2023020001)
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
}

#[derive(Subcommand)]
enum PlayerCommands {
    /// Fetch a player summary by player ID
    /// 
    /// Example: pucksdata player summary 8478402
    /// 
    Summary {
        /// The NHL player ID (e.g., 8478402)
        player_id: String,
    },
    /// Fetch all players data
    /// 
    /// Example: pucksdata player all
    /// 
    /// This command fetches the complete list of players from the NHL API.
    All,
    
}

#[derive(Subcommand)]
enum TeamCommands {
    /// Fetch current team statistics
    /// 
    /// Example: pucksdata team current-stats TOR
    /// 
    /// Team codes are typically 3-letter abbreviations (e.g., TOR, BOS, NYR)
    CurrentStats {
        /// The NHL team code (e.g., TOR, BOS, NYR)
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
        /// The NHL team code (e.g., TOR, BOS, NYR)
        team_code: String,
        /// The season ID (e.g., 20232024)
        season_id: String,
        /// The game type (2 for Regular Season, 3 for Playoffs)
        game_type: String,
    },
    /// Fetch all teams data
    /// 
    /// Example: pucksdata team all
    /// 
    /// This command fetches the complete list of teams from the NHL API.
    All,
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
        },
        Commands::Player { subcommand } => match subcommand {
            PlayerCommands::All => {
                // TODO: Implement player data fetching
                println!("Player data fetching not yet implemented");
            }
            PlayerCommands::Summary {player_id} => {
                if let Err(e) = ingest::fetch_player_summary(&player_id) {
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
        },
        Commands::Season { subcommand } => match subcommand {
            SeasonCommands::All => {
                // TODO: Implement season data fetching
                println!("Season data fetching not yet implemented");
            }
        },
    }
}