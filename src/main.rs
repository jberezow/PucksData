// src/main.rs - Implementation using the endpoint registry

use pucksdata::cli_builder;
use pucksdata::db;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::current_dir()?;
    println!("Current working directory is: {}", path.display());
    
    match dotenv::dotenv() {
        Ok(path) => println!("Loaded .env file from: {}", path.display()),
        Err(e) => println!("Could not load .env file: {e}"),
    }

    let pool = db::create_pool().await?;
    cli_builder::example_main(pool).await;
    Ok(())
} 