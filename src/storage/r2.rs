use crate::storage::StorageBackend;
use async_trait::async_trait;
use aws_config::Region;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, Config};
use std::error::Error;

/// Cloudflare R2 storage backend using S3-compatible API
pub struct R2Storage {
    client: Client,
    bucket: String,
}

impl R2Storage {
    pub async fn new(
        access_key_id: &str,
        secret_access_key: &str,
        endpoint: &str,
        bucket: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Create credentials
        let credentials = Credentials::new(
            access_key_id,
            secret_access_key,
            None, // session_token
            None, // expiration
            "r2-credentials",
        );
        
        // Configure S3 client for R2
        let config = Config::builder()
            .credentials_provider(credentials)
            .endpoint_url(endpoint)
            .region(Region::new("auto")) // R2 uses "auto" region
            .force_path_style(true) // Required for R2
            .build();
        
        let client = Client::from_conf(config);
        
        Ok(Self {
            client,
            bucket: bucket.to_string(),
        })
    }
}

#[async_trait]
impl StorageBackend for R2Storage {
    async fn get(&self, key: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        match self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(response) => {
                let body = response.body.collect().await?;
                let content = String::from_utf8(body.into_bytes().to_vec())?;
                Ok(Some(content))
            }
            Err(e) => {
                // Check if it's a "not found" error
                let error_string = format!("{:?}", e);
                if error_string.contains("NoSuchKey") || error_string.contains("NotFound") {
                    return Ok(None);
                }
                Err(Box::new(e))
            }
        }
    }
    
    async fn put(&self, key: &str, data: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.as_bytes().to_vec().into())
            .send()
            .await?;
        
        Ok(())
    }
    
    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        match self.client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a "not found" error
                let error_string = format!("{:?}", e);
                if error_string.contains("NoSuchKey") || error_string.contains("NotFound") {
                    return Ok(false);
                }
                Err(Box::new(e))
            }
        }
    }
    
    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        
        Ok(())
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let response = self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await?;
        
        let mut keys = Vec::new();
        if let Some(contents) = response.contents {
            for object in contents {
                if let Some(key) = object.key {
                    keys.push(key);
                }
            }
        }
        
        Ok(keys)
    }
}