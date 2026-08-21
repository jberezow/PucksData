//! Bulk-upserts player records to the `players` table via `UNNEST`.
use crate::models::DbPlayer;

/// Bulk-upsert player records into the `players` table using `UNNEST` arrays.
pub async fn upsert_players(
    pool: &sqlx::PgPool,
    records: &[DbPlayer],
) -> Result<usize, sqlx::Error> {
    if records.is_empty() {
        return Ok(0);
    }

    // Collect each column into a parallel Vec so we can pass them as unnest() arrays.
    // One INSERT with 4284 rows replaces 4284 individual round-trips to Neon Postgres.
    let mut player_ids: Vec<i64> = Vec::with_capacity(records.len());
    let mut first_names: Vec<String> = Vec::with_capacity(records.len());
    let mut last_names: Vec<String> = Vec::with_capacity(records.len());
    let mut positions: Vec<Option<String>> = Vec::with_capacity(records.len());
    let mut shoots_catches: Vec<Option<String>> = Vec::with_capacity(records.len());
    let mut current_team_abbrevs: Vec<Option<String>> = Vec::with_capacity(records.len());
    let mut birth_dates: Vec<Option<time::Date>> = Vec::with_capacity(records.len());
    let mut heights_cm: Vec<Option<i16>> = Vec::with_capacity(records.len());
    let mut weights_kg: Vec<Option<i16>> = Vec::with_capacity(records.len());
    let mut draft_years: Vec<Option<i16>> = Vec::with_capacity(records.len());
    let mut draft_rounds: Vec<Option<i16>> = Vec::with_capacity(records.len());
    let mut draft_picks: Vec<Option<i16>> = Vec::with_capacity(records.len());
    let mut draft_team_abbrevs: Vec<Option<String>> = Vec::with_capacity(records.len());
    let mut draft_overall_picks: Vec<Option<i16>> = Vec::with_capacity(records.len());

    for r in records {
        player_ids.push(r.player_id);
        first_names.push(r.first_name.clone());
        last_names.push(r.last_name.clone());
        positions.push(r.position.clone());
        shoots_catches.push(r.shoots_catches.clone());
        current_team_abbrevs.push(r.current_team_abbrev.clone());
        birth_dates.push(r.birth_date);
        heights_cm.push(r.height_cm);
        weights_kg.push(r.weight_kg);
        draft_years.push(r.draft_year);
        draft_rounds.push(r.draft_round);
        draft_picks.push(r.draft_pick);
        draft_team_abbrevs.push(r.draft_team_abbrev.clone());
        draft_overall_picks.push(r.draft_overall_pick);
    }

    sqlx::query!(
        r#"
        INSERT INTO players
            (player_id, first_name, last_name, position, shoots_catches,
             current_team_abbrev, birth_date, height_cm, weight_kg,
             draft_year, draft_round, draft_pick, draft_team_abbrev, draft_overall_pick)
        SELECT * FROM unnest(
            $1::bigint[],
            $2::text[],
            $3::text[],
            $4::text[],
            $5::text[],
            $6::text[],
            $7::date[],
            $8::smallint[],
            $9::smallint[],
            $10::smallint[],
            $11::smallint[],
            $12::smallint[],
            $13::text[],
            $14::smallint[]
        ) AS t(player_id, first_name, last_name, position, shoots_catches,
               current_team_abbrev, birth_date, height_cm, weight_kg,
               draft_year, draft_round, draft_pick, draft_team_abbrev, draft_overall_pick)
        ON CONFLICT (player_id) DO UPDATE SET
            first_name          = EXCLUDED.first_name,
            last_name           = EXCLUDED.last_name,
            position            = EXCLUDED.position,
            shoots_catches      = EXCLUDED.shoots_catches,
            current_team_abbrev = EXCLUDED.current_team_abbrev,
            birth_date          = EXCLUDED.birth_date,
            height_cm           = EXCLUDED.height_cm,
            weight_kg           = EXCLUDED.weight_kg,
            draft_year          = EXCLUDED.draft_year,
            draft_round         = EXCLUDED.draft_round,
            draft_pick          = EXCLUDED.draft_pick,
            draft_team_abbrev   = EXCLUDED.draft_team_abbrev,
            draft_overall_pick  = EXCLUDED.draft_overall_pick
        "#,
        &player_ids,
        &first_names,
        &last_names,
        &positions as &[Option<String>],
        &shoots_catches as &[Option<String>],
        &current_team_abbrevs as &[Option<String>],
        &birth_dates as &[Option<time::Date>],
        &heights_cm as &[Option<i16>],
        &weights_kg as &[Option<i16>],
        &draft_years as &[Option<i16>],
        &draft_rounds as &[Option<i16>],
        &draft_picks as &[Option<i16>],
        &draft_team_abbrevs as &[Option<String>],
        &draft_overall_picks as &[Option<i16>],
    )
    .execute(pool)
    .await?;

    Ok(records.len())
}
