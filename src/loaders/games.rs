//! Upserts game records to the `games` table.
use std::collections::HashSet;

use crate::models::DbGame;

/// Convert a chrono::DateTime<Utc> to time::OffsetDateTime.
///
/// SQLx with the `time` feature maps TIMESTAMPTZ to time::OffsetDateTime at the macro level.
fn chrono_to_time(dt: chrono::DateTime<chrono::Utc>) -> time::OffsetDateTime {
    let ts = dt.timestamp();
    let nanos = dt.timestamp_subsec_nanos();
    time::OffsetDateTime::from_unix_timestamp_nanos((ts as i128) * 1_000_000_000 + nanos as i128)
        .expect("valid timestamp from chrono")
}

/// Upsert a batch of games into the games table.
///
/// Uses one `UNNEST` statement with `ON CONFLICT (game_id) DO UPDATE`.
/// The batch is atomic; duplicate game IDs use the last input record.
/// Returns the count of records processed.
///
/// FK note: home_team_id and away_team_id reference teams(team_id).
/// The caller is responsible for ensuring teams exist before loading games.
/// FK violations surface as sqlx errors and propagate to the caller.
///
/// `pb` is the upsert-phase progress bar. Call `pb.finish_and_clear()` after this
/// returns. Use `ProgressBar::hidden()` for callers that don't need a visible bar.
pub async fn upsert_games(
    pool: &sqlx::PgPool,
    records: &[DbGame],
    pb: &indicatif::ProgressBar,
) -> Result<usize, sqlx::Error> {
    if records.is_empty() {
        return Ok(0);
    }

    // PostgreSQL cannot update the same row twice in one INSERT. Preserve the
    // previous sequential loader's last-record-wins behavior for duplicate IDs.
    let mut seen = HashSet::with_capacity(records.len());
    let mut games: Vec<_> = records
        .iter()
        .rev()
        .filter(|g| seen.insert(g.game_id))
        .collect();
    games.reverse();

    let game_ids: Vec<i64> = games.iter().map(|g| g.game_id).collect();
    let seasons: Vec<i32> = games.iter().map(|g| g.season).collect();
    let dates: Vec<time::Date> = games.iter().map(|g| g.game_date).collect();
    let start_times: Vec<Option<time::OffsetDateTime>> = games
        .iter()
        .map(|g| g.start_time_utc.map(chrono_to_time))
        .collect();
    let home_team_ids: Vec<i64> = games.iter().map(|g| g.home_team_id).collect();
    let away_team_ids: Vec<i64> = games.iter().map(|g| g.away_team_id).collect();
    let game_types: Vec<i16> = games.iter().map(|g| g.game_type).collect();
    let venues: Vec<Option<String>> = games.iter().map(|g| g.venue.clone()).collect();
    let locations: Vec<Option<String>> = games.iter().map(|g| g.venue_location.clone()).collect();
    let states: Vec<Option<String>> = games.iter().map(|g| g.game_state.clone()).collect();
    let home_scores: Vec<Option<i16>> = games.iter().map(|g| g.home_score).collect();
    let away_scores: Vec<Option<i16>> = games.iter().map(|g| g.away_score).collect();

    sqlx::query!(
        r#"
            INSERT INTO games
                (game_id, season, game_date, start_time_utc, home_team_id, away_team_id,
                 game_type, venue, venue_location, game_state, home_score, away_score)
            SELECT * FROM UNNEST(
                $1::bigint[], $2::int[], $3::date[], $4::timestamptz[],
                $5::bigint[], $6::bigint[], $7::smallint[], $8::text[],
                $9::text[], $10::text[], $11::smallint[], $12::smallint[]
            )
            ON CONFLICT (game_id) DO UPDATE SET
                season         = EXCLUDED.season,
                game_date      = EXCLUDED.game_date,
                start_time_utc = EXCLUDED.start_time_utc,
                home_team_id   = EXCLUDED.home_team_id,
                away_team_id   = EXCLUDED.away_team_id,
                game_type      = EXCLUDED.game_type,
                venue          = EXCLUDED.venue,
                venue_location = EXCLUDED.venue_location,
                game_state     = EXCLUDED.game_state,
                home_score     = EXCLUDED.home_score,
                away_score     = EXCLUDED.away_score
            "#,
        &game_ids,
        &seasons,
        &dates,
        &start_times as &[Option<time::OffsetDateTime>],
        &home_team_ids,
        &away_team_ids,
        &game_types,
        &venues as &[Option<String>],
        &locations as &[Option<String>],
        &states as &[Option<String>],
        &home_scores as &[Option<i16>],
        &away_scores as &[Option<i16>],
    )
    .execute(pool)
    .await?;

    for g in records {
        pb.suspend(|| println!("{}  game {}", g.game_date, g.game_id));
        pb.inc(1);
    }
    Ok(records.len())
}
