use crate::api;
use crate::cache;
use crate::endpoints::{DataType, Endpoint, get_endpoint};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct ApiParams {
    params: HashMap<String, String>,
}

impl ApiParams {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    pub fn add_param(&mut self, key: &str, value: &str) -> &mut Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }
}

/// Generic function to fetch an endpoint by name with provided parameters
pub fn fetch_endpoint(endpoint_name: &str, params: &[(&str, &str)]) -> Result<(), Box<dyn std::error::Error>> {
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
    
    fetch_and_cache(endpoint, &api_params)
}

/// Internal function to fetch and cache data for an endpoint
fn fetch_and_cache(endpoint: &Endpoint, params: &ApiParams) -> Result<(), Box<dyn std::error::Error>> {
    let mut file_path = PathBuf::from("data/raw");
    file_path.push(endpoint.data_type.as_str());
    
    // Create parameter-specific subdirectories based on data type and endpoint
    build_path_structure(endpoint, params, &mut file_path)?;
    
    // Create the directory structure
    std::fs::create_dir_all(&file_path)?;
    file_path.push(format!("{}.json", endpoint.name));
    
    // Check cache first
    if let Some(_) = cache::read_from_cache(&file_path) {
        println!("✅ Found cached {} data at {:?}", endpoint.name, file_path);
        return Ok(());
    }
    
    println!("🌐 Fetching {} data from NHL API...", endpoint.name);
    
    // Replace URL parameters with actual values
    let mut url = endpoint.url.to_string();
    for (key, value) in &params.params {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    
    match api::fetch_api_json(&url) {
        Ok(json) => {
            cache::write_to_cache(&file_path, &json)?;
            println!("💾 Saved {} data to {:?}", endpoint.name, file_path);
            Ok(())
        }
        Err(api::ApiError::NotFound) => {
            // Clean up empty directories
            cleanup_empty_directories(&file_path);
            println!("❌ Resource not found at {}", url);
            Err("Resource not found (404)".into())
        }
        Err(api::ApiError::NetworkError(e)) => {
            cleanup_empty_directories(&file_path);
            println!("❌ Network error while fetching {}: {}", url, e);
            Err(Box::new(e))
        }
        Err(api::ApiError::Other(code)) => {
            cleanup_empty_directories(&file_path);
            println!("❌ HTTP error {} while fetching {}", code, url);
            Err(format!("HTTP error: {}", code).into())
        }
    }
}

/// Helper function to build the path structure based on endpoint type and params
fn build_path_structure(endpoint: &Endpoint, params: &ApiParams, file_path: &mut PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    match endpoint.data_type {
        DataType::Games => {
            // For game endpoints with IDs
            if let Some(id) = params.get_param("game_id") {
                file_path.push(id);
            } else if let Some(date) = params.get_param("date") {
                file_path.push(date);
            }
        },
        DataType::Players => {
            if let Some(id) = params.get_param("player_id") {
                file_path.push(id);
                
                // For game logs, add season and game type
                if endpoint.name == "player_game_log" {
                    if let Some(season) = params.get_param("season") {
                        file_path.push(season);
                        if let Some(game_type) = params.get_param("game_type") {
                            file_path.push(game_type);
                        }
                    }
                }
            }
        },
        DataType::Teams => {
            // For team endpoints, always process team_code first
            if let Some(team_code) = params.get_param("team_code") {
                file_path.push(team_code);
                
                // Then add season or date as a subdirectory if available
                if endpoint.name == "team_roster_season" || endpoint.name == "team_schedule_season" {
                    if let Some(season) = params.get_param("season") {
                        file_path.push(season);
                    }
                } else if endpoint.name == "team_schedule_month" {
                    if let Some(date) = params.get_param("date") {
                        file_path.push(date);
                    }
                } else if endpoint.name == "team_stats_by_season" {
                    if let Some(season) = params.get_param("season") {
                        file_path.push(season);
                        if let Some(game_type) = params.get_param("game_type") {
                            file_path.push(game_type);
                        }
                    }
                }
            } else if endpoint.name == "team_standings_date" {
                if let Some(date) = params.get_param("date") {
                    file_path.push(date);
                }
            } else if endpoint.name == "team_standings_season" {
                if let Some(season) = params.get_param("season") {
                    file_path.push(season);
                }
            }
        },
        DataType::Skaters | DataType::Goalies => {
            if let Some(season) = params.get_param("season") {
                file_path.push(season);
                if let Some(game_type) = params.get_param("game_type") {
                    file_path.push(game_type);
                }
            }
        },
        DataType::Schedule => {
            if let Some(date) = params.get_param("date") {
                file_path.push(date);
            }
        },
        DataType::Playoffs => {
            if let Some(year) = params.get_param("year") {
                file_path.push(year);
                if endpoint.name == "playoff_series_metadata" {
                    if let Some(letter) = params.get_param("letter") {
                        file_path.push(letter);
                    }
                }
            } else if let Some(season) = params.get_param("season") {
                file_path.push(season);
                if endpoint.name == "playoff_series_schedule" {
                    if let Some(letter) = params.get_param("letter") {
                        file_path.push(letter);
                    }
                }
            }
        },
        DataType::Draft => {
            if let Some(year) = params.get_param("year") {
                file_path.push(year);
            }
        },
        // Other data types don't need special handling
        _ => {}
    }
    
    Ok(())
}

/// Helper to clean up empty directories after failed API calls
fn cleanup_empty_directories(file_path: &PathBuf) {
    if let Some(parent) = file_path.parent() {
        if parent.exists() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                if entries.count() == 0 {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }
    }
}
