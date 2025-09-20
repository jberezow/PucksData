use std::collections::HashMap;

/// Generate storage key based on endpoint name and parameters
pub fn generate_storage_key(endpoint_name: &str, params: &HashMap<String, String>) -> String {
    match endpoint_name {
        // Game endpoints
        "game_story" => {
            let game_id = params.get("game_id").unwrap();
            let season = &game_id[0..4]; // Extract year from game ID
            format!("games/{season}/{game_id}_story.json")
        }
        "game_boxscore" => {
            let game_id = params.get("game_id").unwrap();
            let season = &game_id[0..4];
            format!("games/{season}/{game_id}_boxscore.json")
        }
        "game_play_by_play" => {
            let game_id = params.get("game_id").unwrap();
            let season = &game_id[0..4];
            format!("games/{season}/{game_id}_play-by-play.json")
        }
        "game_content" => {
            let game_id = params.get("game_id").unwrap();
            let season = &game_id[0..4];
            format!("games/{season}/{game_id}_content.json")
        }
        "games_all" => "games/all_games.json".to_string(),

        // Player endpoints
        "player_summary" => {
            let player_id = params.get("player_id").unwrap();
            format!("players/individual/{player_id}_summary.json")
        }
        "player_game_log" => {
            let player_id = params.get("player_id").unwrap();
            let season = params.get("season").unwrap();
            let game_type = params.get("game_type").unwrap();
            format!("players/individual/{player_id}_game-log_{season}_{game_type}.json")
        }
        "players_all" => "players/all_players.json".to_string(),

        // Team endpoints
        "team_current_stats" => {
            let team_code = params.get("team_code").unwrap();
            format!("teams/stats/{team_code}_current-stats.json")
        }
        "team_stats_by_season" => {
            let team_code = params.get("team_code").unwrap();
            let season = params.get("season").unwrap();
            let game_type = params.get("game_type").unwrap();
            format!("teams/stats/{team_code}_stats_{season}_{game_type}.json")
        }
        "team_roster_season" => {
            let team_code = params.get("team_code").unwrap();
            let season = params.get("season").unwrap();
            format!("teams/rosters/{team_code}_roster_{season}.json")
        }
        "team_schedule_season" => {
            let team_code = params.get("team_code").unwrap();
            let season = params.get("season").unwrap();
            format!("teams/schedules/{team_code}_schedule_{season}.json")
        }
        "team_schedule_month" => {
            let team_code = params.get("team_code").unwrap();
            let date = params.get("date").unwrap();
            let year_month = &date[0..7]; // Extract YYYY-MM
            format!("teams/schedules/{team_code}_schedule_{year_month}.json")
        }
        "team_standings_date" => {
            let date = params.get("date").unwrap();
            format!("teams/standings/standings_{date}.json")
        }
        "team_standings_season" => "teams/standings/standings_season.json".to_string(),

        // Fallback for unknown endpoints
        _ => {
            let mut key = format!("unknown/{endpoint_name}");
            for (param_key, param_value) in params {
                key.push_str(&format!("_{param_key}_{param_value}"));
            }
            key.push_str(".json");
            key
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_storage_keys() {
        let mut params = HashMap::new();
        params.insert("game_id".to_string(), "2023020001".to_string());

        assert_eq!(
            generate_storage_key("game_boxscore", &params),
            "games/2023/2023020001_boxscore.json"
        );

        assert_eq!(
            generate_storage_key("game_play_by_play", &params),
            "games/2023/2023020001_play-by-play.json"
        );
    }

    #[test]
    fn test_player_storage_keys() {
        let mut params = HashMap::new();
        params.insert("player_id".to_string(), "8478402".to_string());

        assert_eq!(
            generate_storage_key("player_summary", &params),
            "players/individual/8478402_summary.json"
        );

        params.insert("season".to_string(), "20232024".to_string());
        params.insert("game_type".to_string(), "2".to_string());

        assert_eq!(
            generate_storage_key("player_game_log", &params),
            "players/individual/8478402_game-log_20232024_2.json"
        );
    }

    #[test]
    fn test_team_storage_keys() {
        let mut params = HashMap::new();
        params.insert("team_code".to_string(), "EDM".to_string());

        assert_eq!(
            generate_storage_key("team_current_stats", &params),
            "teams/stats/EDM_current-stats.json"
        );

        params.insert("season".to_string(), "20232024".to_string());

        assert_eq!(
            generate_storage_key("team_roster_season", &params),
            "teams/rosters/EDM_roster_20232024.json"
        );
    }
}
