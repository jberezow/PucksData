pub mod integrity;
pub mod keys;
pub mod list;
pub mod local;
pub mod r2;
pub mod test;
pub mod schema_inspector;

use async_trait::async_trait;
use std::error::Error;

/// Storage backend abstraction for caching NHL API responses
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get a file from storage by key
    async fn get(&self, key: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>>;

    /// Put a file into storage with the given key
    async fn put(&self, key: &str, data: &str) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Check if a file exists in storage
    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;

    /// Delete a file from storage
    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// List files with a given prefix
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;
}

/// Storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub backend_type: StorageBackendType,
    pub local_path: Option<String>,
    pub r2_access_key_id: Option<String>,
    pub r2_secret_access_key: Option<String>,
    pub r2_endpoint: Option<String>,
    pub r2_bucket_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum StorageBackendType {
    Local,
    R2,
}

impl StorageConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, Box<dyn Error + Send + Sync>> {
        dotenvy::dotenv().ok(); // Load .env file if it exists

        let backend_type = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".to_string());

        let backend_type = match backend_type.as_str() {
            "r2" => StorageBackendType::R2,
            _ => StorageBackendType::Local,
        };

        Ok(StorageConfig {
            backend_type,
            local_path: std::env::var("LOCAL_STORAGE_PATH").ok(),
            r2_access_key_id: std::env::var("R2_ACCESS_KEY_ID").ok(),
            r2_secret_access_key: std::env::var("R2_SECRET_ACCESS_KEY").ok(),
            r2_endpoint: std::env::var("R2_ENDPOINT").ok(),
            r2_bucket_name: std::env::var("R2_BUCKET_NAME").ok(),
        })
    }
}

/// Create a storage backend based on configuration
pub async fn create_storage_backend(
    config: &StorageConfig,
) -> Result<Box<dyn StorageBackend>, Box<dyn Error + Send + Sync>> {
    match config.backend_type {
        StorageBackendType::Local => {
            let path = config.local_path.as_deref().unwrap_or("data/cache");
            Ok(Box::new(local::LocalStorage::new(path)))
        }
        StorageBackendType::R2 => {
            let access_key = config
                .r2_access_key_id
                .as_ref()
                .ok_or("R2_ACCESS_KEY_ID not set")?;
            let secret_key = config
                .r2_secret_access_key
                .as_ref()
                .ok_or("R2_SECRET_ACCESS_KEY not set")?;
            let endpoint = config.r2_endpoint.as_ref().ok_or("R2_ENDPOINT not set")?;
            let bucket = config
                .r2_bucket_name
                .as_ref()
                .ok_or("R2_BUCKET_NAME not set")?;

            let r2_storage = r2::R2Storage::new(access_key, secret_key, endpoint, bucket).await?;
            Ok(Box::new(r2_storage))
        }
    }
}
