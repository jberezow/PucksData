use crate::models::{Game, PlayerBio};
use crate::storage::{DbPool, get_raw_data};
use serde_json::json;

/// Process structured data for a game (teams, game info, players)
pub async fn process_structured_data_for_game(game_id: i64, pool: &DbPool) -> Result<bool, Box<dyn std::error::Error>> {
    let params = json!({"game_id": game_id.to_string()});
    
    // Try to get boxscore data (most comprehensive for basic game info)
    if let Ok(boxscore_data) = get_raw_data(pool, "game_boxscore", &params).await {
        if let Ok(game) = serde_json::from_value::<Game>(boxscore_data) {
            // Upsert teams first
            game.home_team.upsert_to_db(pool).await?;
            game.away_team.upsert_to_db(pool).await?;
            
            // Then upsert the game
            game.upsert_to_db(pool).await?;
            
            return Ok(true);
        }
    }
    
    // If boxscore didn't work, try other endpoints
    for endpoint in &["game_story", "game_play_by_play"] {
        if let Ok(data) = get_raw_data(pool, endpoint, &params).await {
            if let Ok(game) = serde_json::from_value::<Game>(data) {
                game.home_team.upsert_to_db(pool).await?;
                game.away_team.upsert_to_db(pool).await?;
                game.upsert_to_db(pool).await?;
                return Ok(true);
            }
        }
    }
    
    // Try to extract players from player_summary if we have it
    if let Ok(player_data) = get_raw_data(pool, "player_summary", &params).await {
        if let Ok(player_bio) = serde_json::from_value::<PlayerBio>(player_data) {
            player_bio.upsert_to_db(pool).await?;
        }
    }
    
    Ok(false) // No game data found to process
}

/// Check if a game ID matches the year filter criteria
pub fn should_process_game(game_id: i64, start_year: Option<i32>) -> bool {
    if let Some(year) = start_year {
        // Extract year from game ID (format: YYYYTTGGGG where YYYY is year)
        let year_from_id = (game_id / 1000000) as i32;
        year_from_id >= year
    } else {
        true
    }
} 