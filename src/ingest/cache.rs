use std::path::PathBuf;

use crate::cache::write_to_cache;

use super::params::ApiParams;

/// Build a filesystem cache path for a given endpoint and parameter set.
pub fn get_cache_path(endpoint_name: &str, params: &ApiParams) -> PathBuf {
    let mut path = PathBuf::from("data/raw");

    // Organize cache by the endpoint prefix (games, players, etc.)
    if let Some((prefix, _)) = endpoint_name.split_once('_') {
        path.push(prefix);
    }

    // Compose a filename that captures relevant identifiers for uniqueness
    let mut filename = endpoint_name.to_string();
    for key in ["game_id", "player_id", "team_code", "date", "season"] {
        if let Some(value) = params.get_param(key) {
            filename.push('_');
            filename.push_str(value);
        }
    }

    filename.push_str(".json");
    path.push(filename);
    path
}

/// Determine whether cached data already exists for an endpoint + params.
pub fn cache_exists(endpoint_name: &str, params: &ApiParams) -> bool {
    get_cache_path(endpoint_name, params).exists()
}

/// Persist API data to the filesystem cache.
pub fn store_in_cache(
    endpoint_name: &str,
    params: &ApiParams,
    data: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_path = get_cache_path(endpoint_name, params);
    let content = serde_json::to_string_pretty(data)?;
    write_to_cache(&cache_path, &content)?;
    Ok(())
}
