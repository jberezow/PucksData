use crate::models::DbPlayer;

pub async fn upsert_players(pool: &sqlx::PgPool, records: &[DbPlayer]) -> Result<usize, sqlx::Error> {
    for record in records {
        sqlx::query!(
            r#"
            INSERT INTO players
                (player_id, first_name, last_name, position, shoots_catches,
                 current_team_abbrev, birth_date, height_cm, weight_kg,
                 draft_year, draft_round, draft_pick, draft_team_abbrev, draft_overall_pick)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
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
            record.player_id,
            record.first_name,
            record.last_name,
            record.position,
            record.shoots_catches,
            record.current_team_abbrev,
            record.birth_date,
            record.height_cm,
            record.weight_kg,
            record.draft_year,
            record.draft_round,
            record.draft_pick,
            record.draft_team_abbrev,
            record.draft_overall_pick,
        )
        .execute(pool)
        .await?;
    }
    Ok(records.len())
}
