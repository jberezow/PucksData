use pucksdata::inspect;
use std::collections::HashMap;

// Test constants - using the same ones from endpoint_tests.rs
const TEST_GAME_ID: &str = "2023020001"; // A known game ID from the 2023-2024 season
const TEST_PLAYER_ID: &str = "8478402";  // Connor McDavid
const TEST_TEAM_CODE: &str = "EDM";      // Edmonton Oilers
const TEST_SEASON: &str = "20232024";    // 2023-2024 season
const TEST_GAME_TYPE: &str = "2";        // Regular season
const TEST_DATE: &str = "2024-02-15";    // A date in the 2023-2024 season
const TEST_EVENT_ID: &str = "401";       // An event ID for a goal

// Type alias for inspection test function
type InspectFn = Box<dyn Fn() -> Result<(), Box<dyn std::error::Error>>>;

/// Runs an inspection function and returns the result
fn run_inspect(name: &str, f: &InspectFn) {
    println!("Inspecting endpoint: {}", name);
    match f() {
        Ok(_) => {
            println!("✅ Inspection successful: {}", name);
        }
        Err(e) => {
            println!("❌ Inspection failed: {} - {}", name, e);
        }
    }
}

#[test]
fn inspect_game_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        ("game_story", Box::new(|| inspect::inspect_keys("games", "story", TEST_GAME_ID))),
        ("game_boxscore", Box::new(|| inspect::inspect_keys("games", "boxscore", TEST_GAME_ID))),
        ("game_play_by_play", Box::new(|| inspect::inspect_keys("games", "playbyplay", TEST_GAME_ID))),
        ("game_content", Box::new(|| inspect::inspect_keys("games", "content", TEST_GAME_ID))),
        ("game_goal_replay", Box::new(|| inspect::inspect_keys("games", "goal_replay", &format!("{}/{}", TEST_GAME_ID, TEST_EVENT_ID)))),
        ("game_odds", Box::new(|| inspect::inspect_keys("games", "odds", TEST_GAME_ID))),
        ("game_scores_now", Box::new(|| inspect::inspect_keys("games", "scores_now", ""))),
        ("game_scores_date", Box::new(|| inspect::inspect_keys("games", "scores_date", TEST_DATE))),
    ];
    
    for (name, inspect_fn) in &tests {
        run_inspect(name, inspect_fn);
    }
}

#[test]
fn inspect_player_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        ("player_summary", Box::new(|| inspect::inspect_keys("players", "summary", TEST_PLAYER_ID))),
        ("player_game_log", Box::new(|| inspect::inspect_keys("players", "game_log", &format!("{}/{}", TEST_PLAYER_ID, TEST_SEASON)))),
        ("player_game_log_now", Box::new(|| inspect::inspect_keys("players", "game_log_now", TEST_PLAYER_ID))),
        ("player_spotlight", Box::new(|| inspect::inspect_keys("players", "spotlight", ""))),
    ];
    
    for (name, inspect_fn) in &tests {
        run_inspect(name, inspect_fn);
    }
}

#[test]
fn inspect_skater_and_goalie_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        // Skater endpoints
        ("skater_leaders_now", Box::new(|| inspect::inspect_keys("skaters", "leaders_now", ""))),
        ("skater_leaders", Box::new(|| inspect::inspect_keys("skaters", "leaders", &format!("{}/{}", TEST_SEASON, TEST_GAME_TYPE)))),
        
        // Goalie endpoints
        ("goalie_leaders_now", Box::new(|| inspect::inspect_keys("goalies", "leaders_now", ""))),
        ("goalie_leaders", Box::new(|| inspect::inspect_keys("goalies", "leaders", &format!("{}/{}", TEST_SEASON, TEST_GAME_TYPE)))),
    ];
    
    for (name, inspect_fn) in &tests {
        run_inspect(name, inspect_fn);
    }
}

#[test]
fn inspect_team_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        ("team_current_stats", Box::new(|| inspect::inspect_keys("teams", "current_stats", TEST_TEAM_CODE))),
        ("team_stats_by_season", Box::new(|| inspect::inspect_keys("teams", "season_stats", &format!("{}/{}/{}", TEST_TEAM_CODE, TEST_SEASON, TEST_GAME_TYPE)))),
        ("team_standings_now", Box::new(|| inspect::inspect_keys("teams", "standings_now", ""))),
        ("team_standings_date", Box::new(|| inspect::inspect_keys("teams", "standings_date", TEST_DATE))),
        ("team_roster_now", Box::new(|| inspect::inspect_keys("teams", "roster_now", TEST_TEAM_CODE))),
        ("team_roster_season", Box::new(|| inspect::inspect_keys("teams", "roster_season", &format!("{}/{}", TEST_TEAM_CODE, TEST_SEASON)))),
    ];
    
    for (name, inspect_fn) in &tests {
        run_inspect(name, inspect_fn);
    }
}

#[test]
fn inspect_schedule_and_playoff_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        // Schedule endpoints
        ("schedule_now", Box::new(|| inspect::inspect_keys("schedule", "now", ""))),
        ("schedule_date", Box::new(|| inspect::inspect_keys("schedule", "date", TEST_DATE))),
        
        // Playoff endpoints
        ("playoff_bracket", Box::new(|| inspect::inspect_keys("playoffs", "bracket", ""))),
        ("playoff_series_schedule", Box::new(|| inspect::inspect_keys("playoffs", "series_schedule", ""))),
    ];
    
    for (name, inspect_fn) in &tests {
        run_inspect(name, inspect_fn);
    }
}

