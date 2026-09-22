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
    let source_data = serde_json::json!({
        "id": source_shift_id,
        "typeCode": 517,
        "gameId": GAME_ID,
        "playerId": 8_470_001,
        "teamId": 13,
        "period": 1,
        "shiftNumber": source_shift_id,
        "startTime": "00:40",
        "endTime": "00:20",
        "duration": duration,
        "unmodeledSourceField": true
    });
    DbShift {
        source_shift_id,
        game_id: GAME_ID,
        type_code: 517,
        player_id: Some(8_470_001),
        team_id: Some(13),
        period: Some(1),
        shift_number: Some(source_shift_id as i32),
        start_time: Some("00:40".to_string()),
        end_time: Some("00:20".to_string()),
        duration: Some(duration.to_string()),
        start_time_seconds: Some(40),
        end_time_seconds: Some(20),
        duration_seconds: None,
        source_data,
    }
}

#[tokio::test]
async fn raw_shift_snapshot_replacement_is_atomic_and_lossless() {
    if !common::test_database_configured() {
        return;
    }
    let pool = common::test_pool().await;
    prepare_game(pool).await;

    let first = vec![shift(1, "not-a-clock"), shift(2, "00:20")];
    assert_eq!(
        pucksdata::loaders::shifts::replace_game_shifts(pool, GAME_ID, &first)
            .await
            .unwrap(),
        2
    );

    let corrected = vec![shift(3, "still-raw")];
    pucksdata::loaders::shifts::replace_game_shifts(pool, GAME_ID, &corrected)
        .await
        .unwrap();

    let stored: (i64, String, serde_json::Value) = sqlx::query_as(
        "SELECT source_shift_id, duration, source_data
         FROM shifts WHERE game_id = $1",
    )
    .bind(GAME_ID)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(stored.0, 3);
    assert_eq!(stored.1, "still-raw");
    assert_eq!(stored.2, corrected[0].source_data);
}
