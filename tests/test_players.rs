#[test]
fn test_player_landing_deserialize() {
    // Matches the real NHL API response shape for player 8478402 (Connor McDavid)
    let json = r#"{
        "playerId": 8478402,
        "firstName": {"default": "Connor"},
        "lastName": {"default": "McDavid"},
        "position": "C",
        "shootsCatches": "L",
        "currentTeamAbbrev": "EDM",
        "birthDate": "1997-01-13",
        "heightInCentimeters": 185,
        "weightInKilograms": 88,
        "draftDetails": {"year": 2015, "teamAbbrev": "EDM", "round": 1, "pickInRound": 1, "overallPick": 1}
    }"#;
    // This should deserialize without panicking
    let player: pucksdata::fetchers::players::PlayerLanding = serde_json::from_str(json).unwrap();
    assert_eq!(player.first_name.default, "Connor");
    assert_eq!(player.last_name.default, "McDavid");
    assert!(player.draft_details.is_some());

    // Test player with no draft details and no current team
    let json2 = r#"{
        "playerId": 9999999,
        "firstName": {"default": "Test"},
        "lastName": {"default": "Player"},
        "position": null,
        "shootsCatches": null
    }"#;
    let p2: pucksdata::fetchers::players::PlayerLanding = serde_json::from_str(json2).unwrap();
    assert_eq!(p2.first_name.default, "Test");
    assert!(p2.current_team_abbrev.is_none());
    assert!(p2.draft_details.is_none());
}

#[tokio::test]
async fn test_players_upsert_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    // TODO: Plan 03-02 fills in this test body (Task 2)
}
