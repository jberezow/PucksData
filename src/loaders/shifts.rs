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
    super::history::lock_game(&mut tx, game_id).await?;

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
    let event_numbers: Vec<Option<i32>> = shifts.iter().map(|row| row.event_number).collect();
    let detail_codes: Vec<Option<i32>> = shifts.iter().map(|row| row.detail_code).collect();
    let event_descriptions: Vec<Option<&str>> = shifts
        .iter()
        .map(|row| row.event_description.as_deref())
        .collect();
    let event_details: Vec<Option<&str>> = shifts
        .iter()
        .map(|row| row.event_details.as_deref())
        .collect();

    let inserted = sqlx::query(
        r#"INSERT INTO shifts
               (game_id, source_shift_id, type_code, player_id, team_id,
                period, shift_number, start_time, end_time, duration,
                start_time_seconds, end_time_seconds, duration_seconds,
                event_number, detail_code, event_description, event_details)
           SELECT * FROM UNNEST(
               $1::bigint[], $2::bigint[], $3::integer[], $4::bigint[],
               $5::bigint[], $6::smallint[], $7::integer[], $8::text[],
               $9::text[], $10::text[], $11::integer[], $12::integer[],
               $13::integer[], $14::integer[], $15::integer[], $16::text[], $17::text[]
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
    .bind(&event_numbers)
    .bind(&detail_codes)
    .bind(&event_descriptions)
    .bind(&event_details)
    .execute(&mut *tx)
    .await?
    .rows_affected() as usize;

    if inserted != shifts.len() {
        return Err(sqlx::Error::Protocol(format!(
            "inserted {inserted} of {} raw shifts for game {game_id}",
            shifts.len()
        )));
    }

    sqlx::query(
        "INSERT INTO shift_fetch_status (game_id, status) VALUES ($1, 'loaded')
         ON CONFLICT (game_id) DO UPDATE SET status = 'loaded', attempted_at = NOW()",
    )
    .bind(game_id)
    .execute(&mut *tx)
    .await?;
    super::history::shifts(&mut tx, game_id).await?;
    tx.commit().await?;
    Ok(inserted)
}

/// Record a failed/empty attempt without touching a previously stored snapshot.
pub async fn record_unsuccessful_attempt(
    pool: &sqlx::PgPool,
    game_id: i64,
    unavailable: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO shift_fetch_status (game_id, status) VALUES ($1, $2)
         ON CONFLICT (game_id) DO UPDATE SET status = EXCLUDED.status, attempted_at = NOW()",
    )
    .bind(game_id)
    .bind(if unavailable { "unavailable" } else { "failed" })
    .execute(pool)
    .await?;
    Ok(())
}
