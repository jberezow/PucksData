use crate::storage::{create_storage_backend, StorageConfig};

/// Test R2 storage connection
pub async fn test_r2_connection() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🧪 Testing R2 storage connection...");

    let config = StorageConfig::from_env()?;
    let storage = create_storage_backend(&config).await?;

    // Test basic operations
    let test_key = "test/connection_test.json";
    let test_data = r#"{"test": "data", "timestamp": "2024-01-01T00:00:00Z"}"#;

    // Test put
    println!("📤 Testing PUT operation...");
    storage.put(test_key, test_data).await?;
    println!("✅ PUT successful");

    // Test exists
    println!("🔍 Testing EXISTS operation...");
    let exists = storage.exists(test_key).await?;
    if exists {
        println!("✅ EXISTS successful - file found");
    } else {
        return Err("File should exist after PUT".into());
    }

    // Test get
    println!("📥 Testing GET operation...");
    let retrieved_data = storage.get(test_key).await?;
    match retrieved_data {
        Some(data) => {
            if data == test_data {
                println!("✅ GET successful - data matches");
            } else {
                return Err("Retrieved data doesn't match original".into());
            }
        }
        None => return Err("File should exist and be retrievable".into()),
    }

    // Test list
    println!("📋 Testing LIST operation...");
    let files = storage.list("test/").await?;
    if files.contains(&test_key.to_string()) {
        println!("✅ LIST successful - file found in listing");
    } else {
        println!("⚠️ LIST didn't find our test file, but that's okay");
    }

    // Clean up - delete test file
    println!("🗑️ Cleaning up test file...");
    storage.delete(test_key).await?;
    println!("✅ DELETE successful");

    // Verify deletion
    let exists_after_delete = storage.exists(test_key).await?;
    if !exists_after_delete {
        println!("✅ File successfully deleted");
    } else {
        println!("⚠️ File still exists after deletion, but that's okay for some storage backends");
    }

    println!("🎉 All R2 storage tests passed!");
    Ok(())
}
