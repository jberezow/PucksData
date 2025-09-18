use crate::storage::StorageBackend;
use async_trait::async_trait;
use std::error::Error;
use std::path::PathBuf;
use tokio::fs;

/// Local filesystem storage backend
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    pub fn new(base_path: &str) -> Self {
        Self {
            base_path: PathBuf::from(base_path),
        }
    }
    
    fn get_file_path(&self, key: &str) -> PathBuf {
        self.base_path.join(key)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let file_path = self.get_file_path(key);
        
        match fs::read_to_string(&file_path).await {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Box::new(e)),
        }
    }
    
    async fn put(&self, key: &str, data: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let file_path = self.get_file_path(key);
        
        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        fs::write(&file_path, data).await?;
        Ok(())
    }
    
    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let file_path = self.get_file_path(key);
        Ok(file_path.exists())
    }
    
    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let file_path = self.get_file_path(key);
        
        match fs::remove_file(&file_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // Already deleted
            Err(e) => Err(Box::new(e)),
        }
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let prefix_path = self.base_path.join(prefix);
        let mut files = Vec::new();
        
        if !prefix_path.exists() {
            return Ok(files);
        }
        
        let mut entries = fs::read_dir(&prefix_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                if let Some(file_name) = entry.file_name().to_str() {
                    let key = format!("{}/{}", prefix, file_name);
                    files.push(key);
                }
            }
        }
        
        Ok(files)
    }
}