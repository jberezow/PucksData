// LEGACY COMPATIBILITY LAYER
// This file maintains backward compatibility while encouraging migration to the new modular structure
// 
// NEW CODE SHOULD USE:
// - crate::storage::* for database operations
// - crate::models::* for data structures
// - crate::processing::* for business logic
// - crate::workflows::* for orchestration

// Re-export all new modular components for backward compatibility
pub use crate::storage::*;
pub use crate::models::*;

/// Fetch complete game data from multiple API endpoints and store in database
/// 
/// DEPRECATED: This function mixes concerns and should be refactored
/// Use crate::processing::process_structured_data_for_game instead
pub async fn fetch_complete_game_data(game_id: i64, pool: DbPool) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎮 Fetching complete game data for game {}", game_id);
    
    // Fetch from multiple endpoints to get comprehensive data
    let endpoints_to_fetch = vec![
        "game_boxscore",
        "game_story", 
        "game_content"
    ];
    
    let mut game_data: Option<Game> = None;
    
    for endpoint_name in endpoints_to_fetch {
        let params = vec![("game_id", game_id.to_string())];
        let params_refs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        
        match crate::ingest::fetch_endpoint(endpoint_name, &params_refs, pool.clone()).await {
            Ok(_) => {
                // Try to parse the stored data as Game struct
                if game_data.is_none() {
                    if let Ok(stored_data) = get_raw_data(&pool, endpoint_name, &serde_json::json!({"game_id": game_id.to_string()})).await {
                        if let Ok(parsed_game) = serde_json::from_value::<Game>(stored_data) {
                            game_data = Some(parsed_game);
                        }
                    }
                }
                println!("✅ Fetched {} data", endpoint_name);
            }
            Err(e) => {
                println!("⚠️  Failed to fetch {} data: {}", endpoint_name, e);
            }
        }
    }
    
    // If we successfully parsed game data, store teams and game in structured tables
    if let Some(game) = game_data {
        // Upsert teams first
        game.home_team.upsert_to_db(&pool).await
            .map_err(|e| format!("Failed to upsert home team: {}", e))?;
        game.away_team.upsert_to_db(&pool).await
            .map_err(|e| format!("Failed to upsert away team: {}", e))?;
        
        // Then upsert the game
        game.upsert_to_db(&pool).await
            .map_err(|e| format!("Failed to upsert game: {}", e))?;
        
        println!("✅ Successfully stored complete game data for game {}", game_id);
    } else {
        println!("⚠️  Could not parse game data for game {}", game_id);
    }
    
    Ok(())
} 