use serde::Deserialize;
use time::{Date, OffsetDateTime, PrimitiveDateTime};
use crate::models::common::{NameField, PeriodDescriptor, deserialize_date_option, deserialize_datetime_option};
use crate::models::team::Team;
use crate::storage::DbPool;

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
