use serde::Deserialize;
use time::{Date, OffsetDateTime};

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

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct PeriodDescriptor {
    pub number: i32,
    #[serde(rename = "periodType")]
    pub period_type: String,
}

// Custom deserializer for Date from "YYYY-MM-DD" string format
pub fn deserialize_date_option<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
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

// Custom deserializer for OffsetDateTime from ISO 8601 string
pub fn deserialize_datetime_option<'de, D>(deserializer: D) -> Result<Option<OffsetDateTime>, D::Error>
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