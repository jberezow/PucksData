use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use serde::Deserialize;
use time::{Date, OffsetDateTime, PrimitiveDateTime};

pub type DbPool = Pool<Postgres>;

pub async fn create_pool() -> Result<DbPool, sqlx::Error> {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
}

pub async fn insert_raw_data(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
    data: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO raw_data (endpoint, parameters, data)
        VALUES ($1, $2, $3)
        "#
    )
    .bind(endpoint_name)
    .bind(params)
    .bind(data)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn raw_data_exists(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*) FROM raw_data
        WHERE endpoint = $1 AND parameters = $2
        "#,
    )
    .bind(endpoint_name)
    .bind(params)
    .fetch_one(pool)
    .await?;

    Ok(count.0 > 0)
}

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

#[derive(Debug, Deserialize)]
pub struct NameField {
    pub default: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct DraftDetails {
    pub year: Option<i32>,
    pub teamAbbrev: Option<String>,
    pub round: Option<i32>,
    pub pickInRound: Option<i32>,
    pub overallPick: Option<i32>,
}

// Custom deserializer for Date from "YYYY-MM-DD" string format
fn deserialize_date_option<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(date_str) => {
            Date::parse(&date_str, &time::format_description::well_known::Iso8601::DATE)
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
        None => Ok(None),
    }
}

