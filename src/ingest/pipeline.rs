use std::collections::HashMap;

use serde_json::Value;

use crate::api;
use crate::endpoints::{get_endpoint, Endpoint};
use crate::storage::{
    create_storage_backend, integrity::calculate_checksum, keys::generate_storage_key,
    StorageConfig,
};
use crate::AnyError;

use super::cache::{cache_exists, store_in_cache};
use super::metadata::{
    find_payload_record, get_db_pool, resolve_entity_context, upsert_payload_record,
};
use super::params::ApiParams;

enum PayloadSource {
    Database,
    Storage,
    Api,
}

struct PayloadResult {
    content: String,
    storage_key: String,
    source: PayloadSource,
}

/// Fetch an endpoint (by registry name) and persist the payload to the local cache.
pub async fn fetch_endpoint(endpoint_name: &str, params: &[(&str, &str)]) -> Result<(), AnyError> {
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

    if cache_exists(endpoint.name, &api_params) {
        println!("✅ Found {} data in cache", endpoint.name);
        return Ok(());
    }

    let payload = load_or_fetch_payload(endpoint, api_params.as_map()).await?;
    persist_local_cache(endpoint, &api_params, &payload.content)?;
    log_payload_source(endpoint.name, &payload);

    Ok(())
}

/// Internal helper to store JSON content in the filesystem cache.
fn persist_local_cache(
    endpoint: &Endpoint,
    params: &ApiParams,
    payload: &str,
) -> Result<(), AnyError> {
    let data_json: Value = serde_json::from_str(payload)?;
    store_in_cache(endpoint.name, params, &data_json)?;
    Ok(())
}

fn interpolate_url(endpoint: &Endpoint, params: &HashMap<String, String>) -> String {
    let mut url = endpoint.url.to_string();
    for (key, value) in params {
        url = url.replace(&format!("{{{}}}", key), value);
    }
    url
}

async fn load_or_fetch_payload(
    endpoint: &Endpoint,
    params: &HashMap<String, String>,
) -> Result<PayloadResult, AnyError> {
    let storage_key = generate_storage_key(endpoint.name, params);
    let config = StorageConfig::from_env()?;
    let storage = create_storage_backend(&config).await?;
    let pool = get_db_pool().await?;
    let entity_context = resolve_entity_context(endpoint, params, pool).await?;

    if let Some(record) = find_payload_record(
        pool,
        &entity_context.entity_type,
        endpoint.name,
        entity_context.nhl_id,
    )
    .await?
    {
        if let Some(cached) = storage.get(&record.storage_key).await? {
            let computed_checksum = calculate_checksum(&cached);
            if computed_checksum != record.checksum
                || cached.as_bytes().len() as i64 != record.file_size
            {
                println!(
                    "⚠️ Stored metadata for '{}' is stale (checksum/size mismatch); refreshing",
                    endpoint.name
                );
                upsert_payload_record(
                    pool,
                    &entity_context,
                    endpoint.name,
                    &record.storage_key,
                    &computed_checksum,
                    cached.as_bytes().len() as i64,
                )
                .await?;
            }
            return Ok(PayloadResult {
                content: cached,
                storage_key: record.storage_key,
                source: PayloadSource::Database,
            });
        } else {
            println!(
                "⚠️ Metadata found for '{}' but object '{}' is missing (expected size {}); refetching",
                endpoint.name, record.storage_key, record.file_size
            );
        }
    }

    if let Some(existing) = storage.get(&storage_key).await? {
        let checksum = calculate_checksum(&existing);
        let file_size = existing.as_bytes().len() as i64;
        upsert_payload_record(
            pool,
            &entity_context,
            endpoint.name,
            &storage_key,
            &checksum,
            file_size,
        )
        .await?;

        return Ok(PayloadResult {
            content: existing,
            storage_key,
            source: PayloadSource::Storage,
        });
    }

    let url = interpolate_url(endpoint, params);
    println!("🌐 Fetching {} data from NHL API...", endpoint.name);

    match api::fetch_api_json(&url).await {
        Ok(json_str) => {
            let checksum = calculate_checksum(&json_str);
            let file_size = json_str.as_bytes().len() as i64;
            storage.put(&storage_key, &json_str).await?;
            upsert_payload_record(
                pool,
                &entity_context,
                endpoint.name,
                &storage_key,
                &checksum,
                file_size,
            )
            .await?;

            Ok(PayloadResult {
                content: json_str,
                storage_key,
                source: PayloadSource::Api,
            })
        }
        Err(api::ApiError::NotFound) => Err("Resource not found (404)".into()),
        Err(api::ApiError::NetworkError(e)) => Err(Box::new(e)),
        Err(api::ApiError::Other(code)) => Err(format!("HTTP error: {}", code).into()),
    }
}

fn log_payload_source(endpoint_name: &str, payload: &PayloadResult) {
    let checksum = calculate_checksum(&payload.content);
    match payload.source {
        PayloadSource::Database => println!(
            "✅ Using cached {} data via DB metadata: {}",
            endpoint_name, payload.storage_key
        ),
        PayloadSource::Storage => println!(
            "✅ Loaded {} data from object storage: {}",
            endpoint_name, payload.storage_key
        ),
        PayloadSource::Api => println!(
            "💾 Saved {} data to storage: {}",
            endpoint_name, payload.storage_key
        ),
    }
    println!("🔐 Checksum: {}", checksum);
}

/// Fetch NHL data with the storage-first strategy, persisting to object storage when needed.
pub async fn fetch_and_store_enhanced(
    endpoint_name: &str,
    params: &[(&str, &str)],
) -> Result<(), AnyError> {
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

    let payload = load_or_fetch_payload(endpoint, &param_map).await?;
    log_payload_source(endpoint.name, &payload);

    Ok(())
}

/// Retrieve previously stored payload from object storage for inspection.
pub async fn get_from_storage(
    endpoint_name: &str,
    params: &[(&str, &str)],
) -> Result<Option<String>, AnyError> {
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
