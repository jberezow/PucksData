use crate::endpoints::{get_endpoints_by_type, DataType, Endpoint, Parameter};
use crate::ingest;
use crate::AnyError;
use clap::{Arg, ArgMatches, Command};

const DATA_TYPES: [DataType; 3] = [DataType::Games, DataType::Players, DataType::Teams];

/// Build the top-level CLI application
pub fn build_cli() -> Command {
    let mut app = Command::new("pucksdata")
        .about("NHL Stats Engine CLI - A tool for fetching and caching NHL data")
        .version(env!("CARGO_PKG_VERSION"));

    for data_type in DATA_TYPES {
        if let Some(command) = build_data_type_command(data_type) {
            app = app.subcommand(command);
        }
    }

    // Add list-endpoints command to help discovery
    app = app.subcommand(
        Command::new("list-endpoints")
            .about("List available API endpoints")
            .arg(
                Arg::new("data_type")
                    .help("Filter by data type (games, players, etc.)")
                    .required(false),
            ),
    );

    // Add test-storage command
    app = app.subcommand(Command::new("test-storage").about("Test object storage connection"));

    // Add fetch-to-storage command
    app = app.subcommand(
        Command::new("fetch-to-storage")
            .about("Fetch NHL data and store in object storage")
            .arg(
                Arg::new("endpoint")
                    .help("Endpoint name (e.g., game_boxscore)")
                    .required(true),
            )
            .arg(
                Arg::new("params")
                    .help("Parameters (key=value pairs)")
                    .action(clap::ArgAction::Append),
            ),
    );

    // Add list-storage command
    app = app.subcommand(
        Command::new("list-storage")
            .about("List files in object storage")
            .arg(Arg::new("prefix").help("Filter by prefix (optional)")),
    );

    // Add inspect-schema command
    app = app.subcommand(
        Command::new("inspect-schema").about("Inspect database schema and save to file"),
    );

    app
}

fn build_data_type_command(data_type: DataType) -> Option<Command> {
    let endpoints: Vec<&Endpoint> = get_endpoints_by_type(data_type)
        .into_iter()
        .filter(|endpoint| endpoint.implemented)
        .collect();

    if endpoints.is_empty() {
        return None;
    }

    let mut command = Command::new(data_type.as_str()).about(data_type_about(data_type));

    for endpoint in endpoints {
        command = command.subcommand(build_endpoint_subcommand(endpoint));
    }

    Some(command)
}

fn build_endpoint_subcommand(endpoint: &Endpoint) -> Command {
    let mut subcommand = Command::new(endpoint.cli_subcommand_name()).about(endpoint.description);

    for (index, parameter) in endpoint.parameters.iter().enumerate() {
        subcommand = subcommand.arg(build_arg_from_parameter(parameter, index + 1));
    }

    subcommand
}

fn build_arg_from_parameter(parameter: &Parameter, index: usize) -> Arg {
    Arg::new(parameter.name)
        .help(parameter.description)
        .required(parameter.required)
        .index(index)
}

fn data_type_about(data_type: DataType) -> &'static str {
    match data_type {
        DataType::Games => "Games-related data operations",
        DataType::Players => "Players-related data operations",
        DataType::Teams => "Teams-related data operations",
    }
}

fn find_endpoint_by_cli_name(data_type: DataType, cli_name: &str) -> Option<&'static Endpoint> {
    get_endpoints_by_type(data_type)
        .into_iter()
        .filter(|endpoint| endpoint.implemented)
        .find(|endpoint| endpoint.cli_subcommand_name() == cli_name)
}

fn parse_data_type_filter(value: &str) -> Option<DataType> {
    match value {
        "games" => Some(DataType::Games),
        "players" => Some(DataType::Players),
        "teams" => Some(DataType::Teams),
        _ => None,
    }
}

/// Main command handler
pub async fn handle_command(matches: &ArgMatches) -> Result<(), AnyError> {
    match matches.subcommand() {
        Some(("games", sub_matches)) => {
            handle_data_type_command(DataType::Games, sub_matches).await
        }
        Some(("players", sub_matches)) => {
            handle_data_type_command(DataType::Players, sub_matches).await
        }
        Some(("teams", sub_matches)) => {
            handle_data_type_command(DataType::Teams, sub_matches).await
        }

        Some(("list-endpoints", sub_matches)) => handle_list_endpoints_command(sub_matches),
        Some(("test-storage", _)) => handle_test_storage_command().await,
        Some(("fetch-to-storage", sub_matches)) => {
            handle_fetch_to_storage_command(sub_matches).await
        }
        Some(("list-storage", sub_matches)) => handle_list_storage_command(sub_matches).await,
        Some(("inspect-schema", _)) => handle_inspect_schema_command().await,
        _ => {
            // No subcommand provided, so we'll print help
            build_cli().print_help()?;
            Ok(())
        }
    }
}

