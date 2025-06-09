use std::process::Command;
use std::env;

fn setup() {
    // dotenv is not needed if not used elsewhere
}

#[test]
fn test_list_endpoints_command() {
    setup();
    let output = Command::new(env!("CARGO_BIN_EXE_pucksdata"))
        .arg("list-endpoints")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let _stdout = String::from_utf8_lossy(&output.stdout);
    let _stderr = String::from_utf8_lossy(&output.stderr);
    assert!(_stderr.is_empty(), "Stderr should be empty");
}

#[test]
fn test_games_boxscore_command() {
    setup();
    let output = Command::new(env!("CARGO_BIN_EXE_pucksdata"))
        .env("DATABASE_URL", env::var("DATABASE_URL").expect("DATABASE_URL must be set for test"))
        .args(["games", "boxscore", "2023020001"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        panic!("Command failed with non-zero status. Stderr:\n{}", stderr);
    }
}