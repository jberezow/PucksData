use std::collections::HashMap;

use pucksdata::endpoints::{get_endpoint, DataType};

#[test]
fn test_endpoint_registry() {
    // Test that we can get a known endpoint
    let endpoint = get_endpoint("game_boxscore");
    assert!(endpoint.is_some(), "game_boxscore endpoint should exist");

    let endpoint = endpoint.unwrap();
    assert_eq!(endpoint.name, "game_boxscore");
    assert_eq!(endpoint.data_type, DataType::Games);
    assert!(endpoint.implemented, "game_boxscore should be implemented");
}

#[test]
fn test_api_url_formatting() {
    // Test that API URLs are properly formatted
    let endpoint = get_endpoint("game_boxscore").unwrap();
    let mut params = HashMap::new();
    params.insert("game_id".to_string(), "2023020001".to_string());

    let url = endpoint.build_url(&params).expect("should build URL");

    assert!(url.contains("2023020001"), "URL should contain game ID");
    assert!(url.starts_with("https://"), "URL should be HTTPS");
    assert!(
        url.contains("api-web.nhle.com"),
        "URL should point to NHL API"
    );
}

#[test]
fn test_build_url_missing_parameter() {
    let endpoint = get_endpoint("game_boxscore").unwrap();
    let params = HashMap::new();

    let error = endpoint
        .build_url(&params)
        .expect_err("should error when parameter missing");

    assert!(
        error.to_string().contains("Required parameter 'game_id'"),
        "error should mention missing parameter"
    );
}
