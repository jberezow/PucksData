use crate::models::common::{deserialize_date_option, DraftDetails, NameField};
use serde::Deserialize;
use time::Date;

#[derive(Debug, Deserialize)]
pub struct PlayerBio {
    #[serde(rename = "playerId")]
    pub player_id: i32,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "firstName")]
    pub first_name: NameField,
    #[serde(rename = "lastName")]
    pub last_name: NameField,
    #[serde(rename = "sweaterNumber")]
    pub sweater_number: Option<i32>,
    #[serde(rename = "position")]
    pub position: Option<String>,
    #[serde(rename = "heightInInches")]
    pub height_in_inches: Option<i32>,
    #[serde(rename = "heightInCentimeters")]
    pub height_in_centimeters: Option<i32>,
    #[serde(rename = "weightInPounds")]
    pub weight_in_pounds: Option<i32>,
    #[serde(rename = "weightInKilograms")]
    pub weight_in_kilograms: Option<i32>,
    #[serde(deserialize_with = "deserialize_date_option")]
    pub birth_date: Option<Date>,
    #[serde(rename = "birthCity")]
    pub birth_city: Option<NameField>,
    #[serde(rename = "birthStateProvince")]
    pub birth_state_province: Option<NameField>,
    #[serde(rename = "birthCountry")]
    pub birth_country: Option<String>,
    #[serde(rename = "shootsCatches")]
    pub shoots_catches: Option<String>,
    #[serde(rename = "draftDetails")]
    pub draft_details: Option<DraftDetails>,
}
