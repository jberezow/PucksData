// src/inspect.rs

use crate::api;
use crate::endpoints::Endpoint;
use serde_json::{Map, Value};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Inspect an endpoint using the endpoint registry
pub fn inspect_endpoint(endpoint: &Endpoint, params: &[(&str, &str)]) -> Result<(), Box<dyn std::error::Error>> {
    // Fill in the URL template with the provided parameters
    let mut url = endpoint.url.to_string();
    
    // Replace placeholders in the URL with actual values
    for (name, value) in params {
        url = url.replace(&format!("{{{}}}", name), value);
    }
    
    // Check if URL still contains any {} parameters
    if url.contains('{') && url.contains('}') {
        return Err(format!("URL still contains unresolved parameters: {}", url).into());
    }
    
    println!("Fetching data from: {}", url);
    let json_str = api::fetch_api_json(&url)?;
    let json_val: Value = serde_json::from_str(&json_str)?;

    let key_tree = build_key_tree(&json_val);

    // Store keys in data/inspect/<data_type>/<endpoint>_keys.json
    let mut file_path = PathBuf::from("data/inspect");
    file_path.push(endpoint.data_type.as_str());
    std::fs::create_dir_all(&file_path)?;
    file_path.push(format!("{}_keys.json", endpoint.name));

    let mut file = File::create(file_path)?;
    let pretty = serde_json::to_string_pretty(&key_tree)?;
    file.write_all(pretty.as_bytes())?;

    println!("🔍 Inspection complete. Keys written to inspect/{}/{}_keys.json", 
             endpoint.data_type.as_str(), endpoint.name);

    Ok(())
}

/// Legacy method for backward compatibility
pub fn inspect_keys(data_type: &str, endpoint: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Try to find the endpoint in the registry
    let endpoint_name = format!("{}_{}", data_type, endpoint.replace('-', "_"));
    if let Some(endpoint_def) = crate::endpoints::get_endpoint(&endpoint_name) {
        // Map the id to the correct parameter
        let mut params = Vec::new();
        if let Some(first_param) = endpoint_def.parameters.first() {
            params.push((first_param.name, id));
        }
        inspect_endpoint(endpoint_def, &params)
    } else {
        // Fall back to the old method using api_urls
        legacy_inspect_keys(data_type, endpoint, id)
    }
}

/// The original inspect_keys implementation using api_urls
fn legacy_inspect_keys(data_type: &str, endpoint: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Get URL template from centralized location
    let url_template = crate::api_urls::get_url_template(data_type, endpoint)
        .ok_or_else(|| "Unsupported combination of data_type and endpoint")?;

    let url = url_template.replace("{id}", id);
    let json_str = api::fetch_api_json(&url)?;
    let json_val: Value = serde_json::from_str(&json_str)?;

    let key_tree = build_key_tree(&json_val);

    // Store keys in data/inspect/<data_type>/<endpoint>_keys.json
    let mut file_path = PathBuf::from("data/inspect");
    file_path.push(data_type);
    std::fs::create_dir_all(&file_path)?;
    file_path.push(format!("{}_keys.json", endpoint));

    let mut file = File::create(file_path)?;
    let pretty = serde_json::to_string_pretty(&key_tree)?;
    file.write_all(pretty.as_bytes())?;

    println!("🔍 Inspection complete. Keys written to inspect/{}/{}_keys.json", data_type, endpoint);

    Ok(())
}

fn build_key_tree(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut tree = Map::new();
            for (k, v) in map {
                tree.insert(k.clone(), build_key_tree(v));
            }
            Value::Object(tree)
        }
        Value::Array(arr) => {
            if let Some(first) = arr.get(0) {
                build_key_tree(first)
            } else {
                Value::Bool(true) // empty array
            }
        }
        _ => Value::Bool(true),
    }
}
