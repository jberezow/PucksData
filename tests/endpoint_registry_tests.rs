use pucksdata::endpoints::{get_endpoint, get_endpoints_by_type, DataType};

#[test]
fn test_endpoint_registry() {
    // Test that we can retrieve endpoints by name
    let game_story = get_endpoint("game_story");
    assert!(game_story.is_some(), "game_story endpoint should exist");
    
    if let Some(endpoint) = game_story {
        assert_eq!(endpoint.name, "game_story");
        assert_eq!(endpoint.data_type, DataType::Games);
        assert!(endpoint.implemented);
        assert_eq!(endpoint.parameters.len(), 1);
        assert_eq!(endpoint.parameters[0].name, "game_id");
    }
    
    // Test that we can filter endpoints by data type
    let game_endpoints = get_endpoints_by_type(DataType::Games);
    assert!(!game_endpoints.is_empty(), "should have game endpoints");
    assert!(game_endpoints.iter().all(|e| e.data_type == DataType::Games), 
            "all returned endpoints should be game endpoints");
}

#[test]
fn test_endpoint_url_parameters() {
    // Test that endpoint URLs contain the correct parameter placeholders
    let game_story = get_endpoint("game_story").unwrap();
    assert!(game_story.url.contains("{game_id}"), 
            "game_story URL should contain {{game_id}} placeholder");
    
    // Verify that parameters in URL match declared parameters
    for param in &game_story.parameters {
        assert!(game_story.url.contains(&format!("{{{}}}", param.name)),
                "URL should contain placeholder for parameter {}", param.name);
    }
}

#[test]
fn test_endpoint_test_params() {
    // Verify that test parameters match required parameters
    let endpoints_with_required_params = get_endpoints_by_type(DataType::Games)
        .into_iter()
        .filter(|e| !e.parameters.is_empty())
        .collect::<Vec<_>>();
    
    for endpoint in endpoints_with_required_params {
        for param in &endpoint.parameters {
            if param.required {
                assert!(endpoint.test_params.contains_key(param.name),
                        "Endpoint {} should have test parameter for required param {}", 
                        endpoint.name, param.name);
            }
        }
    }
} 