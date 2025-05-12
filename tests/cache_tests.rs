use pucksdata::cache;
use std::fs;
use std::path::PathBuf;

// Test constants
const TEST_CONTENT: &str = r#"{"test": "data", "value": 123}"#;

/// Tests writing to and reading from the cache
#[test]
fn test_write_read_cache() {
    // Create a test directory in the cache
    let test_dir = PathBuf::from("data/test_cache");
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");
    
    // Test file path
    let test_file = test_dir.join("test_data.json");
    
    // Clean up any existing test file
    if test_file.exists() {
        fs::remove_file(&test_file).expect("Failed to clean up existing test file");
    }
    
    // Test writing to cache
    cache::write_to_cache(&test_file, TEST_CONTENT).expect("Failed to write to cache");
    
    // Verify the file exists
    assert!(test_file.exists(), "Cache file was not created");
    
    // Test reading from cache
    let cached_content = cache::read_from_cache(&test_file);
    assert!(cached_content.is_some(), "Failed to read from cache");
    assert_eq!(cached_content.unwrap(), TEST_CONTENT, "Cache content doesn't match original data");
    
    // Clean up
    fs::remove_file(&test_file).expect("Failed to clean up test file");
    fs::remove_dir(&test_dir).expect("Failed to clean up test directory");
}

/// Tests reading from a non-existent cache file
#[test]
fn test_read_nonexistent_cache() {
    let nonexistent_file = PathBuf::from("data/test_cache/nonexistent.json");
    
    // Make sure the file doesn't exist
    if nonexistent_file.exists() {
        fs::remove_file(&nonexistent_file).expect("Failed to clean up existing test file");
    }
    
    // Test reading from a non-existent cache
    let cached_content = cache::read_from_cache(&nonexistent_file);
    assert!(cached_content.is_none(), "Expected None for non-existent file, but got Some");
} 