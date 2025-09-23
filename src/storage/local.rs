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

        let mut stack = vec![(prefix.to_string(), prefix_path)];

        while let Some((current_prefix, current_path)) = stack.pop() {
            let mut entries = match fs::read_dir(&current_path).await {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(Box::new(e)),
            };

            while let Some(entry) = entries.next_entry().await? {
                let file_type = entry.file_type().await?;
                let file_name = match entry.file_name().to_str() {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                let relative_key = if current_prefix.is_empty() {
                    file_name.clone()
                } else {
                    format!("{}/{}", current_prefix, file_name)
                };

                if file_type.is_dir() {
                    stack.push((relative_key, entry.path()));
                } else if file_type.is_file() {
                    files.push(relative_key);
                }
            }
        }

        files.sort();
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn lists_nested_files_recursively() {
        let temp_dir = TempDir::new().expect("tempdir");
        let base_path = temp_dir.path().join("cache");
        tokio::fs::create_dir_all(base_path.join("games/2023"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(base_path.join("games/2024/nested"))
            .await
            .unwrap();
        tokio::fs::write(base_path.join("games/2023/file.json"), "{}")
            .await
            .unwrap();
        tokio::fs::write(base_path.join("games/2024/nested/file.json"), "{}")
            .await
            .unwrap();

        let storage = LocalStorage::new(base_path.to_str().unwrap());
        let mut files = storage.list("games").await.unwrap();
        files.sort();

        assert_eq!(
            files,
            vec![
                "games/2023/file.json".to_string(),
                "games/2024/nested/file.json".to_string()
            ]
        );
    }
}
