use crate::api;
use crate::endpoints::{Endpoint, get_endpoint};
use crate::cache::{write_to_cache};
use std::collections::HashMap;
use std::path::PathBuf;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use indicatif::{ProgressBar, ProgressStyle};

pub struct ApiParams {
    params: HashMap<String, String>,
}

impl ApiParams {
    pub fn new() -> Self {
        Self { params: HashMap::new() }
    }

    pub fn add_param(&mut self, key: &str, value: &str) -> &mut Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!(self.params)
    }
}

/// Generate cache file path for an endpoint and parameters
fn get_cache_path(endpoint_name: &str, params: &ApiParams) -> PathBuf {
    let mut path = PathBuf::from("data/raw");
    
    // Create directory structure based on endpoint type
    let parts: Vec<&str> = endpoint_name.split('_').collect();
    if parts.len() > 1 {
        path.push(parts[0]); // e.g., "games", "players", "teams"
    }
    
    // Create filename based on endpoint and parameters
    let mut filename = endpoint_name.to_string();
    
    // Add key parameters to filename for uniqueness
    if let Some(game_id) = params.get_param("game_id") {
        filename.push_str(&format!("_{}", game_id));
    }
    if let Some(player_id) = params.get_param("player_id") {
        filename.push_str(&format!("_{}", player_id));
    }
    if let Some(team_code) = params.get_param("team_code") {
        filename.push_str(&format!("_{}", team_code));
    }
    if let Some(date) = params.get_param("date") {
        filename.push_str(&format!("_{}", date));
    }
    if let Some(season) = params.get_param("season") {
        filename.push_str(&format!("_{}", season));
    }
    
    filename.push_str(".json");
    path.push(filename);
    path
}

/// Check if cached data exists for an endpoint
fn cache_exists(endpoint_name: &str, params: &ApiParams) -> bool {
    let cache_path = get_cache_path(endpoint_name, params);
    cache_path.exists()
}



/// Store data in cache
fn store_in_cache(endpoint_name: &str, params: &ApiParams, data: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let cache_path = get_cache_path(endpoint_name, params);
    let content = serde_json::to_string_pretty(data)?;
    write_to_cache(&cache_path, &content)?;
    Ok(())
}

/// Generic function to fetch an endpoint by name with provided parameters
pub async fn fetch_endpoint(endpoint_name: &str, params: &[(&str, &str)]) -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = get_endpoint(endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;
    
    if !endpoint.implemented {
        return Err(format!("Endpoint '{}' is not implemented", endpoint_name).into());
    }
    
    let mut api_params = ApiParams::new();
    
    // Add all parameters
    for (key, value) in params {
        api_params.add_param(key, value);
    }
    
    // Validate required parameters
    for param in &endpoint.parameters {
        if param.required && api_params.get_param(param.name).is_none() {
            return Err(format!("Required parameter '{}' missing for endpoint '{}'", 
                              param.name, endpoint_name).into());
        }
    }
    
    fetch_and_store(endpoint, &api_params).await
}

/// Internal function to fetch and cache data for an endpoint
async fn fetch_and_store(endpoint: &Endpoint, params: &ApiParams) -> Result<(), Box<dyn std::error::Error>> {
    // Check if we already have cached data
    if cache_exists(endpoint.name, params) {
        println!("✅ Found {} data in cache", endpoint.name);
        return Ok(());
    }

    println!("🌐 Fetching {} data from NHL API...", endpoint.name);
    
    // Replace URL parameters with actual values
    let mut url = endpoint.url.to_string();
    for (key, value) in &params.params {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    
    match api::fetch_api_json(&url).await {
        Ok(json_str) => {
            let data_json: Value = serde_json::from_str(&json_str)?;
            store_in_cache(endpoint.name, params, &data_json)?;
            println!("💾 Saved {} data to cache", endpoint.name);
            Ok(())
        }
        Err(api::ApiError::NotFound) => {
            println!("❌ Resource not found at {}", url);
            Err("Resource not found (404)".into())
        }
        Err(api::ApiError::NetworkError(e)) => {
            println!("❌ Network error while fetching {}: {}", url, e);
            Err(Box::new(e))
        }
        Err(api::ApiError::Other(code)) => {
            println!("❌ HTTP error {} while fetching {}", code, url);
            Err(format!("HTTP error: {}", code).into())
        }
    }
}

/// Fetch and store data from an API endpoint with retry logic
pub async fn fetch_and_store_with_retry(
    endpoint: &Endpoint, 
    params: &ApiParams
) -> Result<(), Box<dyn std::error::Error>> {
    // Replace URL parameters with actual values
    let mut url = endpoint.url.to_string();
    for (key, value) in &params.params {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    
    match api::fetch_api_json(&url).await {
        Ok(json_str) => {
            let data_json: Value = serde_json::from_str(&json_str)?;
            store_in_cache(endpoint.name, params, &data_json)?;
            
            // Extract game_id for better logging
            let game_id = params.get_param("game_id").unwrap_or("unknown");
            println!("💾 Saved {} data for game {} to cache", endpoint.name, game_id);
            
            Ok(())
        }
        Err(api::ApiError::NotFound) => {
            Err("Resource not found (404)".into())
        }
        Err(api::ApiError::NetworkError(e)) => {
            Err(Box::new(e))
        }
        Err(api::ApiError::Other(429)) => {
            Err("Rate limited".into())
        }
        Err(api::ApiError::Other(code)) => {
            Err(format!("HTTP error: {}", code).into())
        }
    }
}

#[derive(Debug)]
enum ProcessResult {
    Success,
    Skipped,
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
    if cache_exists(endpoint.name, &api_params) {
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

/// Bulk import all games with rate limiting and error recovery
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