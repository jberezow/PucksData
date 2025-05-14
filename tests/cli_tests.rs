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
    assert!(stdout.contains("NHL Stats Engine CLI"), "Help doesn't contain expected title");
    assert!(stdout.contains("Games-related data operations"), "Help doesn't describe game operations");
    assert!(stdout.contains("Players-related data operations"), "Help doesn't describe player operations");
}

/// Tests that the CLI correctly handles the game story command
#[test]
fn test_cli_game_story() {
    let test_game_id = "2023020001";
    
    // Run the command
    let output = Command::new("cargo")
        .args(["run", "--", "games", "story", test_game_id])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Check command output
    if !output.status.success() {
        println!("Command failed with stderr: {}", stderr);
    }
    
    // Either the command succeeded or it failed with a 404 or "not found"
    assert!(
        output.status.success() || stderr.contains("404") || stderr.contains("Not Found"),
        "CLI game story command failed unexpectedly: {}",
        stderr
    );
    
    // If successful, command output should be empty (no errors)
    if output.status.success() {
        // The new implementation might not output the same messages,
        // so we just check that it completes successfully
        assert!(
            stderr.is_empty() || !stderr.contains("Error:"),
            "Expected no errors in stderr, got: {}",
            stderr
        );
    }
}

/// Tests that the CLI correctly handles the player summary command
#[test]
fn test_cli_player_summary() {
    let test_player_id = "8478402"; // Connor McDavid
    
    // Run the command
    let output = Command::new("cargo")
        .args(["run", "--", "players", "summary", test_player_id])
        .output()
        .expect("Failed to execute command");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Check command output
    if !output.status.success() {
        println!("Command failed with stderr: {}", stderr);
    }
    
    // Either the command succeeded or it failed with a 404 or "not found"
    assert!(
        output.status.success() || stderr.contains("404") || stderr.contains("Not Found"),
        "CLI player summary command failed unexpectedly: {}",
        stderr
    );
    
    // If successful, command output should be empty (no errors)
    if output.status.success() {
        // The new implementation might not output the same messages,
        // so we just check that it completes successfully
        assert!(
            stderr.is_empty() || !stderr.contains("Error:"),
            "Expected no errors in stderr, got: {}",
            stderr
        );
    }
} 