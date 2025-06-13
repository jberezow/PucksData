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
    let url = endpoint.url.replace("{game_id}", "2023020001");
    
    assert!(url.contains("2023020001"), "URL should contain game ID");
    assert!(url.starts_with("https://"), "URL should be HTTPS");
    assert!(url.contains("api-web.nhle.com"), "URL should point to NHL API");
} 