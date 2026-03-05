#[test]
fn test_games_deserialize_stats_response() {
    // Matches the real NHL stats API shape for /en/game
    // CRITICAL: field is visitingTeamId / visitingScore, NOT awayTeamId / awayScore
    let json = r#"{
        "data": [
            {
                "id": 2024020001,
                "season": 20242025,
                "gameDate": "2024-10-08",
                "gameType": 2,
                "homeTeamId": 10,
                "visitingTeamId": 22,
                "homeScore": 3,
                "visitingScore": 1
            },
            {
                "id": 2024030211,
                "season": 20242025,
                "gameDate": "2025-05-01",
                "gameType": 3,
                "homeTeamId": 6,
                "visitingTeamId": 17,
                "homeScore": null,
                "visitingScore": null
            }
        ],
        "total": 2
    }"#;

    use pucksdata::fetchers::games::{StatsApiResponse, StatsGameRecord};
    let resp: StatsApiResponse<StatsGameRecord> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.data.len(), 2);

    let g = &resp.data[0];
    assert_eq!(g.id, 2024020001_i64);
    assert_eq!(g.season, 20242025_i32);
    assert_eq!(g.away_team_id, 22_i64);   // visitingTeamId mapped to away_team_id
    assert_eq!(g.home_score, Some(3_i16));
    assert_eq!(g.away_score, Some(1_i16)); // visitingScore mapped to away_score

    // Playoff game ID exceeds i32 max — must be i64
    let playoff = &resp.data[1];
    assert_eq!(playoff.id, 2024030211_i64);
    assert!(playoff.home_score.is_none());

    // Boxscore deserialization: venue is a localized object, teams have nested id/score
    let boxscore_json = r#"{
        "id": 2024020001,
        "startTimeUTC": "2024-10-09T00:00:00Z",
        "gameState": "OFF",
        "venue": {"default": "United Center"},
        "venueLocation": {"default": "Chicago, IL"},
        "homeTeam": {"id": 10, "score": 3},
        "awayTeam": {"id": 22, "score": 1}
    }"#;

    use pucksdata::fetchers::games::BoxscoreGame;
    let bs: BoxscoreGame = serde_json::from_str(boxscore_json).unwrap();
    assert_eq!(bs.id, 2024020001_i64);
    assert_eq!(bs.venue.as_ref().map(|v| v.default.as_str()), Some("United Center"));
    assert_eq!(bs.home_team.score, Some(3_i16));
    assert_eq!(bs.away_team.score, Some(1_i16));
}

#[tokio::test]
async fn test_games_upsert_idempotent() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    // TODO: Plan 03-03 fills in this test body
}

#[tokio::test]
#[ignore]
async fn test_fetch_idempotency() {
    if std::env::var("DATABASE_URL").is_err() { return; }
    // TODO: Plan 03-03 fills in this test body
}
