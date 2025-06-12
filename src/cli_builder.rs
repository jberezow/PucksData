use clap::{Command, Arg, ArgMatches};
use crate::endpoints::{DataType, Endpoint, get_endpoint, get_all_endpoints};
use crate::ingest;
use crate::inspect;
use crate::db::DbPool;

/// Build the top-level CLI application
pub fn build_cli() -> Command {
    let mut app = Command::new("pucksdata")
        .about("NHL Stats Engine CLI - A tool for fetching and caching NHL data")
        .version(env!("CARGO_PKG_VERSION"));
    
    // Add predefined commands for the most common use cases
    app = app.subcommand(build_games_commands());
    app = app.subcommand(build_players_commands());
    app = app.subcommand(build_teams_commands());
    app = app.subcommand(build_schedule_commands());
    app = app.subcommand(build_playoff_commands());
    
    // Add special inspect command
    app = app.subcommand(
        Command::new("inspect")
            .about("Inspect API endpoints")
            .arg(Arg::new("data_type").help("The data type (games, players, etc.)").required(true))
            .arg(Arg::new("endpoint").help("The endpoint name (e.g. boxscore, summary)").required(true))
            .arg(Arg::new("params").help("Parameters for the endpoint (key=value pairs)").action(clap::ArgAction::Append))
    );
    
    // Add list-endpoints command to help discovery
    app = app.subcommand(
        Command::new("list-endpoints")
            .about("List available API endpoints")
            .arg(Arg::new("data_type").help("Filter by data type (games, players, etc.)").required(false))
    );
    
    // Add database inspection command
    app = app.subcommand(
        Command::new("inspect-db")
            .about("Inspect what's in the raw_data database table")
    );
    
    app
}

