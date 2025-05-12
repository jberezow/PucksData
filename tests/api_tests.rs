use pucksdata::api;

// Test constants
const VALID_URL: &str = "https://api-web.nhle.com/v1/standings/now";
const INVALID_URL: &str = "https://api-web.nhle.com/v1/nonexistent-endpoint";

/// Tests whether the fetch_api_json function correctly handles a valid URL
#[test]
fn test_fetch_valid_endpoint() {
    match api::fetch_api_json(VALID_URL) {
        Ok(_) => {
            println!("✅ Successfully fetched data from {}", VALID_URL);
        }
        Err(api::ApiError::NotFound) => {
            panic!("❌ Expected valid endpoint, but got 404 Not Found for {}", VALID_URL);
        }
        Err(e) => {
            panic!("❌ Expected success, but got error: {:?} for {}", e, VALID_URL);
        }
    }
}

/// Tests whether the fetch_api_json function correctly handles a 404 response
#[test]
fn test_fetch_invalid_endpoint() {
    match api::fetch_api_json(INVALID_URL) {
        Ok(_) => {
            panic!("❌ Expected 404 Not Found, but request succeeded for {}", INVALID_URL);
        }
        Err(api::ApiError::NotFound) => {
            println!("✅ Correctly identified 404 for {}", INVALID_URL);
        }
        Err(e) => {
            panic!("❌ Expected 404 Not Found, but got different error: {:?} for {}", e, INVALID_URL);
        }
    }
}

/// Tests whether network errors are correctly handled
#[test]
fn test_network_error_handling() {
    // Malformed URL to trigger network error
    let malformed_url = "https://nonexistent-domain-12345.example";
    match api::fetch_api_json(malformed_url) {
        Ok(_) => {
            panic!("❌ Expected network error, but request succeeded for {}", malformed_url);
        }
        Err(api::ApiError::NetworkError(_)) => {
            println!("✅ Correctly identified network error for {}", malformed_url);
        }
        Err(e) => {
            panic!("❌ Expected NetworkError, but got different error: {:?} for {}", e, malformed_url);
        }
    }
} 