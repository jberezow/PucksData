use serde::Deserialize;
use time::{Date, OffsetDateTime};
use crate::models::common::{NameField, PeriodDescriptor, deserialize_date_option, deserialize_datetime_option};
use crate::models::team::Team;

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


