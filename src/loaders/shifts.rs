//! Transactional loading of typed, unnormalized shift-chart rows.

use crate::models::DbShift;

/// Atomically replace the source rows returned for one game.
pub async fn replace_game_shifts(
    pool: &sqlx::PgPool,
    game_id: i64,
    shifts: &[DbShift],
) -> Result<usize, sqlx::Error> {
    if shifts.is_empty() {
        return Err(sqlx::Error::Protocol(
            "a successful shift response cannot be empty".to_string(),
        ));
    }
    if shifts.iter().any(|shift| shift.game_id != game_id) {
        return Err(sqlx::Error::Protocol(format!(
            "shift response for game {game_id} contains a different gameId"
        )));
    }

    let mut tx = pool.begin().await?;
    let lock_name = format!("pucksdata:shifts:{game_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_name)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM shifts WHERE game_id = $1")
        .bind(game_id)
        .execute(&mut *tx)
        .await?;

    let game_ids: Vec<i64> = shifts.iter().map(|row| row.game_id).collect();
    let source_ids: Vec<i64> = shifts.iter().map(|row| row.source_shift_id).collect();
    let type_codes: Vec<i32> = shifts.iter().map(|row| row.type_code).collect();
    let player_ids: Vec<Option<i64>> = shifts.iter().map(|row| row.player_id).collect();
    let team_ids: Vec<Option<i64>> = shifts.iter().map(|row| row.team_id).collect();
    let periods: Vec<Option<i16>> = shifts.iter().map(|row| row.period).collect();
    let shift_numbers: Vec<Option<i32>> = shifts.iter().map(|row| row.shift_number).collect();
    let starts: Vec<Option<&str>> = shifts.iter().map(|row| row.start_time.as_deref()).collect();
    let ends: Vec<Option<&str>> = shifts.iter().map(|row| row.end_time.as_deref()).collect();
    let durations: Vec<Option<&str>> = shifts.iter().map(|row| row.duration.as_deref()).collect();
    let start_seconds: Vec<Option<i32>> = shifts.iter().map(|row| row.start_time_seconds).collect();
    let end_seconds: Vec<Option<i32>> = shifts.iter().map(|row| row.end_time_seconds).collect();
    let duration_seconds: Vec<Option<i32>> =
        shifts.iter().map(|row| row.duration_seconds).collect();
    let source_data: Vec<sqlx::types::Json<&serde_json::Value>> = shifts
        .iter()
        .map(|row| sqlx::types::Json(&row.source_data))
        .collect();

    let inserted = sqlx::query(
        r#"INSERT INTO shifts
               (game_id, source_shift_id, type_code, player_id, team_id,
                period, shift_number, start_time, end_time, duration,
                start_time_seconds, end_time_seconds, duration_seconds, source_data)
           SELECT * FROM UNNEST(
               $1::bigint[], $2::bigint[], $3::integer[], $4::bigint[],
               $5::bigint[], $6::smallint[], $7::integer[], $8::text[],
               $9::text[], $10::text[], $11::integer[], $12::integer[],
               $13::integer[], $14::jsonb[]
           )"#,
    )
    .bind(&game_ids)
    .bind(&source_ids)
    .bind(&type_codes)
    .bind(&player_ids)
    .bind(&team_ids)
    .bind(&periods)
    .bind(&shift_numbers)
    .bind(&starts)
    .bind(&ends)
    .bind(&durations)
    .bind(&start_seconds)
    .bind(&end_seconds)
    .bind(&duration_seconds)
    .bind(&source_data)
    .execute(&mut *tx)
    .await?
    .rows_affected() as usize;

    if inserted != shifts.len() {
        return Err(sqlx::Error::Protocol(format!(
            "inserted {inserted} of {} raw shifts for game {game_id}",
            shifts.len()
        )));
    }

    tx.commit().await?;
    Ok(inserted)
}