/// Build commands for games data
fn build_games_commands() -> Command {
    let mut cmd = Command::new("games")
        .about("Games-related data operations");
    
    // Manually add each game endpoint
    cmd = cmd.subcommand(
        Command::new("story")
            .about("Fetch a game story by game ID")
            .arg(Arg::new("game_id")
                .help("The NHL game ID")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("boxscore")
            .about("Fetch a game boxscore by game ID")
            .arg(Arg::new("game_id")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("play-by-play")
            .about("Fetch play-by-play data by game ID")
            .arg(Arg::new("game_id")
                .help("The NHL game ID")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("all")
            .about("Fetch all games data")
    );
    
    cmd = cmd.subcommand(
        Command::new("content")
            .about("Fetch game content")
            .arg(Arg::new("game_id")
                .help("The NHL game ID")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("goal-replay")
            .about("Fetch goal replay")
            .arg(Arg::new("game_id")
                .help("The NHL game ID")
                .required(true))
            .arg(Arg::new("event_id")
                .help("The event ID for the goal")
                .required(true))
    );

    cmd = cmd.subcommand(
        Command::new("scores-date")
            .about("Fetch scores by date")
            .arg(Arg::new("date")
                .help("The date (YYYY-MM-DD)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("complete")
            .about("Fetch complete game data from multiple endpoints")
            .arg(Arg::new("game_id")
                .help("The NHL game ID")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("bulk-import")
            .about("Bulk import NHL games (be careful!)")
            .arg(Arg::new("games_file")
                .help("Path to the games JSON file")
                .long("games-file")
                .default_value("data/raw/games/game_all_games.json"))
            .arg(Arg::new("endpoints")
                .help("Endpoints to fetch (comma-separated)")
                .long("endpoints")
                .default_value("game_boxscore,game_play_by_play,game_story"))
            .arg(Arg::new("batch_size")
                .help("Number of games to process in each batch")
                .long("batch-size")
                .default_value("50"))
            .arg(Arg::new("max_retries")
                .help("Maximum retry attempts per request")
                .long("max-retries")
                .default_value("3"))
            .arg(Arg::new("start_year")
                .help("Only process games from this year forward (e.g., 2020)")
                .long("start-year")
                .short('y'))
            .arg(Arg::new("structured_only")
                .help("Only update structured tables (games, teams, players) from existing raw data")
                .long("structured-only")
                .action(clap::ArgAction::SetTrue))
            .arg(Arg::new("dry_run")
                .help("Show what would be done without actually doing it")
                .long("dry-run")
                .action(clap::ArgAction::SetTrue))
    );
    
    cmd
}

/// Build commands for players data
fn build_players_commands() -> Command {
    let mut cmd = Command::new("players")
        .about("Players-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("summary")
            .about("Fetch a player summary by player ID")
            .arg(Arg::new("player_id")
                .help("The NHL player ID")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("all")
            .about("Fetch all players data")
    );

    cmd = cmd.subcommand(
        Command::new("game-log")
            .about("Fetch player game log for a specific season and game type")
            .arg(Arg::new("player_id")
                .help("The NHL player ID")
                .required(true))
            .arg(Arg::new("season")
                .help("The season (YYYYYYYY)")
                .required(true))
            .arg(Arg::new("game_type")
                .help("The game type (2=Reg, 3=Post)")
                .required(true))
    );

    cmd
}

/// Build commands for teams data
fn build_teams_commands() -> Command {
    let mut cmd = Command::new("teams")
        .about("Teams-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("current-stats")
            .about("Fetch current team statistics")
            .arg(Arg::new("team_code")
                .help("The NHL team code (e.g. TOR)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("stats-by-season")
            .about("Fetch team statistics for a specific season and game type")
            .arg(Arg::new("team_code")
                .help("The NHL team code")
                .required(true))
            .arg(Arg::new("season")
                .help("The season (YYYYYYYY)")
                .required(true))
            .arg(Arg::new("game_type")
                .help("The game type (2=Reg, 3=Post)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("standings-by-date")
            .about("Fetch standings by date")
            .arg(Arg::new("date")
                .help("The date (YYYY-MM-DD)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("standings-season")
            .about("Fetch standings for a specific season")
    );
    
    cmd = cmd.subcommand(
        Command::new("roster-season")
            .about("Fetch team roster for a specific season")
            .arg(Arg::new("team_code")
                .help("The NHL team code")
                .required(true))
            .arg(Arg::new("season")
                .help("The season (YYYYYYYY)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("prospects")
            .about("Fetch team prospects")
            .arg(Arg::new("team_code")
                .help("The NHL team code")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("schedule-season")
            .about("Fetch team schedule for a specific season")
            .arg(Arg::new("team_code")
                .help("The NHL team code")
                .required(true))
            .arg(Arg::new("season")
                .help("The season (YYYYYYYY)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("schedule-month")
            .about("Fetch team schedule for a specific month")
            .arg(Arg::new("team_code")
                .help("The NHL team code")
                .required(true))
            .arg(Arg::new("date")
                .help("The date (YYYY-MM-DD)")
                .required(true))
    );
    
    cmd
}

/// Build commands for schedule data
fn build_schedule_commands() -> Command {
    let mut cmd = Command::new("schedule")
        .about("Schedule-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("by-date")
            .about("Fetch schedule by date")
            .arg(Arg::new("date")
                .help("The date (YYYY-MM-DD)")
                .required(true))
    );
    
    cmd
}

/// Build commands for playoff data
fn build_playoff_commands() -> Command {
    let mut cmd = Command::new("playoffs")
        .about("Playoff-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("bracket")
            .about("Fetch playoff bracket")
            .arg(Arg::new("year")
                .help("The year (YYYY)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("series-metadata")
            .about("Fetch playoff series metadata")
            .arg(Arg::new("season")
                .help("The season (YYYYYYYY)")
                .required(true))
            .arg(Arg::new("letter")
                .help("The series letter")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("standings-season")
            .about("Fetch playoff standings for a specific season")
            .arg(Arg::new("season")
                .help("The season (YYYYYYYY)")
                .required(true))
    );
    
    cmd = cmd.subcommand(
        Command::new("series-schedule")
            .about("Fetch playoff series schedule")
            .arg(Arg::new("year")
                .help("The year (YYYY)")
                .required(true))
            .arg(Arg::new("letter")
                .help("The series letter")
                .required(true))
    );
    
    cmd
}

/// Main command handler
pub async fn handle_command(matches: &ArgMatches, pool: DbPool) -> Result<(), Box<dyn std::error::Error>> {
    match matches.subcommand() {
        Some(("games", sub_matches)) => handle_data_type_command(DataType::Games, sub_matches, pool).await,
        Some(("players", sub_matches)) => handle_data_type_command(DataType::Players, sub_matches, pool).await,
        Some(("teams", sub_matches)) => handle_data_type_command(DataType::Teams, sub_matches, pool).await,
        Some(("schedule", sub_matches)) => handle_data_type_command(DataType::Schedule, sub_matches, pool).await,
        Some(("playoffs", sub_matches)) => handle_data_type_command(DataType::Playoffs, sub_matches, pool).await,
        Some(("inspect", sub_matches)) => handle_inspect_command(sub_matches).await,
        Some(("list-endpoints", sub_matches)) => handle_list_endpoints_command(sub_matches),
        Some(("inspect-db", _)) => {
            if let Err(e) = crate::db::inspect_raw_data_table(&pool).await {
                eprintln!("Error inspecting database: {}", e);
            }
            Ok(())
        },
        _ => {
            // No subcommand provided, so we'll print help
            // You might need to adjust this part depending on clap's version and your preference
            build_cli().print_help()?;
            Ok(())
        }
    }
}

/// Generic handler for data type commands (games, players, etc.)
async fn handle_data_type_command(data_type: DataType, matches: &ArgMatches, pool: DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let subcommand_name = matches.subcommand_name().unwrap_or("default");
    
    // Special handling for bulk import
    if data_type == DataType::Games && subcommand_name == "bulk-import" {
        let sub_matches = matches.subcommand().unwrap().1;
        
        let games_file = sub_matches.get_one::<String>("games_file").unwrap();
        let endpoints_str = sub_matches.get_one::<String>("endpoints").unwrap();
        let batch_size: usize = sub_matches.get_one::<String>("batch_size").unwrap()
            .parse().map_err(|_| "Invalid batch size")?;
        let max_retries: u32 = sub_matches.get_one::<String>("max_retries").unwrap()
            .parse().map_err(|_| "Invalid max retries")?;
        let start_year: Option<i32> = sub_matches.get_one::<String>("start_year")
            .map(|s| s.parse().map_err(|_| "Invalid start year"))
            .transpose()?;
        let structured_only = sub_matches.get_flag("structured_only");
        let dry_run = sub_matches.get_flag("dry_run");
        
        let endpoints: Vec<&str> = endpoints_str.split(',').map(|s| s.trim()).collect();
        
        if dry_run {
            println!("🔍 DRY RUN - This is what would be done:");
            println!("   📄 Games file: {}", games_file);
            println!("   🎯 Endpoints: {:?}", endpoints);
            println!("   📦 Batch size: {}", batch_size);
            println!("   🔄 Max retries: {}", max_retries);
            if let Some(year) = start_year {
                println!("   📅 Start year: {} (games from {} onwards)", year, year);
            } else {
                println!("   📅 All years (72,000+ games from 1917 onwards)");
            }
            println!("   🏗️  Structured tables: {}", if structured_only { "ONLY structured" } else { "Raw + structured" });
            println!("   ⏱️  Estimated time: ~{} hours at safe rates", 
                    if start_year.is_some() { "2-4" } else { "6-10" });
            println!("\nUse without --dry-run to actually run the import.");
            return Ok(());
        }
        
        println!("⚠️  WARNING: This will attempt to download data for ALL NHL games!");
        println!("📊 This includes 72,000+ games from 1917 to present");
        println!("⏱️  This may take 6-10 hours to complete");
        println!("🌐 Rate limiting is enabled to be respectful to NHL servers");
        println!();
        println!("Press Ctrl+C within 10 seconds to cancel...");
        
        for i in (1..=10).rev() {
            print!("\rStarting in {} seconds...", i);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        println!("\n🚀 Starting bulk import...");
        
        return crate::ingest::bulk_import_all_games(
            games_file,
            pool,
            &endpoints,
            max_retries,
            batch_size,
            start_year,
            structured_only,
        ).await;
    }
    
    // Special handling for complete game data
    if data_type == DataType::Games && subcommand_name == "complete" {
        let sub_matches = matches.subcommand().unwrap().1;
        if let Some(game_id_str) = sub_matches.get_one::<String>("game_id") {
            let game_id: i64 = game_id_str.parse()
                .map_err(|_| "Invalid game ID format")?;
            return crate::db::fetch_complete_game_data(game_id, pool).await;
        } else {
            return Err("game_id is required for complete command".into());
        }
    }
    
    let endpoint_name = match (data_type, subcommand_name) {
        (DataType::Games, "story") => "game_story",
        (DataType::Games, "boxscore") => "game_boxscore",
        (DataType::Games, "play-by-play") => "game_play_by_play",
        (DataType::Games, "all") => "games_all", // A placeholder
        (DataType::Games, "content") => "game_content",
        (DataType::Games, "goal-replay") => "game_goal_replay",
        (DataType::Games, "scores-date") => "scores_by_date",
        (DataType::Games, "complete") => return Err("Complete command should be handled above".into()),
        (DataType::Players, "summary") => "player_summary",
        (DataType::Players, "all") => "players_all", // Placeholder
        (DataType::Players, "game-log") => "player_game_log",
        (DataType::Teams, "current-stats") => "team_current_stats",
        (DataType::Teams, "stats-by-season") => "team_stats_by_season",
        (DataType::Teams, "standings-by-date") => "team_standings_date",
        (DataType::Teams, "standings-season") => "team_standings_season",
        (DataType::Teams, "roster-season") => "team_roster_season",
        (DataType::Teams, "prospects") => "team_prospects",
        (DataType::Teams, "schedule-season") => "team_schedule_season",
        (DataType::Teams, "schedule-month") => "team_schedule_month",
        (DataType::Schedule, "by-date") => "schedule_by_date",
        (DataType::Playoffs, "bracket") => "playoff_bracket",
        (DataType::Playoffs, "series-metadata") => "playoff_series_metadata",
        (DataType::Playoffs, "series-schedule") => "playoff_series_schedule",
        _ => return Err(format!("Unknown command for {:?}", data_type).into())
    };

    let endpoint = get_endpoint(endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;

    let sub_matches = matches.subcommand().unwrap().1;
    let mut params = Vec::new();
    for param_def in &endpoint.parameters {
        if let Some(value) = sub_matches.get_one::<String>(param_def.name) {
            params.push((param_def.name, value.as_str()));
        }
    }
    
    ingest::fetch_endpoint(endpoint_name, &params, pool).await
}

/// Handler for the 'inspect' command
async fn handle_inspect_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let data_type_str = matches.get_one::<String>("data_type")
        .ok_or("data_type is required for inspect")?;
    let endpoint_str = matches.get_one::<String>("endpoint")
        .ok_or("endpoint is required for inspect")?;

    let endpoint_name = format!("{}_{}", data_type_str.trim(), endpoint_str.replace('-', "_"));
    let endpoint = get_endpoint(&endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;

    let mut params = Vec::new();
    if let Some(values) = matches.get_many::<String>("params") {
        for val in values {
            let parts: Vec<&str> = val.split('=').collect();
            if parts.len() == 2 {
                params.push((parts[0], parts[1]));
            } else {
                return Err(format!("Invalid parameter format: {}", val).into());
            }
        }
    }

    inspect::inspect_endpoint(endpoint, &params).await
}

/// Handle the list-endpoints command
fn handle_list_endpoints_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let filter_type = matches.get_one::<String>("data_type").map(|s| s.as_str());
    
    println!("Available API Endpoints:");
    println!("=======================");
    
    let all_endpoints = get_all_endpoints();
    
    // Group endpoints by data type
    let mut by_type: std::collections::HashMap<DataType, Vec<&Endpoint>> = std::collections::HashMap::new();
    
    for endpoint in all_endpoints {
        if let Some(filter) = filter_type {
            if endpoint.data_type.as_str() != filter {
                continue;
            }
        }
        
        by_type.entry(endpoint.data_type).or_default().push(endpoint);
    }
    
    // Display endpoints by type
    for (data_type, endpoints) in by_type.iter() {
        println!("\n{} ({}):", data_type.as_str(), endpoints.len());
        println!("{}", "-".repeat(data_type.as_str().len() + 4));
        
        for endpoint in endpoints {
            let name_parts: Vec<&str> = endpoint.name.split('_').collect();
            if name_parts.len() > 1 {
                let subcommand_name = name_parts[1..].join("-");
                
                println!("  {} {} - {}", data_type.as_str(), subcommand_name, endpoint.description);
                
                // Print required parameters if any
                let required_params: Vec<_> = endpoint.parameters.iter()
                    .filter(|p| p.required)
                    .collect();
                
                if !required_params.is_empty() {
                    println!("    Required parameters:");
                    for param in required_params {
                        println!("      {} - {} (example: {})", param.name, param.description, param.example);
                    }
                }
                
                println!("    Example: {}", endpoint.example);
                println!();
            }
        }
    }
    
    Ok(())
}

/// An example main function to demonstrate CLI usage
pub async fn example_main(pool: DbPool) {
    let matches = build_cli().get_matches();
    if let Err(e) = handle_command(&matches, pool).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
} 