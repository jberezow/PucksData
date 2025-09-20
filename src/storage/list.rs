use crate::storage::{create_storage_backend, StorageConfig};
use std::error::Error;

/// List all files in storage with optional prefix filter
pub async fn list_storage_files(prefix: Option<&str>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = StorageConfig::from_env()?;
    let storage = create_storage_backend(&config).await?;

    let prefix_str = prefix.unwrap_or("");
    let files = storage.list(prefix_str).await?;

    if files.is_empty() {
        println!("📂 No files found in storage with prefix '{}'", prefix_str);
    } else {
        println!("📂 Found {} files in storage:", files.len());
        for file in files {
            println!("  📄 {}", file);
        }
    }

    Ok(())
}