/// Generic handler for data type commands (games, players, etc.)
async fn handle_data_type_command(
    data_type: DataType,
    matches: &ArgMatches,
) -> Result<(), AnyError> {
    let (subcommand_name, sub_matches) = matches
        .subcommand()
        .ok_or_else(|| format!("No {} subcommand provided", data_type.as_str()))?;

    let endpoint = find_endpoint_by_cli_name(data_type, subcommand_name).ok_or_else(|| {
        format!(
            "Unknown command '{}' for {}",
            subcommand_name,
            data_type.as_str()
        )
    })?;

    let mut params = Vec::new();
    for parameter in &endpoint.parameters {
        if let Some(value) = sub_matches.get_one::<String>(parameter.name) {
            params.push((parameter.name, value.as_str()));
        }
    }

    ingest::fetch_endpoint(endpoint.name, &params).await
}

/// Handle the test-storage command
async fn handle_test_storage_command() -> Result<(), AnyError> {
    match crate::storage::test::test_r2_connection().await {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("Storage test failed: {}", e).into()),
    }
}

/// Handle the fetch-to-storage command
async fn handle_fetch_to_storage_command(matches: &ArgMatches) -> Result<(), AnyError> {
    let endpoint_name = matches
        .get_one::<String>("endpoint")
        .ok_or("endpoint is required")?;

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

    match crate::ingest::fetch_and_store_enhanced(endpoint_name, &params).await {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("Fetch to storage failed: {}", e).into()),
    }
}

/// Handle the list-storage command
async fn handle_list_storage_command(matches: &ArgMatches) -> Result<(), AnyError> {
    let prefix = matches.get_one::<String>("prefix").map(|s| s.as_str());

    match crate::storage::list::list_storage_files(prefix).await {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("List storage failed: {}", e).into()),
    }
}

/// Handle the inspect-schema command
async fn handle_inspect_schema_command() -> Result<(), AnyError> {
    match crate::storage::schema_inspector::inspect_and_save_schema().await {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("Schema inspection failed: {}", e).into()),
    }
}

/// Handle the list-endpoints command
fn handle_list_endpoints_command(matches: &ArgMatches) -> Result<(), AnyError> {
    let filter = matches.get_one::<String>("data_type");
    let filter_data_type = match filter {
        Some(value) => match parse_data_type_filter(value) {
            Some(data_type) => Some(data_type),
            None => {
                println!(
                    "Unknown data type '{}'. Available values: games, players, teams.",
                    value
                );
                return Ok(());
            }
        },
        None => None,
    };

    println!("Available API Endpoints:");
    println!("=======================");

    let mut any_printed = false;

    for data_type in DATA_TYPES {
        if let Some(expected) = filter_data_type {
            if data_type != expected {
                continue;
            }
        }

        let endpoints: Vec<&Endpoint> = get_endpoints_by_type(data_type)
            .into_iter()
            .filter(|endpoint| endpoint.implemented)
            .collect();

        if endpoints.is_empty() {
            continue;
        }

        any_printed = true;

        println!("\n{} ({}):", data_type.as_str(), endpoints.len());
        println!("{}", "-".repeat(data_type.as_str().len() + 4));

        for endpoint in endpoints {
            let cli_name = endpoint.cli_subcommand_name();
            println!(
                "  {} {} - {}",
                data_type.as_str(),
                cli_name,
                endpoint.description
            );

            let required_params: Vec<_> = endpoint
                .parameters
                .iter()
                .filter(|parameter| parameter.required)
                .collect();

            if !required_params.is_empty() {
                println!("    Required parameters:");
                for param in required_params {
                    println!(
                        "      {} - {} (example: {})",
                        param.name, param.description, param.example
                    );
                }
            }

            println!("    Example: {}", endpoint.example);
            println!();
        }
    }

    if !any_printed {
        println!("No endpoints found for provided filter.");
    }

    Ok(())
}

/// Main entry point for CLI
pub async fn example_main() {
    let app = build_cli();
    let matches = app.get_matches();

    if let Err(e) = handle_command(&matches).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
