use std::time::{Duration, Instant};
use tokio::time::sleep;
use serde_json::Value;
use indicatif::{ProgressBar, ProgressStyle};

use crate::ingest::{ApiParams, fetch_and_store_with_retry};
use crate::endpoints::get_endpoint;
use std::path::PathBuf;

#[derive(Debug)]
enum ProcessResult {
    Success,
    Skipped,
}

/// Generate cache file path for an endpoint and parameters
fn get_cache_path(endpoint_name: &str, game_id: i64) -> PathBuf {
    let mut path = PathBuf::from("data/raw");
    
    // Create directory structure based on endpoint type
    let parts: Vec<&str> = endpoint_name.split('_').collect();
    if parts.len() > 1 {
        path.push(parts[0]); // e.g., "games", "players", "teams"
    }
    
    // Create filename based on endpoint and game_id
    let filename = format!("{}_{}.json", endpoint_name, game_id);
    path.push(filename);
    path
}

/// Check if cached data exists for a game endpoint
fn cache_exists_for_game(endpoint_name: &str, game_id: i64) -> bool {
    let cache_path = get_cache_path(endpoint_name, game_id);
    cache_path.exists()
}

pub async fn bulk_import_all_games(
    games_file_path: &str, 
    endpoints: &[&str], // Which endpoints to fetch for each game
    max_retries: u32,
    batch_size: usize,
    start_year: Option<i32>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting bulk import of NHL games...");
    
    // Load games data
    let games_json = std::fs::read_to_string(games_file_path)?;
    let games_data: Value = serde_json::from_str(&games_json)?;
    
    let all_games = games_data["data"].as_array()
        .ok_or("Invalid games data format")?;
    
    // Filter games by start year if specified
    let filtered_games: Vec<&Value> = if let Some(year) = start_year {
        all_games.iter()
            .filter(|game| {
                if let Some(game_id) = game["id"].as_i64() {
                    // Extract year from game ID (format: YYYYTTGGGG)
                    let game_year = game_id / 1000000;
                    game_year >= year as i64
                } else {
                    false
                }
            })
            .collect()
    } else {
        all_games.iter().collect()
    };
    
    println!("📊 Found {} total games, {} after filtering", all_games.len(), filtered_games.len());
    println!("🎯 Endpoints to fetch: {:?}", endpoints);
    println!("📦 Batch size: {}", batch_size);
    
    let mut success_count = 0;
    let mut error_count = 0;
    let mut skip_count = 0;
    let start_time = Instant::now();
    
    // Create overall progress bar
    let total_games = filtered_games.len();
    let overall_pb = ProgressBar::new(total_games as u64);
    overall_pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} games ({per_sec}, ETA: {eta})")
        .unwrap()
        .progress_chars("#>-"));
    
    for (batch_idx, game_batch) in filtered_games.chunks(batch_size).enumerate() {
        println!("\n📦 Processing batch {} ({} games)...", 
                batch_idx + 1, game_batch.len());
        
        // Create batch progress bar
        let batch_pb = ProgressBar::new(game_batch.len() as u64);
        batch_pb.set_style(ProgressStyle::default_bar()
            .template("  {spinner:.yellow} [{bar:30.green/blue}] {pos}/{len} batch games")
            .unwrap()
            .progress_chars("=>-"));
        
        for game in game_batch.iter() {
            let game_id = game["id"].as_i64()
                .ok_or("Game missing ID field")?;
            
            // Fetch raw data for each endpoint
            for endpoint_name in endpoints {
                match process_game_endpoint(game_id, endpoint_name, max_retries).await {
                    Ok(ProcessResult::Success) => success_count += 1,
                    Ok(ProcessResult::Skipped) => skip_count += 1,
                    Err(e) => {
                        error_count += 1;
                        eprintln!("❌ Failed to process game {} with endpoint {}: {}", 
                                game_id, endpoint_name, e);
                    }
                }
            }
            
            batch_pb.inc(1);
            overall_pb.inc(1);
            
            // Small delay between games to be respectful
            sleep(Duration::from_millis(50)).await;
        }
        
        batch_pb.finish_and_clear();
        
        // Longer pause between batches
        println!("⏸️  Batch complete, pausing 2 seconds...");
        sleep(Duration::from_secs(2)).await;
    }
    
    overall_pb.finish_and_clear();
    
    let total_time = start_time.elapsed();
    println!("\n🎉 Bulk import completed!");
    println!("📈 Final stats:");
    println!("   ✅ Raw data operations: {}", success_count);
    println!("   ⏭️  Skipped: {}", skip_count);
    println!("   ❌ Errors: {}", error_count);
    println!("   ⏱️  Total time: {:.2} minutes", total_time.as_secs_f64() / 60.0);
    println!("   📊 Games processed: {}", total_games);
    println!("   📊 Average rate: {:.1} requests/sec", 
            (success_count + skip_count + error_count) as f64 / total_time.as_secs_f64());
    
    Ok(())
}

async fn process_game_endpoint(
    game_id: i64,
    endpoint_name: &str,
    max_retries: u32,
) -> Result<ProcessResult, Box<dyn std::error::Error>> {
    let endpoint = get_endpoint(endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;
    
    if !endpoint.implemented {
        return Err(format!("Endpoint '{}' is not implemented", endpoint_name).into());
    }
    
    let mut api_params = ApiParams::new();
    api_params.add_param("game_id", &game_id.to_string());
    
    // Check if we already have this data cached
    if cache_exists_for_game(endpoint.name, game_id) {
        // Occasionally log skips to show we're checking the cache
        if game_id % 1000 == 0 {
            println!("⏭️  Skipping game {} ({}) - already in cache", game_id, endpoint_name);
        }
        return Ok(ProcessResult::Skipped);
    }
    
    // Try to fetch with retries
    for attempt in 1..=max_retries {
        match fetch_and_store_with_retry(endpoint, &api_params).await {
            Ok(()) => return Ok(ProcessResult::Success),
            Err(e) if attempt == max_retries => {
                return Err(format!("Failed after {} attempts: {}", max_retries, e).into());
            }
            Err(e) => {
                println!("⚠️ Attempt {}/{} failed for game {} ({}): {}", 
                        attempt, max_retries, game_id, endpoint_name, e);
                // Exponential backoff
                let delay = Duration::from_secs(2_u64.pow(attempt - 1));
                sleep(delay).await;
            }
        }
    }
    
    unreachable!()
}