use std::process::Command;
use std::path::Path;

/// Tests basic CLI functionality to ensure it runs without crashing
#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Check that the command runs successfully
    assert!(output.status.success(), "CLI help command failed");
    
    // Check that the help output contains expected sections
    assert!(stdout.contains("A command-line tool for fetching, caching, and processing NHL data"), "Help doesn't contain expected title");
    assert!(stdout.contains("Game-related data operations"), "Help doesn't describe game operations");
    assert!(stdout.contains("Player-related data operations"), "Help doesn't describe player operations");
}

/// Tests that the CLI correctly handles the game story command
#[test]
fn test_cli_game_story() {
    let test_game_id = "2023020001";
    let expected_cache_path = format!("data/raw/games/{}/story.json", test_game_id);
    
    // Remove cache file if it exists
    if Path::new(&expected_cache_path).exists() {
        std::fs::remove_file(&expected_cache_path).unwrap_or_else(|_| {
            println!("Warning: Could not delete existing cache file");
        });
    }
    
    // Run the command
    let output = Command::new("cargo")
        .args(["run", "--", "game", "story", test_game_id])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Check command output
    if !output.status.success() {
        println!("Command failed with stderr: {}", stderr);
    }
    
    // Either the command succeeded or it failed with a 404
    assert!(
        output.status.success() || stderr.contains("404") || stderr.contains("Not Found"),
        "CLI game story command failed unexpectedly: {}",
        stderr
    );
    
    // If successful, check that the cache file exists
    if output.status.success() {
        assert!(
            stdout.contains("Fetching story data") || stdout.contains("Found cached story data"),
            "Expected fetching or cache message, got: {}",
            stdout
        );
        
        // Only check for cache file if we got a success message
        if stdout.contains("Saved story data") {
            assert!(
                Path::new(&expected_cache_path).exists(),
                "Cache file was not created at {}",
                expected_cache_path
            );
        }
    }
}

/// Tests that the CLI correctly handles the player summary command
#[test]
fn test_cli_player_summary() {
    let test_player_id = "8478402"; // Connor McDavid
    let expected_cache_path = format!("data/raw/players/{}/summary.json", test_player_id);
    
    // Remove cache file if it exists
    if Path::new(&expected_cache_path).exists() {
        std::fs::remove_file(&expected_cache_path).unwrap_or_else(|_| {
            println!("Warning: Could not delete existing cache file");
        });
    }
    
    // Run the command
    let output = Command::new("cargo")
        .args(["run", "--", "player", "summary", test_player_id])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Check command output
    if !output.status.success() {
        println!("Command failed with stderr: {}", stderr);
    }
    
    // Either the command succeeded or it failed with a 404
    assert!(
        output.status.success() || stderr.contains("404") || stderr.contains("Not Found"),
        "CLI player summary command failed unexpectedly: {}",
        stderr
    );
    
    // If successful, check that the cache file exists
    if output.status.success() {
        assert!(
            stdout.contains("Fetching summary data") || stdout.contains("Found cached summary data"),
            "Expected fetching or cache message, got: {}",
            stdout
        );
        
        // Only check for cache file if we got a success message
        if stdout.contains("Saved summary data") {
            assert!(
                Path::new(&expected_cache_path).exists(),
                "Cache file was not created at {}",
                expected_cache_path
            );
        }
    }
} 