// src/main.rs

mod ingest;
mod api;
mod cache;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pucksdata")]
#[command(about = "NHL Stats Engine CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Retrieve and cache game story data
    Game {
        #[command(subcommand)]
        subcommand: GameCommands,
    },
}

#[derive(Subcommand)]
enum GameCommands {
    /// Fetch a game story by game ID
    Story {
        game_id: String,
    },
    /// Fetch a game boxscore by game ID
    Boxscore {
        game_id: String,
    },
    /// Fetch a game summary by game ID
    Summary {
        game_id: String,
    },
    /// Fetch play-by-play data by game ID
    PlayByPlay {
        game_id: String,
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
            GameCommands::Summary { game_id } => {
                if let Err(e) = ingest::fetch_game_summary(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
            GameCommands::PlayByPlay { game_id } => {
                if let Err(e) = ingest::fetch_game_play_by_play(&game_id) {
                    eprintln!("Error: {}", e);
                }
            }
        },
    }
}