use pucksdata::ingest;
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

// Type alias for closure
type TestFn = Box<dyn Fn() -> Result<(), Box<dyn std::error::Error>>>;

/// Enum to track test results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EndpointTestResult {
    Success,
    NotFound,
    NetworkError,
    OtherError,
}

/// Tests a single endpoint and returns the result
fn test_endpoint(name: &str, f: &TestFn) -> EndpointTestResult {
    println!("Testing endpoint: {}", name);
    match f() {
        Ok(_) => {
            println!("✅ {} - Success", name);
            EndpointTestResult::Success
        }
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("404") || error_str.contains("Not Found") {
                println!("🚫 {} - Not Found (404)", name);
                EndpointTestResult::NotFound
            } else if error_str.contains("Network error") {
                println!("🌐 {} - Network Error: {}", name, error_str);
                EndpointTestResult::NetworkError
            } else {
                println!("❌ {} - Other Error: {}", name, error_str);
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
    
    let tests: Vec<(&str, TestFn)> = vec![
        ("game_story", Box::new(|| ingest::fetch_game_story(TEST_GAME_ID))),
        ("game_boxscore", Box::new(|| ingest::fetch_game_boxscore(TEST_GAME_ID))),
        ("game_play_by_play", Box::new(|| ingest::fetch_game_play_by_play(TEST_GAME_ID))),
        ("game_all_games", Box::new(|| ingest::fetch_game_all_games())),
        ("game_metadata", Box::new(|| ingest::fetch_game_metadata())),
        ("game_content", Box::new(|| ingest::fetch_game_content(TEST_GAME_ID))),
        ("game_goal_replay", Box::new(|| ingest::fetch_game_goal_replay(TEST_GAME_ID, TEST_EVENT_ID))),
        ("game_odds", Box::new(|| ingest::fetch_game_odds(TEST_GAME_ID))),
        ("game_scores_now", Box::new(|| ingest::fetch_game_scores_now())),
        ("game_scores_date", Box::new(|| ingest::fetch_game_scores_date(TEST_DATE))),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_player_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests: Vec<(&str, TestFn)> = vec![
        ("player_summary", Box::new(|| ingest::fetch_player_summary(TEST_PLAYER_ID))),
        ("player_all", Box::new(|| ingest::fetch_player_all())),
        ("player_game_log", Box::new(|| ingest::fetch_player_game_log(TEST_PLAYER_ID, TEST_SEASON, TEST_GAME_TYPE))),
        ("player_game_log_now", Box::new(|| ingest::fetch_player_game_log_now(TEST_PLAYER_ID))),
        ("player_spotlight", Box::new(|| ingest::fetch_player_spotlight())),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_skater_and_goalie_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests: Vec<(&str, TestFn)> = vec![
        // Skater endpoints
        ("skater_leaders_now", Box::new(|| ingest::fetch_skater_leaders_now())),
        ("skater_leaders", Box::new(|| ingest::fetch_skater_leaders(TEST_SEASON, TEST_GAME_TYPE))),
        
        // Goalie endpoints
        ("goalie_leaders_now", Box::new(|| ingest::fetch_goalie_leaders_now())),
        ("goalie_leaders", Box::new(|| ingest::fetch_goalie_leaders(TEST_SEASON, TEST_GAME_TYPE))),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_team_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests: Vec<(&str, TestFn)> = vec![
        ("team_current_stats", Box::new(|| ingest::fetch_team_current_stats(TEST_TEAM_CODE))),
        ("team_stats_by_season", Box::new(|| ingest::fetch_team_stats_by_season(TEST_TEAM_CODE, TEST_SEASON, TEST_GAME_TYPE))),
        ("team_standings_now", Box::new(|| ingest::fetch_team_standings_now())),
        ("team_standings_date", Box::new(|| ingest::fetch_team_standings_date(TEST_DATE))),
        ("team_roster_now", Box::new(|| ingest::fetch_team_roster_now(TEST_TEAM_CODE))),
        ("team_roster_season", Box::new(|| ingest::fetch_team_roster_season(TEST_TEAM_CODE, TEST_SEASON))),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_schedule_and_playoff_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests: Vec<(&str, TestFn)> = vec![
        // Schedule endpoints
        ("schedule_now", Box::new(|| ingest::fetch_schedule_now())),
        ("schedule_date", Box::new(|| ingest::fetch_schedule_date(TEST_DATE))),
        
        // Playoff endpoints
        ("playoff_bracket", Box::new(|| ingest::fetch_playoff_bracket())),
        ("playoff_series_schedule", Box::new(|| ingest::fetch_playoff_series_schedule())),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

#[test]
fn test_season_draft_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests: Vec<(&str, TestFn)> = vec![
        // Season endpoints
        ("season_all", Box::new(|| ingest::fetch_season_all())),
        
        // Draft endpoints
        ("draft_current_rankings", Box::new(|| ingest::fetch_draft_current_rankings())),
        ("draft_tracker_now", Box::new(|| ingest::fetch_draft_tracker_now())),
        ("draft_picks_now", Box::new(|| ingest::fetch_draft_picks_now())),
        ("draft_picks", Box::new(|| ingest::fetch_draft_picks(TEST_DRAFT_YEAR))),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
}

/// Overall test that runs all endpoints - useful for a complete check
#[test]
fn test_all_endpoints() {
    let mut results = HashMap::<EndpointTestResult, usize>::new();
    
    let tests: Vec<(&str, TestFn)> = vec![
        // Game endpoints
        ("game_story", Box::new(|| ingest::fetch_game_story(TEST_GAME_ID))),
        ("game_boxscore", Box::new(|| ingest::fetch_game_boxscore(TEST_GAME_ID))),
        ("game_play_by_play", Box::new(|| ingest::fetch_game_play_by_play(TEST_GAME_ID))),
        ("game_all_games", Box::new(|| ingest::fetch_game_all_games())),
        ("game_metadata", Box::new(|| ingest::fetch_game_metadata())),
        ("game_content", Box::new(|| ingest::fetch_game_content(TEST_GAME_ID))),
        ("game_goal_replay", Box::new(|| ingest::fetch_game_goal_replay(TEST_GAME_ID, TEST_EVENT_ID))),
        ("game_odds", Box::new(|| ingest::fetch_game_odds(TEST_GAME_ID))),
        ("game_scores_now", Box::new(|| ingest::fetch_game_scores_now())),
        ("game_scores_date", Box::new(|| ingest::fetch_game_scores_date(TEST_DATE))),
        
        // Player endpoints
        ("player_summary", Box::new(|| ingest::fetch_player_summary(TEST_PLAYER_ID))),
        ("player_all", Box::new(|| ingest::fetch_player_all())),
        ("player_game_log", Box::new(|| ingest::fetch_player_game_log(TEST_PLAYER_ID, TEST_SEASON, TEST_GAME_TYPE))),
        ("player_game_log_now", Box::new(|| ingest::fetch_player_game_log_now(TEST_PLAYER_ID))),
        ("player_spotlight", Box::new(|| ingest::fetch_player_spotlight())),
        
        // Skater endpoints
        ("skater_leaders_now", Box::new(|| ingest::fetch_skater_leaders_now())),
        ("skater_leaders", Box::new(|| ingest::fetch_skater_leaders(TEST_SEASON, TEST_GAME_TYPE))),
        
        // Goalie endpoints
        ("goalie_leaders_now", Box::new(|| ingest::fetch_goalie_leaders_now())),
        ("goalie_leaders", Box::new(|| ingest::fetch_goalie_leaders(TEST_SEASON, TEST_GAME_TYPE))),
        
        // Team endpoints
        ("team_current_stats", Box::new(|| ingest::fetch_team_current_stats(TEST_TEAM_CODE))),
        ("team_stats_by_season", Box::new(|| ingest::fetch_team_stats_by_season(TEST_TEAM_CODE, TEST_SEASON, TEST_GAME_TYPE))),
        ("team_standings_now", Box::new(|| ingest::fetch_team_standings_now())),
        ("team_standings_date", Box::new(|| ingest::fetch_team_standings_date(TEST_DATE))),
        ("team_roster_now", Box::new(|| ingest::fetch_team_roster_now(TEST_TEAM_CODE))),
        ("team_roster_season", Box::new(|| ingest::fetch_team_roster_season(TEST_TEAM_CODE, TEST_SEASON))),
        
        // Schedule endpoints
        ("schedule_now", Box::new(|| ingest::fetch_schedule_now())),
        ("schedule_date", Box::new(|| ingest::fetch_schedule_date(TEST_DATE))),
        
        // Playoff endpoints
        ("playoff_bracket", Box::new(|| ingest::fetch_playoff_bracket())),
        ("playoff_series_schedule", Box::new(|| ingest::fetch_playoff_series_schedule())),
        
        // Season endpoints
        ("season_all", Box::new(|| ingest::fetch_season_all())),
        
        // Draft endpoints
        ("draft_current_rankings", Box::new(|| ingest::fetch_draft_current_rankings())),
        ("draft_tracker_now", Box::new(|| ingest::fetch_draft_tracker_now())),
        ("draft_picks_now", Box::new(|| ingest::fetch_draft_picks_now())),
        ("draft_picks", Box::new(|| ingest::fetch_draft_picks(TEST_DRAFT_YEAR))),
    ];
    
    for (name, test_fn) in &tests {
        let result = test_endpoint(name, test_fn);
        *results.entry(result).or_insert(0) += 1;
    }
    
    print_summary(&results, tests.len());
} 