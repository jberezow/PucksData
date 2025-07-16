// src/main.rs - Implementation using the endpoint registry

use pucksdata::cli_builder;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::current_dir()?;
    println!("Current working directory is: {}", path.display());
    
    cli_builder::example_main().await;
    Ok(())
} 