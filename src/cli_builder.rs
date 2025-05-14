use clap::{Command, Arg, ArgMatches};
use crate::endpoints::{DataType, Endpoint, get_endpoint, get_all_endpoints};
use crate::ingest;
use crate::inspect;

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
    app = app.subcommand(build_draft_commands());
    app = app.subcommand(build_skaters_commands());
    app = app.subcommand(build_goalies_commands());
    app = app.subcommand(build_seasons_commands());
    
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
                .help("The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)")
                .required(true)
                .value_name("GAME_ID"))
    );
    
    cmd = cmd.subcommand(
        Command::new("boxscore")
            .about("Fetch a game boxscore by game ID")
            .arg(Arg::new("game_id")
                .help("The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)")
                .required(true)
                .value_name("GAME_ID"))
    );
    
    cmd = cmd.subcommand(
        Command::new("play-by-play")
            .about("Fetch play-by-play data by game ID")
            .arg(Arg::new("game_id")
                .help("The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)")
                .required(true)
                .value_name("GAME_ID"))
    );
    
    cmd = cmd.subcommand(
        Command::new("all")
            .about("Fetch all games data")
    );
    
    cmd = cmd.subcommand(
        Command::new("content")
            .about("Fetch game content")
            .arg(Arg::new("game_id")
                .help("The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)")
                .required(true)
                .value_name("GAME_ID"))
    );
    
    cmd = cmd.subcommand(
        Command::new("goal-replay")
            .about("Fetch goal replay")
            .arg(Arg::new("game_id")
                .help("The NHL game ID (format: YYYYTTGGGG, e.g., 2023020001)")
                .required(true)
                .value_name("GAME_ID"))
            .arg(Arg::new("event_id")
                .help("The event ID for the goal (format: numeric, e.g., 401)")
                .required(true)
                .value_name("EVENT_ID"))
    );
    
    cmd = cmd.subcommand(
        Command::new("scores-now")
            .about("Fetch current scores")
    );
    
    cmd = cmd.subcommand(
        Command::new("scores-date")
            .about("Fetch scores by date")
            .arg(Arg::new("date")
                .help("The date (format: YYYY-MM-DD, e.g., 2024-02-15)")
                .required(true)
                .value_name("DATE"))
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
                .help("The NHL player ID (format: numeric, e.g., 8478402 for Connor McDavid)")
                .required(true)
                .value_name("PLAYER_ID"))
    );
    
    cmd = cmd.subcommand(
        Command::new("all")
            .about("Fetch all players data")
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
                .help("The NHL team code (format: 3 letters, e.g., TOR, BOS, NYR, LAK)")
                .required(true)
                .value_name("TEAM_CODE"))
    );
    
    cmd = cmd.subcommand(
        Command::new("standings-now")
            .about("Fetch current standings")
    );
    
    cmd
}

/// Build commands for schedule data
fn build_schedule_commands() -> Command {
    let mut cmd = Command::new("schedule")
        .about("Schedule-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("now")
            .about("Fetch current schedule")
    );
    
    cmd = cmd.subcommand(
        Command::new("date")
            .about("Fetch schedule by date")
            .arg(Arg::new("date")
                .help("The date (format: YYYY-MM-DD, e.g., 2024-02-15)")
                .required(true)
                .value_name("DATE"))
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
                .help("The year (format: YYYY, e.g., 2024)")
                .required(true)
                .value_name("YEAR"))
    );
    
    cmd
}

/// Build commands for draft data
fn build_draft_commands() -> Command {
    let mut cmd = Command::new("draft")
        .about("Draft-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("current-rankings")
            .about("Fetch current draft rankings")
    );
    
    cmd
}

/// Build commands for skaters data
fn build_skaters_commands() -> Command {
    let mut cmd = Command::new("skaters")
        .about("Skater statistics-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("leaders-now")
            .about("Fetch current skater stats leaders")
    );
    
    cmd = cmd.subcommand(
        Command::new("leaders")
            .about("Fetch skater stats leaders for a specific season and game type")
            .arg(Arg::new("season")
                .help("The season (format: YYYYYYYY, e.g., 20232024)")
                .required(true)
                .value_name("SEASON"))
            .arg(Arg::new("game_type")
                .help("The game type (e.g., 2 for regular season, 3 for playoffs)")
                .required(true)
                .value_name("GAME_TYPE"))
    );
    
    cmd
}

/// Build commands for goalies data
fn build_goalies_commands() -> Command {
    let mut cmd = Command::new("goalies")
        .about("Goalie statistics-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("leaders-now")
            .about("Fetch current goalie stats leaders")
    );
    
    cmd = cmd.subcommand(
        Command::new("leaders")
            .about("Fetch goalie stats leaders for a specific season and game type")
            .arg(Arg::new("season")
                .help("The season (format: YYYYYYYY, e.g., 20232024)")
                .required(true)
                .value_name("SEASON"))
            .arg(Arg::new("game_type")
                .help("The game type (e.g., 2 for regular season, 3 for playoffs)")
                .required(true)
                .value_name("GAME_TYPE"))
    );
    
    cmd
}

/// Build commands for seasons data
fn build_seasons_commands() -> Command {
    let mut cmd = Command::new("seasons")
        .about("Season-related data operations");
    
    cmd = cmd.subcommand(
        Command::new("all")
            .about("Fetch all seasons data")
    );
    
    cmd
}

