// src/inspect.rs

use crate::api;
use crate::api_urls;
use serde_json::{Map, Value};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub fn inspect_keys(data_type: &str, endpoint: &str, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Get URL template from centralized location
    let url_template = api_urls::get_url_template(data_type, endpoint)
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
