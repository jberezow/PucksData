#[test]
fn test_games_deserialize_stats_response() {
    // The stats endpoint uses `visitingTeamId` and `visitingScore`.
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
    assert_eq!(g.away_team_id, 22_i64); // visitingTeamId mapped to away_team_id
    assert_eq!(g.home_score, Some(3_i16));
    assert_eq!(g.away_score, Some(1_i16)); // visitingScore mapped to away_score

    // Playoff game ID exceeds i32 max — must be i64
    let playoff = &resp.data[1];
    assert_eq!(playoff.id, 2024030211_i64);
    assert!(playoff.home_score.is_none());

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
    assert_eq!(
        bs.venue.as_ref().map(|v| v.default.as_str()),
        Some("United Center")
    );
    assert_eq!(bs.home_team.score, Some(3_i16));
    assert_eq!(bs.away_team.score, Some(1_i16));
}

#[tokio::test]
async fn test_games_upsert_empty_without_connection() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://localhost/pucksdata_test")
        .unwrap();
    pool.close().await;
    let pb = indicatif::ProgressBar::hidden();
    assert_eq!(
        pucksdata::loaders::games::upsert_games(&pool, &[], &pb)
            .await
            .unwrap(),
        0
    );
    assert_eq!(pb.position(), 0);
}

#[tokio::test]
async fn test_games_batch_upsert() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    use pucksdata::{loaders::games::upsert_games, models::DbGame};

    sqlx::query(
        "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
         VALUES (99101, 'Test Home', 'Home', 'Testville', 'HME'),
                (99102, 'Test Away', 'Away', 'Testville', 'AWY')",
    )
    .execute(pool)
    .await
    .unwrap();

    let game = |game_id| DbGame {
        game_id,
        season: 20242025,
        game_date: time::macros::date!(2024 - 10 - 08),
        start_time_utc: Some("2024-10-09T00:00:00.123456Z".parse().unwrap()),
        home_team_id: 99101,
        away_team_id: 99102,
        game_type: 2,
        venue: Some("Test Arena".into()),
        venue_location: Some("Testville, TS".into()),
        game_state: Some("OFF".into()),
        home_score: Some(3),
        away_score: Some(1),
    };
    let pb = indicatif::ProgressBar::hidden();
    assert_eq!(
        upsert_games(pool, &[game(9910000001), game(9910000002)], &pb)
            .await
            .unwrap(),
        2
    );

    let mut updated = game(9910000001);
    updated.season = 20252026;
    updated.game_date = time::macros::date!(2025 - 10 - 10);
    updated.start_time_utc = Some("2025-10-11T01:02:03.654321Z".parse().unwrap());
    updated.home_team_id = 99102;
    updated.away_team_id = 99101;
    updated.game_type = 3;
    updated.venue = Some("Updated Arena".into());
    updated.venue_location = Some("Updated City".into());
    updated.game_state = Some("FINAL".into());
    updated.home_score = Some(5);
    updated.away_score = Some(4);

    let mut nullable = game(9910000002);
    nullable.start_time_utc = None;
    nullable.venue = None;
    nullable.venue_location = None;
    nullable.game_state = None;
    nullable.home_score = None;
    nullable.away_score = None;

    // Mix inserts, updates and a duplicate; the final occurrence must win.
    let records = [game(9910000001), nullable, game(9910000003), updated];
    for _ in 0..2 {
        assert_eq!(upsert_games(pool, &records, &pb).await.unwrap(), 4);
    }
    assert_eq!(pb.position(), 10);

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT * FROM games WHERE game_id BETWEEN 9910000001 AND 9910000003 ORDER BY game_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 3);
    for (row, expected) in rows.iter().zip([&records[3], &records[1], &records[2]]) {
        assert_eq!(row.get::<i64, _>("game_id"), expected.game_id);
        assert_eq!(row.get::<i32, _>("season"), expected.season);
        assert_eq!(row.get::<time::Date, _>("game_date"), expected.game_date);
        let timestamp = row.get::<Option<time::OffsetDateTime>, _>("start_time_utc");
        assert_eq!(
            timestamp.map(|t| t.unix_timestamp_nanos()),
            expected
                .start_time_utc
                .map(|t| i128::from(t.timestamp_micros()) * 1000)
        );
        assert_eq!(row.get::<i64, _>("home_team_id"), expected.home_team_id);
        assert_eq!(row.get::<i64, _>("away_team_id"), expected.away_team_id);
        assert_eq!(row.get::<i16, _>("game_type"), expected.game_type);
        assert_eq!(row.get::<Option<String>, _>("venue"), expected.venue);
        assert_eq!(
            row.get::<Option<String>, _>("venue_location"),
            expected.venue_location
        );
        assert_eq!(
            row.get::<Option<String>, _>("game_state"),
            expected.game_state
        );
        assert_eq!(row.get::<Option<i16>, _>("home_score"), expected.home_score);
        assert_eq!(row.get::<Option<i16>, _>("away_score"), expected.away_score);
    }

    // A failed batch must neither insert nor overwrite earlier records.
    let mut invalid = game(9910000005);
    invalid.home_team_id = -99101;
    let error = upsert_games(pool, &[game(9910000001), game(9910000004), invalid], &pb)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23503")
    );
    assert_eq!(pb.position(), 10);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM games WHERE game_id BETWEEN 9910000001 AND 9910000005",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count, 3);
    let venue: Option<String> =
        sqlx::query_scalar("SELECT venue FROM games WHERE game_id = 9910000001")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(venue.as_deref(), Some("Updated Arena"));

    sqlx::query("DELETE FROM games WHERE game_id BETWEEN 9910000001 AND 9910000005")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM teams WHERE team_id IN (99101, 99102)")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_fetch_idempotency() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;

    // This live test expects the 2024–25 teams to be loaded already.
    let test_season = 20242025_i32;

    use indicatif::{ProgressBar, ProgressStyle};
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );

    let games_run1 =
        pucksdata::fetchers::games::fetch_games_for_season_enriched(test_season, &pb).await;
    assert!(
        !games_run1.is_empty(),
        "expected at least one game for season {test_season}"
    );
    pucksdata::loaders::games::upsert_games(pool, &games_run1, &pb)
        .await
        .unwrap();

    let count_after_run1: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM games WHERE season = $1", test_season)
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);

    let games_run2 =
        pucksdata::fetchers::games::fetch_games_for_season_enriched(test_season, &pb).await;
    pucksdata::loaders::games::upsert_games(pool, &games_run2, &pb)
        .await
        .unwrap();

    let count_after_run2: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM games WHERE season = $1", test_season)
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap_or(0);

    assert_eq!(
        count_after_run1, count_after_run2,
        "Re-running games fetch for season {test_season} changed the row count: \
         run1={count_after_run1}, run2={count_after_run2}. Upsert semantics violated."
    );

    pb.finish_with_message(format!(
        "Idempotency verified: {count_after_run2} games for season {test_season} after 2 runs"
    ));
}
mod common;
