use serde::Deserialize;
use time::Date;
use sqlx::PgPool;
use crate::models::common::{NameField, DraftDetails, deserialize_date_option};

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PlayerBio {
    pub playerId: i32,
    pub isActive: bool,
    pub currentTeamId: Option<i32>,
    pub currentTeamAbbrev: Option<String>,
    pub firstName: NameField,
    pub lastName: NameField,
    pub sweaterNumber: Option<i32>,
    pub position: Option<String>,
    pub heightInInches: Option<i32>,
    pub heightInCentimeters: Option<i32>,
    pub weightInPounds: Option<i32>,
    pub weightInKilograms: Option<i32>,
    #[serde(deserialize_with = "deserialize_date_option")]
    pub birthDate: Option<Date>,
    pub birthCity: Option<NameField>,
    pub birthStateProvince: Option<NameField>,
    pub birthCountry: Option<String>,
    pub shootsCatches: Option<String>,
    pub draftDetails: Option<DraftDetails>,
}

impl PlayerBio {
    pub async fn upsert_to_db(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO players (
                player_id, first_name, last_name, is_active, current_team_id, current_team_abbrev,
                sweater_number, position, height_in_inches, height_in_centimeters, weight_in_pounds,
                weight_in_kilograms, birth_date, birth_city, birth_state_province, birth_country,
                shoots_catches, draft_year, draft_team_abbrev, draft_round, draft_pick_in_round, draft_overall_pick
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14, $15, $16,
                $17, $18, $19, $20, $21, $22
            )
            ON CONFLICT (player_id) DO UPDATE SET
                first_name = EXCLUDED.first_name,
                last_name = EXCLUDED.last_name,
                is_active = EXCLUDED.is_active,
                current_team_id = EXCLUDED.current_team_id,
                current_team_abbrev = EXCLUDED.current_team_abbrev,
                sweater_number = EXCLUDED.sweater_number,
                position = EXCLUDED.position,
                height_in_inches = EXCLUDED.height_in_inches,
                height_in_centimeters = EXCLUDED.height_in_centimeters,
                weight_in_pounds = EXCLUDED.weight_in_pounds,
                weight_in_kilograms = EXCLUDED.weight_in_kilograms,
                birth_date = EXCLUDED.birth_date,
                birth_city = EXCLUDED.birth_city,
                birth_state_province = EXCLUDED.birth_state_province,
                birth_country = EXCLUDED.birth_country,
                shoots_catches = EXCLUDED.shoots_catches,
                draft_year = EXCLUDED.draft_year,
                draft_team_abbrev = EXCLUDED.draft_team_abbrev,
                draft_round = EXCLUDED.draft_round,
                draft_pick_in_round = EXCLUDED.draft_pick_in_round,
                draft_overall_pick = EXCLUDED.draft_overall_pick
            "#,
            self.playerId,
            self.firstName.default.as_str(),
            self.lastName.default.as_str(),
            self.isActive,
            self.currentTeamId,
            self.currentTeamAbbrev.as_deref(),
            self.sweaterNumber,
            self.position.as_deref(),
            self.heightInInches,
            self.heightInCentimeters,
            self.weightInPounds,
            self.weightInKilograms,
            self.birthDate,
            self.birthCity.as_ref().map(|c| c.default.as_str()),
            self.birthStateProvince.as_ref().map(|s| s.default.as_str()),
            self.birthCountry.as_deref(),
            self.shootsCatches.as_deref(),
            self.draftDetails.as_ref().and_then(|d| d.year),
            self.draftDetails.as_ref().and_then(|d| d.teamAbbrev.as_deref()),
            self.draftDetails.as_ref().and_then(|d| d.round),
            self.draftDetails.as_ref().and_then(|d| d.pickInRound),
            self.draftDetails.as_ref().and_then(|d| d.overallPick),
        )
        .execute(pool)
        .await?;
        Ok(())
    }
} 