use serde::Deserialize;
/*
  JSON example for a Shot:
  {
    "eventId": 84,
    "periodDescriptor": {
      "number": 1,
      "periodType": "REG",
      "maxRegulationPeriods": 3
    },
    "timeInPeriod": "02:44",
    "timeRemaining": "17:16",
    "situationCode": "1451",
    "homeTeamDefendingSide": "right",
    "typeCode": 506,
    "typeDescKey": "shot-on-goal",
    "sortOrder": 41,
    "details": {
      "xCoord": -59,
      "yCoord": -19,
      "zoneCode": "O",
      "shotType": "snap",
      "shootingPlayerId": 8482109,
      "goalieInNetId": 8481668,
      "eventOwnerTeamId": 3,
      "awaySOG": 2,
      "homeSOG": 1
    }
  },
*/
#[derive(Debug, Deserialize)]
pub struct Shot {
    pub id: i32,
    #[serde(rename = "timeInPeriod")]
    pub time_in_period: String,
    #[serde(rename = "period")]
    pub period: i32,
    #[serde(rename = "shootingPlayerId")]
    pub shooting_player_id: i32,
    #[serde(rename = "goalieInNetId")]
    pub goalie_in_net_id: i32,
    #[serde(rename = "eventOwnerTeamId")]
    pub shooting_player_team_id: i32,
    #[serde(rename = "xCoord")]
    pub x_coord: Option<i32>,
    #[serde(rename = "yCoord")]
    pub y_coord: Option<i32>,
    #[serde(rename = "zoneCode")]
    pub zone_code: Option<String>,
    #[serde(rename = "shotType")]
    pub shot_type: Option<String>,
}