/// Handle CLI commands by executing appropriate functions
pub fn handle_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    match matches.subcommand() {
        Some((data_type_str, data_type_matches)) => {
            // Handle list-endpoints command
            if data_type_str == "list-endpoints" {
                return handle_list_endpoints_command(data_type_matches);
            }
            
            // Convert string to DataType enum
            let data_type = match data_type_str {
                "games" => DataType::Games,
                "players" => DataType::Players,
                "teams" => DataType::Teams,
                "schedule" => DataType::Schedule,
                "playoffs" => DataType::Playoffs,
                "draft" => DataType::Draft,
                "skaters" => DataType::Skaters,
                "goalies" => DataType::Goalies,
                "seasons" => DataType::Seasons,
                "inspect" => {
                    // Special case for inspect command
                    return handle_inspect_command(data_type_matches);
                }
                _ => return Err(format!("Unknown data type: {}", data_type_str).into()),
            };
            
            handle_data_type_command(data_type, data_type_matches)
        }
        None => {
            Err("No command specified. Use --help for usage information.".into())
        }
    }
}

/// Handle commands for a specific data type
fn handle_data_type_command(data_type: DataType, matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    // Get the subcommand
    match matches.subcommand() {
        Some((subcmd_name, subcmd_matches)) => {
            // Convert kebab-case back to snake_case
            let snake_name = subcmd_name.replace('-', "_");
            
            // Convert data_type to singular form for endpoint lookup
            let singular_type = match data_type.as_str() {
                "games" => "game",
                "players" => "player",
                "teams" => "team",
                "skaters" => "skater",
                "goalies" => "goalie",
                "playoffs" => "playoff",
                "seasons" => "season",
                "draft" => "draft",
                "schedule" => "schedule",
                _ => data_type.as_str(),
            };
            
            // Build endpoint name correctly (with singular type)
            let endpoint_name = format!("{}_{}", singular_type, snake_name);
            
            // Get the endpoint definition
            let endpoint = get_endpoint(&endpoint_name)
                .ok_or_else(|| format!("Endpoint not found: {}", endpoint_name))?;
            
            // Build parameters from matches
            let mut params = Vec::new();
            for param in &endpoint.parameters {
                if let Some(value) = subcmd_matches.get_one::<String>(param.name) {
                    params.push((param.name, value.as_str()));
                }
            }
            
            // Call the endpoint
            ingest::fetch_endpoint(&endpoint_name, &params)
        }
        None => {
            Err(format!("No {} subcommand specified. Use --help for usage information.", 
                       data_type.as_str()).into())
        }
    }
}

/// Handle the special inspect command
fn handle_inspect_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let data_type_str = matches.get_one::<String>("data_type")
        .ok_or("data_type is required")?;
    let endpoint_str = matches.get_one::<String>("endpoint")
        .ok_or("endpoint is required")?;
    
    // Convert data_type string to enum
    let data_type = match data_type_str.as_str() {
        "games" => DataType::Games,
        "players" => DataType::Players,
        "teams" => DataType::Teams,
        "schedule" => DataType::Schedule,
        "playoffs" => DataType::Playoffs,
        "draft" => DataType::Draft,
        "skaters" => DataType::Skaters,
        "goalies" => DataType::Goalies,
        "seasons" => DataType::Seasons,
        _ => return Err(format!("Unknown data type: {}", data_type_str).into()),
    };
    
    // First try with pluralized form
    let endpoint_name = format!("{}_{}", data_type.as_str(), endpoint_str.replace('-', "_"));
    
    // Get endpoint from registry
    let endpoint = match get_endpoint(&endpoint_name) {
        Some(ep) => ep,
        None => {
            // Try with singularized form if not found
            let singular_type = data_type.as_str().strip_suffix('s').unwrap_or(data_type.as_str());
            let alt_endpoint_name = format!("{}_{}", singular_type, endpoint_str.replace('-', "_"));
            get_endpoint(&alt_endpoint_name)
                .ok_or_else(|| format!("Endpoint not found: {} or {}", endpoint_name, alt_endpoint_name))?
        }
    };
    
    // Extract parameters from arguments
    let mut params = Vec::new();
    if let Some(param_values) = matches.get_many::<String>("params") {
        for param_value in param_values {
            if let Some((key, value)) = param_value.split_once('=') {
                params.push((key, value));
            } else {
                // If not in key=value format, assume it's a value for the first required parameter
                if let Some(first_param) = endpoint.parameters.first() {
                    if first_param.required {
                        params.push((first_param.name, param_value.as_str()));
                        continue;
                    }
                }
                return Err(format!("Invalid parameter format: {}. Use key=value format", param_value).into());
            }
        }
    }
    
    // Check that all required parameters are provided
    for param_def in &endpoint.parameters {
        if param_def.required && !params.iter().any(|(k, _)| k == &param_def.name) {
            let param_examples = endpoint.parameters.iter()
                .filter(|p| p.required)
                .map(|p| format!("{}={}", p.name, p.example))
                .collect::<Vec<_>>()
                .join(" ");
            
            return Err(format!("Missing required parameter: {}\nUsage: inspect {} {} {}", 
                              param_def.name, data_type_str, endpoint_str, param_examples).into());
        }
    }
    
    println!("Inspecting {} endpoint", endpoint.name);
    
    // Call the inspect module
    if let Err(e) = inspect::inspect_endpoint(endpoint, &params) {
        eprintln!("Error: {}", e);
    }
    
    Ok(())
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

/// Example of how to use the CLI builder in main.rs
pub fn example_main() {
    let app = build_cli();
    let matches = app.get_matches();
    
    if let Err(e) = handle_command(&matches) {
        eprintln!("Error: {}", e);
    }
} 