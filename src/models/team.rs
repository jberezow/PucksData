use serde::Deserialize;
use crate::models::common::NameField;

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
    pub fn get_score(&self) -> Option<i32> {
        self.score
    }
    
    pub fn get_sog(&self) -> Option<i32> {
        self.sog
    }
} 