mod common;

use pucksdata::models::DbShift;

const GAME_ID: i64 = 1900020099;
const HOME_TEAM_ID: i64 = 99_990;
const AWAY_TEAM_ID: i64 = 99_991;

async fn prepare_game(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM games WHERE game_id = $1")
        .bind(GAME_ID)
        .execute(pool)
        .await
        .unwrap();
    for (team_id, abbrev) in [(HOME_TEAM_ID, "SHT"), (AWAY_TEAM_ID, "SVA")] {
        sqlx::query(
            "INSERT INTO teams (team_id, full_name, common_name, place_name, abbrev)
             VALUES ($1, $2, $2, 'Test', $3)
             ON CONFLICT (team_id) DO NOTHING",
        )
        .bind(team_id)
        .bind(format!("Shift Test {team_id}"))
        .bind(abbrev)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO games
             (game_id, season, game_date, home_team_id, away_team_id, game_type, game_state)
         VALUES ($1, 20252026, '2025-10-07', $2, $3, 2, 'OFF')",
    )
    .bind(GAME_ID)
    .bind(HOME_TEAM_ID)
    .bind(AWAY_TEAM_ID)
    .execute(pool)
    .await
    .unwrap();
}

fn shift(source_shift_id: i64, duration: &str) -> DbShift {
    DbShift {
        source_shift_id,
        game_id: GAME_ID,
        type_code: 517,
        player_id: Some(8_470_001),
        team_id: Some(13),
        period: Some(1),
        shift_number: Some(1),
        start_time: Some("00:40".to_string()),
        end_time: Some("00:20".to_string()),
        duration: Some(duration.to_string()),
        start_time_seconds: Some(40),
        end_time_seconds: Some(20),
        duration_seconds: None,
        event_number: Some(7),
        detail_code: Some(0),
        event_description: Some("source description".to_string()),
        event_details: Some("source details".to_string()),
    }
}

#[tokio::test]
async fn typed_shift_snapshot_replacement_preserves_source_fields_and_rolls_back_on_failure() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    prepare_game(pool).await;

    // Distinct source IDs must survive even when every other field is identical.
    let first = vec![shift(1, "not-a-clock"), shift(2, "not-a-clock")];
    assert_eq!(
        pucksdata::loaders::shifts::replace_game_shifts(pool, GAME_ID, &first)
            .await
            .unwrap(),
        2
    );

    let stored_details: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT source_shift_id, event_details FROM shifts WHERE game_id = $1 ORDER BY source_shift_id",
    )
    .bind(GAME_ID)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        stored_details,
        vec![
            (1, first[0].event_details.clone()),
            (2, first[1].event_details.clone())
        ]
    );

    // This passes input checks and fails after DELETE, exercising actual rollback.
    let invalid = vec![shift(3, "00:20"), shift(3, "00:30")];
    let error = pucksdata::loaders::shifts::replace_game_shifts(pool, GAME_ID, &invalid)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    let unchanged: Vec<(i64, String)> = sqlx::query_as(
        "SELECT source_shift_id, duration FROM shifts WHERE game_id = $1 ORDER BY source_shift_id",
    )
    .bind(GAME_ID)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        unchanged,
        vec![
            (1, "not-a-clock".to_string()),
            (2, "not-a-clock".to_string())
        ]
    );

    let mut corrected = vec![shift(3, "still-raw")];
    corrected[0].event_details = None;
    pucksdata::loaders::shifts::replace_game_shifts(pool, GAME_ID, &corrected)
        .await
        .unwrap();

    #[derive(sqlx::FromRow)]
    struct StoredShift {
        source_shift_id: i64,
        duration: String,
        duration_seconds: Option<i32>,
        event_number: Option<i32>,
        detail_code: Option<i32>,
        event_description: Option<String>,
        event_details: Option<String>,
    }

    let stored: StoredShift = sqlx::query_as(
        "SELECT source_shift_id, duration, duration_seconds, event_number, detail_code,
                event_description, event_details
         FROM shifts WHERE game_id = $1",
    )
    .bind(GAME_ID)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(stored.source_shift_id, 3);
    assert_eq!(stored.duration, "still-raw");
    assert_eq!(stored.duration_seconds, None);
    assert_eq!(stored.event_number, Some(7));
    assert_eq!(stored.detail_code, Some(0));
    assert_eq!(stored.event_description, corrected[0].event_description);
    assert_eq!(stored.event_details, None);

    // An empty refresh records the latest attempt without deleting the snapshot.
    pucksdata::loaders::shifts::record_unsuccessful_attempt(pool, GAME_ID, true)
        .await
        .unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM shift_fetch_status WHERE game_id = $1")
            .bind(GAME_ID)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(status, "unavailable");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shifts WHERE game_id = $1")
        .bind(GAME_ID)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    pucksdata::loaders::shifts::replace_game_shifts(pool, GAME_ID, &corrected)
        .await
        .unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM shift_fetch_status WHERE game_id = $1")
            .bind(GAME_ID)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(status, "loaded");
}