impl PlayerBio {
    pub async fn upsert_to_db(&self, pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
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

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct Game {
    pub id: i64,
    pub season: i32,
    #[serde(rename = "gameType")]
    pub game_type: i32,
    #[serde(deserialize_with = "deserialize_date_option")]
    #[serde(rename = "gameDate")]
    pub game_date: Option<Date>,
    #[serde(deserialize_with = "deserialize_datetime_option")]
    #[serde(rename = "startTimeUTC")]
    pub start_time_utc: Option<OffsetDateTime>,
    pub venue: Option<NameField>,
    #[serde(rename = "venueLocation")]
    pub venue_location: Option<NameField>,
    #[serde(rename = "venueTimezone")]
    pub venue_timezone: Option<String>,
    #[serde(rename = "easternUTCOffset")]
    pub eastern_utc_offset: Option<String>,
    #[serde(rename = "venueUTCOffset")]
    pub venue_utc_offset: Option<String>,
    
    #[serde(rename = "homeTeam")]
    pub home_team: Team,
    #[serde(rename = "awayTeam")]
    pub away_team: Team,
    
    #[serde(rename = "gameState")]
    pub game_state: Option<String>,
    #[serde(rename = "gameScheduleState")]
    pub game_schedule_state: Option<String>,
    
    #[serde(rename = "limitedScoring")]
    pub limited_scoring: Option<bool>,
    #[serde(rename = "shootoutInUse")]
    pub shootout_in_use: Option<bool>,
    #[serde(rename = "otInUse")]
    pub ot_in_use: Option<bool>,
    #[serde(rename = "tiesInUse")]
    pub ties_in_use: Option<bool>,
    #[serde(rename = "maxPeriods")]
    pub max_periods: Option<i32>,
    #[serde(rename = "regPeriods")]
    pub reg_periods: Option<i32>,
    
    #[serde(rename = "periodDescriptor")]
    pub period_descriptor: Option<PeriodDescriptor>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PeriodDescriptor {
    pub number: i32,
    #[serde(rename = "periodType")]
    pub period_type: String,
}

// Custom deserializer for OffsetDateTime from ISO 8601 string
fn deserialize_datetime_option<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(datetime_str) => {
            OffsetDateTime::parse(&datetime_str, &time::format_description::well_known::Iso8601::DEFAULT)
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
        None => Ok(None),
    }
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
}

impl Game {
    /// Upsert game data into the database
    /// This assumes teams have already been upserted
    pub async fn upsert_to_db(&self, pool: &DbPool) -> Result<(), sqlx::Error> {
        // Convert OffsetDateTime to PrimitiveDateTime for the database
        let start_time_naive = self.start_time_utc.map(|dt| {
            PrimitiveDateTime::new(dt.date(), dt.time())
        });
        
        sqlx::query!(
            r#"
            INSERT INTO games (
                game_id, season, game_type, game_date, start_time_utc,
                venue_name, venue_location, venue_timezone, eastern_utc_offset, venue_utc_offset,
                home_team_id, away_team_id, game_state, game_schedule_state,
                home_score, away_score, home_sog, away_sog,
                limited_scoring, shootout_in_use, ot_in_use, ties_in_use,
                max_periods, reg_periods, final_period_number, final_period_type,
                updated_at
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13, $14,
                $15, $16, $17, $18,
                $19, $20, $21, $22,
                $23, $24, $25, $26,
                CURRENT_TIMESTAMP
            )
            ON CONFLICT (game_id) DO UPDATE SET
                season = EXCLUDED.season,
                game_type = EXCLUDED.game_type,
                game_date = EXCLUDED.game_date,
                start_time_utc = EXCLUDED.start_time_utc,
                venue_name = EXCLUDED.venue_name,
                venue_location = EXCLUDED.venue_location,
                venue_timezone = EXCLUDED.venue_timezone,
                eastern_utc_offset = EXCLUDED.eastern_utc_offset,
                venue_utc_offset = EXCLUDED.venue_utc_offset,
                home_team_id = EXCLUDED.home_team_id,
                away_team_id = EXCLUDED.away_team_id,
                game_state = EXCLUDED.game_state,
                game_schedule_state = EXCLUDED.game_schedule_state,
                home_score = EXCLUDED.home_score,
                away_score = EXCLUDED.away_score,
                home_sog = EXCLUDED.home_sog,
                away_sog = EXCLUDED.away_sog,
                limited_scoring = EXCLUDED.limited_scoring,
                shootout_in_use = EXCLUDED.shootout_in_use,
                ot_in_use = EXCLUDED.ot_in_use,
                ties_in_use = EXCLUDED.ties_in_use,
                max_periods = EXCLUDED.max_periods,
                reg_periods = EXCLUDED.reg_periods,
                final_period_number = EXCLUDED.final_period_number,
                final_period_type = EXCLUDED.final_period_type,
                updated_at = CURRENT_TIMESTAMP
            "#,
            self.id,
            self.season,
            self.game_type,
            self.game_date,
            start_time_naive,
            self.venue.as_ref().map(|v| v.default.as_str()),
            self.venue_location.as_ref().map(|v| v.default.as_str()),
            self.venue_timezone.as_deref(),
            self.eastern_utc_offset.as_deref(),
            self.venue_utc_offset.as_deref(),
            self.home_team.id,
            self.away_team.id,
            self.game_state.as_deref(),
            self.game_schedule_state.as_deref(),
            self.home_team.get_score(),
            self.away_team.get_score(),
            self.home_team.get_sog(),
            self.away_team.get_sog(),
            self.limited_scoring,
            self.shootout_in_use,
            self.ot_in_use,
            self.ties_in_use,
            self.max_periods,
            self.reg_periods,
            self.period_descriptor.as_ref().map(|p| p.number),
            self.period_descriptor.as_ref().map(|p| p.period_type.as_str()),
        )
        .execute(pool)
        .await?;
        
        Ok(())
    }
}

// Extension trait to handle score/sog fields that may be in different structs
impl Team {
    fn get_score(&self) -> Option<i32> {
        self.score
    }
    
    fn get_sog(&self) -> Option<i32> {
        self.sog
    }
}

/// Fetch complete game data from multiple API endpoints and store in database
pub async fn fetch_complete_game_data(game_id: i64, pool: DbPool) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎮 Fetching complete game data for game {}", game_id);
    
    // Fetch from multiple endpoints to get comprehensive data
    let endpoints_to_fetch = vec![
        "game_boxscore",
        "game_story", 
        "game_content"
    ];
    
    let mut game_data: Option<Game> = None;
    
    for endpoint_name in endpoints_to_fetch {
        let params = vec![("game_id", game_id.to_string())];
        let params_refs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        
        match crate::ingest::fetch_endpoint(endpoint_name, &params_refs, pool.clone()).await {
            Ok(_) => {
                // Try to parse the stored data as Game struct
                if game_data.is_none() {
                    if let Ok(stored_data) = get_raw_data(&pool, endpoint_name, &serde_json::json!({"game_id": game_id.to_string()})).await {
                        if let Ok(parsed_game) = serde_json::from_value::<Game>(stored_data) {
                            game_data = Some(parsed_game);
                        }
                    }
                }
                println!("✅ Fetched {} data", endpoint_name);
            }
            Err(e) => {
                println!("⚠️  Failed to fetch {} data: {}", endpoint_name, e);
            }
        }
    }
    
    // If we successfully parsed game data, store teams and game in structured tables
    if let Some(game) = game_data {
        // Upsert teams first
        game.home_team.upsert_to_db(&pool).await
            .map_err(|e| format!("Failed to upsert home team: {}", e))?;
        game.away_team.upsert_to_db(&pool).await
            .map_err(|e| format!("Failed to upsert away team: {}", e))?;
        
        // Then upsert the game
        game.upsert_to_db(&pool).await
            .map_err(|e| format!("Failed to upsert game: {}", e))?;
        
        println!("✅ Successfully stored complete game data for game {}", game_id);
    } else {
        println!("⚠️  Could not parse game data for game {}", game_id);
    }
    
    Ok(())
}

/// Get raw data from the database
pub async fn get_raw_data(
    pool: &DbPool,
    endpoint_name: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT data FROM raw_data WHERE endpoint = $1 AND parameters = $2 LIMIT 1",
        endpoint_name,
        params
    )
    .fetch_one(pool)
    .await?;
    
    Ok(row.data)
}

/// Get games by team for a season
pub async fn get_games_by_team(team_id: i32, season: Option<i32>, pool: &DbPool) -> Result<Vec<i64>, sqlx::Error> {
    if let Some(s) = season {
        let rows = sqlx::query!(
            "SELECT game_id FROM games WHERE (home_team_id = $1 OR away_team_id = $1) AND season = $2 ORDER BY game_date",
            team_id, s
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.game_id).collect())
    } else {
        let rows = sqlx::query!(
            "SELECT game_id FROM games WHERE (home_team_id = $1 OR away_team_id = $1) ORDER BY game_date",
            team_id
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.game_id).collect())
    }
}

/// Get team by ID
pub async fn get_team_by_id(team_id: i32, pool: &DbPool) -> Result<Option<(String, String)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT abbrev, common_name FROM teams WHERE team_id = $1",
        team_id
    )
    .fetch_optional(pool)
    .await?;
    
    Ok(row.map(|r| (r.abbrev, r.common_name)))
} 