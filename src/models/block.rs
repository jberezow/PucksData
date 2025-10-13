use serde::Deserialize;

/*
JSON example for a Block:
  {
    "eventId": 69,
    "periodDescriptor": {
      "number": 1,
      "periodType": "REG",
      "maxRegulationPeriods": 3
    },
    "timeInPeriod": "01:22",
    "timeRemaining": "18:38",
    "situationCode": "1451",
    "homeTeamDefendingSide": "right",
    "typeCode": 508,
    "typeDescKey": "blocked-shot",
    "sortOrder": 26,
    "details": {
      "xCoord": -72,
      "yCoord": 0,
      "zoneCode": "D",
      "blockingPlayerId": 8476468,
      "shootingPlayerId": 8479323,
      "eventOwnerTeamId": 3,
      "reason": "teammate-blocked"
    }
  },
*/

#[derive(Debug, Deserialize)]
pub struct Block {
    pub id: i32,
    #[serde(rename = "timeInPeriod")]
    pub time_in_period: String,
    #[serde(rename = "period")]
    pub period: i32,
    #[serde(rename = "blockingPlayerId")]
    pub blocking_player_id: i32,
    #[serde(rename = "shootingPlayerId")]
    pub shooting_player_id: i32,
    #[serde(rename = "eventOwnerTeamId")]
    pub blocking_player_team_id: i32,
    #[serde(rename = "xCoord")]
    pub x_coord: Option<i32>,
    #[serde(rename = "yCoord")]
    pub y_coord: Option<i32>,
    #[serde(rename = "zoneCode")]
    pub zone_code: Option<String>,
    #[serde(rename = "reason")]
    pub reason: Option<String>,
}