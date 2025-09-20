use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use serde_json::Value;
use tokio::time::sleep;

use crate::api;
use crate::endpoints::{get_endpoint, Endpoint};
use crate::storage::{
    create_storage_backend, integrity::calculate_checksum, keys::generate_storage_key,
    StorageConfig,
};

use super::cache::{cache_exists, store_in_cache};
use super::params::ApiParams;

/// Fetch an endpoint (by registry name) and persist the payload to the local cache.
pub async fn fetch_endpoint(
    endpoint_name: &str,
    params: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    let endpoint = get_endpoint(endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;

    if !endpoint.implemented {
        return Err(format!("Endpoint '{}' is not implemented", endpoint_name).into());
    }

    let mut api_params = ApiParams::new();
    for (key, value) in params {
        api_params.add_param(key, value);
    }

    for param in &endpoint.parameters {
        if param.required && api_params.get_param(param.name).is_none() {
            return Err(format!(
                "Required parameter '{}' missing for endpoint '{}'",
                param.name, endpoint_name
            )
            .into());
        }
    }

    fetch_and_store(endpoint, &api_params).await
}

async fn fetch_and_store(endpoint: &Endpoint, params: &ApiParams) -> Result<(), Box<dyn Error>> {
    if cache_exists(endpoint.name, params) {
        println!("✅ Found {} data in cache", endpoint.name);
        return Ok(());
    }

    println!("🌐 Fetching {} data from NHL API...", endpoint.name);

    let url = interpolate_url(endpoint, params);

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

fn interpolate_url(endpoint: &Endpoint, params: &ApiParams) -> String {
    let mut url = endpoint.url.to_string();
    for (key, value) in params.as_map() {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    url
}

async fn fetch_and_store_with_retry(
    endpoint: &Endpoint,
    params: &ApiParams,
) -> Result<(), Box<dyn Error>> {
    let url = interpolate_url(endpoint, params);

    match api::fetch_api_json(&url).await {
        Ok(json_str) => {
            let data_json: Value = serde_json::from_str(&json_str)?;
            store_in_cache(endpoint.name, params, &data_json)?;

            let game_id = params.get_param("game_id").unwrap_or("unknown");
            println!(
                "💾 Saved {} data for game {} to cache",
                endpoint.name, game_id
            );

            Ok(())
        }
        Err(api::ApiError::NotFound) => Err("Resource not found (404)".into()),
        Err(api::ApiError::NetworkError(e)) => Err(Box::new(e)),
        Err(api::ApiError::Other(429)) => Err("Rate limited".into()),
        Err(api::ApiError::Other(code)) => Err(format!("HTTP error: {}", code).into()),
    }
}

/// Possible outcomes from processing an individual game request.
#[derive(Debug)]
pub enum ProcessResult {
    Success,
    Skipped,
}

/// Process a single game endpoint, honoring cached content and retry policy.
pub async fn process_game_endpoint(
    game_id: i64,
    endpoint_name: &str,
    max_retries: u32,
) -> Result<ProcessResult, Box<dyn Error>> {
    let endpoint = get_endpoint(endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;

    if !endpoint.implemented {
        return Err(format!("Endpoint '{}' is not implemented", endpoint_name).into());
    }

    let mut api_params = ApiParams::new();
    api_params.add_param("game_id", &game_id.to_string());

    if cache_exists(endpoint.name, &api_params) {
        return Ok(ProcessResult::Skipped);
    }

    for attempt in 1..=max_retries {
        match fetch_and_store_with_retry(endpoint, &api_params).await {
            Ok(()) => return Ok(ProcessResult::Success),
            Err(e) if attempt == max_retries => {
                return Err(format!("Failed after {} attempts: {}", max_retries, e).into())
            }
            Err(e) => {
                println!(
                    "⚠️ Attempt {}/{} failed for game {} ({}): {}",
                    attempt, max_retries, game_id, endpoint_name, e
                );
                let delay = Duration::from_secs(2_u64.pow(attempt - 1));
                sleep(delay).await;
            }
        }
    }

    unreachable!()
}

/// Fetch NHL data with the storage-first strategy, persisting to object storage when needed.
pub async fn fetch_and_store_enhanced(
    endpoint_name: &str,
    params: &[(&str, &str)],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let endpoint = get_endpoint(endpoint_name)
        .ok_or_else(|| format!("Endpoint '{}' not found", endpoint_name))?;

    if !endpoint.implemented {
        return Err(format!("Endpoint '{}' is not implemented", endpoint_name).into());
    }

    let mut param_map = HashMap::new();
    for (key, value) in params {
        param_map.insert((*key).to_string(), (*value).to_string());
    }

    for param in &endpoint.parameters {
        if param.required && !param_map.contains_key(param.name) {
            return Err(format!(
                "Required parameter '{}' missing for endpoint '{}'",
                param.name, endpoint_name
            )
            .into());
        }
    }

    let storage_key = generate_storage_key(endpoint_name, &param_map);

    let config = StorageConfig::from_env()?;
    let storage = create_storage_backend(&config).await?;

    if storage.exists(&storage_key).await? {
        println!(
            "✅ Found {} data in storage: {}",
            endpoint_name, storage_key
        );
        return Ok(());
    }

    println!("🌐 Fetching {} data from NHL API...", endpoint_name);

    let mut url = endpoint.url.to_string();
    for (key, value) in &param_map {
        url = url.replace(&format!("{{{}}}", key), value);
    }

    match api::fetch_api_json(&url).await {
        Ok(json_str) => {
            let checksum = calculate_checksum(&json_str);
            storage.put(&storage_key, &json_str).await?;

            println!(
                "💾 Saved {} data to storage: {}",
                endpoint_name, storage_key
            );
            println!("🔐 Checksum: {}", checksum);

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

/// Retrieve previously stored payload from object storage for inspection.
pub async fn get_from_storage(
    endpoint_name: &str,
    params: &[(&str, &str)],
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let mut param_map = HashMap::new();
    for (key, value) in params {
        param_map.insert((*key).to_string(), (*value).to_string());
    }

    let storage_key = generate_storage_key(endpoint_name, &param_map);

    let config = StorageConfig::from_env()?;
    let storage = create_storage_backend(&config).await?;

    let data = storage.get(&storage_key).await?;

    if let Some(ref content) = data {
        let checksum = calculate_checksum(content);
        println!(
            "📥 Retrieved {} data from storage: {}",
            endpoint_name, storage_key
        );
        println!("🔐 Checksum: {}", checksum);
    }

    Ok(data)
}
