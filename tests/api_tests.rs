use pucksdata::api::{self, ApiError};

#[tokio::test]
async fn test_fetch_valid_url() {
    const VALID_URL: &str = "https://api-web.nhle.com/v1/gamecenter/2023020001/boxscore";
    match api::fetch_api_json(VALID_URL).await {
        Ok(_) => {
            // Success, do nothing
        }
        Err(ApiError::NotFound) => {
            panic!("Should not return NotFound for a valid URL");
        }
        Err(e) => {
            panic!("An unexpected error occurred: {}", e);
        }
    }
}

#[tokio::test]
async fn test_fetch_invalid_url() {
    const INVALID_URL: &str = "https://api-web.nhle.com/v1/invalid/endpoint";
    match api::fetch_api_json(INVALID_URL).await {
        Ok(_) => {
            panic!("Should have returned an error for an invalid URL");
        }
        Err(ApiError::NotFound) => {
            // This is the expected outcome
        }
        Err(e) => {
            panic!("An unexpected error occurred: {}", e);
        }
    }
}

#[tokio::test]
async fn test_fetch_malformed_url() {
    let malformed_url = "this-is-not-a-valid-url";
    match api::fetch_api_json(malformed_url).await {
        Ok(_) => {
            panic!("Should have returned an error for a malformed URL");
        }
        Err(ApiError::NetworkError(_)) => {
            // This is the expected outcome
        }
        Err(e) => {
            panic!("An unexpected error occurred: {}", e);
        }
    }
}