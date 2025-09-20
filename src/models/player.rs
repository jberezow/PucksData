use crate::models::common::{deserialize_date_option, DraftDetails, NameField};
use serde::Deserialize;
use time::Date;

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
