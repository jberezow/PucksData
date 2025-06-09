use crate::api;
use crate::endpoints::{Endpoint, get_endpoint};
use std::collections::HashMap;
use crate::db::{DbPool, insert_raw_data, raw_data_exists};
use serde_json::{json, Value};

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

    pub fn to_json(&self) -> serde_json::Value {
        json!(self.params)
    }
}

/// Generic function to fetch an endpoint by name with provided parameters
pub async fn fetch_endpoint(endpoint_name: &str, params: &[(&str, &str)], pool: DbPool) -> Result<(), Box<dyn std::error::Error>> {
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
    
    fetch_and_store(endpoint, &api_params, pool).await
}

/// Internal function to fetch and cache data for an endpoint
async fn fetch_and_store(endpoint: &Endpoint, params: &ApiParams, pool: DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let params_json = params.to_json();
    if raw_data_exists(&pool, endpoint.name, &params_json).await? {
        println!("✅ Found {} data in database", endpoint.name);
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
            insert_raw_data(&pool, endpoint.name, &params_json, &data_json).await?;
            println!("💾 Saved {} data to database", endpoint.name);
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
