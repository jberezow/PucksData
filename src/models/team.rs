use serde::Deserialize;
use crate::models::common::NameField;
use crate::storage::DbPool;

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Team {
    pub id: i32,
    #[serde(rename = "abbrev")]
    pub abbrev: String,
    #[serde(rename = "commonName")]
    pub common_name: NameField,
    #[serde(rename = "placeName")]
    pub place_name: NameField,
    #[serde(rename = "placeNameWithPreposition")]
    pub place_name_with_preposition: Option<NameField>,
    #[serde(rename = "logo")]
    pub logo_light_url: Option<String>,
    #[serde(rename = "darkLogo")]
    pub logo_dark_url: Option<String>,
    pub score: Option<i32>,
    pub sog: Option<i32>, // shots on goal
}

impl Team {
    /// Upsert team data into the database
    pub async fn upsert_to_db(&self, pool: &DbPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO teams (
                team_id, abbrev, common_name, place_name, place_name_with_preposition,
                logo_light_url, logo_dark_url, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP
            )
            ON CONFLICT (team_id) DO UPDATE SET
                abbrev = EXCLUDED.abbrev,
                common_name = EXCLUDED.common_name,
                place_name = EXCLUDED.place_name,
                place_name_with_preposition = EXCLUDED.place_name_with_preposition,
                logo_light_url = EXCLUDED.logo_light_url,
                logo_dark_url = EXCLUDED.logo_dark_url,
                updated_at = CURRENT_TIMESTAMP
            "#,
            self.id,
            self.abbrev,
            self.common_name.default,
            self.place_name.default,
            self.place_name_with_preposition.as_ref().map(|p| p.default.as_str()),
            self.logo_light_url.as_deref(),
            self.logo_dark_url.as_deref(),
        )
        .execute(pool)
        .await?;
        
        Ok(())
    }

    pub fn get_score(&self) -> Option<i32> {
        self.score
    }
    
    pub fn get_sog(&self) -> Option<i32> {
        self.sog
    }
} 