#[test]
fn inspect_season_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        // Season endpoints
        ("season_all", Box::new(|| inspect::inspect_keys("seasons", "all", ""))),
    ];
    
    for (name, inspect_fn) in &tests {
        run_inspect(name, inspect_fn);
    }
}

/// Test that inspects all endpoints in one go
#[test]
fn inspect_all_endpoints() {
    let tests: Vec<(&str, InspectFn)> = vec![
        // Game endpoints
        ("game_story", Box::new(|| inspect::inspect_keys("games", "story", TEST_GAME_ID))),
        ("game_boxscore", Box::new(|| inspect::inspect_keys("games", "boxscore", TEST_GAME_ID))),
        ("game_play_by_play", Box::new(|| inspect::inspect_keys("games", "playbyplay", TEST_GAME_ID))),
        ("game_content", Box::new(|| inspect::inspect_keys("games", "content", TEST_GAME_ID))),
        ("game_goal_replay", Box::new(|| inspect::inspect_keys("games", "goal_replay", &format!("{}/{}", TEST_GAME_ID, TEST_EVENT_ID)))),
        ("game_odds", Box::new(|| inspect::inspect_keys("games", "odds", TEST_GAME_ID))),
        ("game_scores_now", Box::new(|| inspect::inspect_keys("games", "scores_now", ""))),
        ("game_scores_date", Box::new(|| inspect::inspect_keys("games", "scores_date", TEST_DATE))),
        
        // Player endpoints
        ("player_summary", Box::new(|| inspect::inspect_keys("players", "summary", TEST_PLAYER_ID))),
        ("player_game_log", Box::new(|| inspect::inspect_keys("players", "game_log", &format!("{}/{}", TEST_PLAYER_ID, TEST_SEASON)))),
        ("player_game_log_now", Box::new(|| inspect::inspect_keys("players", "game_log_now", TEST_PLAYER_ID))),
        ("player_spotlight", Box::new(|| inspect::inspect_keys("players", "spotlight", ""))),
        
        // Skater endpoints
        ("skater_leaders_now", Box::new(|| inspect::inspect_keys("skaters", "leaders_now", ""))),
        ("skater_leaders", Box::new(|| inspect::inspect_keys("skaters", "leaders", &format!("{}/{}", TEST_SEASON, TEST_GAME_TYPE)))),
        
        // Goalie endpoints
        ("goalie_leaders_now", Box::new(|| inspect::inspect_keys("goalies", "leaders_now", ""))),
        ("goalie_leaders", Box::new(|| inspect::inspect_keys("goalies", "leaders", &format!("{}/{}", TEST_SEASON, TEST_GAME_TYPE)))),
        
        // Team endpoints
        ("team_current_stats", Box::new(|| inspect::inspect_keys("teams", "current_stats", TEST_TEAM_CODE))),
        ("team_stats_by_season", Box::new(|| inspect::inspect_keys("teams", "season_stats", &format!("{}/{}/{}", TEST_TEAM_CODE, TEST_SEASON, TEST_GAME_TYPE)))),
        ("team_standings_now", Box::new(|| inspect::inspect_keys("teams", "standings_now", ""))),
        ("team_standings_date", Box::new(|| inspect::inspect_keys("teams", "standings_date", TEST_DATE))),
        ("team_roster_now", Box::new(|| inspect::inspect_keys("teams", "roster_now", TEST_TEAM_CODE))),
        ("team_roster_season", Box::new(|| inspect::inspect_keys("teams", "roster_season", &format!("{}/{}", TEST_TEAM_CODE, TEST_SEASON)))),
        
        // Schedule endpoints
        ("schedule_now", Box::new(|| inspect::inspect_keys("schedule", "now", ""))),
        ("schedule_date", Box::new(|| inspect::inspect_keys("schedule", "date", TEST_DATE))),
        
        // Playoff endpoints
        ("playoff_bracket", Box::new(|| inspect::inspect_keys("playoffs", "bracket", ""))),
        ("playoff_series_schedule", Box::new(|| inspect::inspect_keys("playoffs", "series_schedule", ""))),
        
        // Season endpoints
        ("season_all", Box::new(|| inspect::inspect_keys("seasons", "all", ""))),
    ];
    
    println!("\n--- RUNNING ALL ENDPOINT INSPECTIONS ---");
    let mut success_count = 0;
    let mut fail_count = 0;
    
    for (name, inspect_fn) in &tests {
        println!("Inspecting endpoint: {}", name);
        match inspect_fn() {
            Ok(_) => {
                println!("✅ Inspection successful: {}", name);
                success_count += 1;
            }
            Err(e) => {
                println!("❌ Inspection failed: {} - {}", name, e);
                fail_count += 1;
            }
        }
    }
    
    println!("\n--- INSPECTION SUMMARY ---");
    println!("Total endpoints inspected: {}", tests.len());
    println!("✅ Successful: {}", success_count);
    println!("❌ Failed: {}", fail_count);
    println!("------------------------\n");
    
    // At least some endpoints should be successful
    assert!(success_count > 0, "Expected at least some successful endpoint inspections");
} 