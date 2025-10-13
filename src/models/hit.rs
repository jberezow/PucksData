use serde::Deserialize;

/* 
JSON example for a Hit:
  {
      "eventId": 1147,
      "periodDescriptor": {
        "number": 3,
        "periodType": "REG",
        "maxRegulationPeriods": 3
      },
      "timeInPeriod": "18:39",
      "timeRemaining": "01:21",
      "situationCode": "1551",
      "homeTeamDefendingSide": "right",
      "typeCode": 503,
      "typeDescKey": "hit",
      "sortOrder": 837,
      "details": {
        "xCoord": -41,
        "yCoord": 38,
        "zoneCode": "D",
        "eventOwnerTeamId": 5,
        "hittingPlayerId": 8478438,
        "hitteePlayerId": 8482073
      }
    },
*/

#[derive(Debug, Deserialize)]
pub struct Hit {
    pub id: i32,
    #[serde(rename = "timeInPeriod")]
    pub time_in_period: String,
    #[serde(rename = "period")]
    pub period: i32,
    #[serde(rename = "hittingPlayerId")]
    pub hitting_player_id: i32,
    #[serde(rename = "hitteePlayerId")]
    pub hittee_player_id: i32,
    #[serde(rename = "eventOwnerTeamId")]
    pub hitting_player_team_id: i32,
    #[serde(rename = "xCoord")]
    pub x_coord: Option<i32>,
    #[serde(rename = "yCoord")]
    pub y_coord: Option<i32>,
    #[serde(rename = "zoneCode")]
    pub zone_code: Option<String>,
}