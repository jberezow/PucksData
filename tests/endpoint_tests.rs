use pucksdata::ingest;
use pucksdata::endpoints::{get_endpoint, get_all_endpoints};
use std::collections::HashMap;

// Test constants
const TEST_GAME_ID: &str = "2023020001"; // A known game ID from the 2023-2024 season
const TEST_PLAYER_ID: &str = "8478402";  // Connor McDavid
const TEST_TEAM_CODE: &str = "EDM";      // Edmonton Oilers
const TEST_SEASON: &str = "20232024";    // 2023-2024 season
const TEST_GAME_TYPE: &str = "2";        // Regular season
const TEST_DATE: &str = "2024-02-15";    // A date in the 2023-2024 season
const TEST_EVENT_ID: &str = "401";       // An event ID for a goal
const TEST_DRAFT_YEAR: &str = "2023";    // 2023 draft
const TEST_PLAYOFF_YEAR: &str = "2024";  // 2024 playoffs
const TEST_PLAYOFF_SEASON: &str = "20232024"; // 2023-2024 playoff season
const TEST_SERIES_LETTER: &str = "a";    // Series letter for playoff series

/// Enum to track test results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EndpointTestResult {
    Success,
    NotFound,
    NetworkError,
    OtherError,
}

/// Tests a single endpoint by name and returns the result
fn test_endpoint_by_name(endpoint_name: &str) -> EndpointTestResult {
    println!("Testing endpoint: {}", endpoint_name);
    
    // Get endpoint definition
    let endpoint = match get_endpoint(endpoint_name) {
        Some(e) => e,
        None => {
            println!("❓ {} - Endpoint not defined in registry", endpoint_name);
            return EndpointTestResult::OtherError;
        }
    };
    
    // Map parameters based on endpoint
    let mut params = Vec::new();
    for param in &endpoint.parameters {
        if param.required {
            // Map common test parameters
            let value = match param.name {
                "game_id" => TEST_GAME_ID,
                "player_id" => TEST_PLAYER_ID,
                "team_code" => TEST_TEAM_CODE,
                "season" => TEST_SEASON,
                "game_type" => TEST_GAME_TYPE,
                "date" => TEST_DATE,
                "event_id" => TEST_EVENT_ID,
                "year" => TEST_DRAFT_YEAR,
                "letter" => TEST_SERIES_LETTER,
                _ => {
                    println!("⚠️ {} - Unknown parameter: {}", endpoint_name, param.name);
                    param.example // Use example value as fallback
                }
            };
            params.push((param.name, value));
        }
    }
    
    // Call the endpoint
    match ingest::fetch_endpoint(endpoint_name, &params) {
        Ok(_) => {
            println!("✅ {} - Success", endpoint_name);
            EndpointTestResult::Success
        }
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("404") || error_str.contains("Not Found") {
                println!("🚫 {} - Not Found (404)", endpoint_name);
                EndpointTestResult::NotFound
            } else if error_str.contains("Network error") {
                println!("🌐 {} - Network Error: {}", endpoint_name, error_str);
                EndpointTestResult::NetworkError
            } else {
                println!("❌ {} - Other Error: {}", endpoint_name, error_str);
                EndpointTestResult::OtherError
            }
        }
    }
}

/// Helper function to print test summary
fn print_summary(results: &HashMap<EndpointTestResult, usize>, total_tests: usize) {
    let success_count = *results.get(&EndpointTestResult::Success).unwrap_or(&0);
    let not_found_count = *results.get(&EndpointTestResult::NotFound).unwrap_or(&0);
    let network_error_count = *results.get(&EndpointTestResult::NetworkError).unwrap_or(&0);
    let other_error_count = *results.get(&EndpointTestResult::OtherError).unwrap_or(&0);
    
    println!("\n--- ENDPOINT TEST SUMMARY ---");
    println!("Total endpoints tested: {}", total_tests);
    println!("✅ Successful: {}", success_count);
    println!("🚫 Not Found (404): {}", not_found_count);
    println!("🌐 Network Errors: {}", network_error_count);
    println!("❌ Other Errors: {}", other_error_count);
    println!("-----------------------------\n");
    
    // At least some endpoints should be successful
    assert!(success_count > 0, "Expected at least some successful endpoint tests");
}

// Individual test functions for each category

#[test]
fn test_game_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests = vec![
        "game_story",
        "game_boxscore",
        "game_play_by_play",
        "game_all_games",
        "game_content",
        "game_goal_replay",
        "game_scores_now",
        "game_scores_date",
    ];
    
    for endpoint_name in &tests {
        let result = test_endpoint_by_name(endpoint_name);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_player_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests = vec![
        "player_summary",
        "player_all",
        "player_game_log",
        "player_game_log_now",
        "player_spotlight",
    ];
    
    for endpoint_name in &tests {
        let result = test_endpoint_by_name(endpoint_name);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_skater_and_goalie_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests = vec![
        // Skater endpoints
        "skater_stats_leaders_now",
        "skater_stats_leaders",
        
        // Goalie endpoints
        "goalie_stats_leaders_now",
        "goalie_stats_leaders",
    ];
    
    for endpoint_name in &tests {
        let result = test_endpoint_by_name(endpoint_name);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_team_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests = vec![
        "team_current_stats",
        "team_stats_by_season",
        "team_standings_now",
        "team_standings_by_date",
        "team_standings_season",
        "team_roster_now",
        "team_roster_season",
        "team_prospects",
        "team_schedule_now",
        "team_schedule_season",
        "team_schedule_month",
    ];
    
    for endpoint_name in &tests {
        let result = test_endpoint_by_name(endpoint_name);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_schedule_and_playoff_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests = vec![
        // Schedule endpoints
        "schedule_now",
        "schedule_date",
        
        // Playoff endpoints
        "playoff_bracket",
        "playoff_series_schedule",
        "playoff_series_carousel", 
        "playoff_series_metadata",
    ];
    
    for endpoint_name in &tests {
        let result = test_endpoint_by_name(endpoint_name);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_season_draft_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests = vec![
        // Season endpoints
        "season_all_seasons",
        
        // Draft endpoints
        "draft_current_rankings",
        "draft_tracker_now",
        "draft_picks_now",
        "draft_picks",
    ];
    
    for endpoint_name in &tests {
        let result = test_endpoint_by_name(endpoint_name);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_all_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    let all_endpoints = get_all_endpoints();
    
    println!("Testing all {} registered endpoints", all_endpoints.len());
    
    for endpoint in all_endpoints {
        if endpoint.implemented {
            let result = test_endpoint_by_name(endpoint.name);
            *results.entry(result).or_insert(0) += 1;
        } else {
            println!("⏩ {} - Skipping unimplemented endpoint", endpoint.name);
        }
    }
    
    print_summary(&results, all_endpoints.len());
} 