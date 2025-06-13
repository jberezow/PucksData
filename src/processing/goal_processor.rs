use crate::storage::DbPool;
use serde_json::json;
use sqlx::postgres::types::PgInterval;
use sqlx::Row;
use std::error::Error;
use std::collections::HashSet;

/// Convert time string (MM:SS) to PgInterval
fn parse_time_to_interval(time_str: &str) -> Result<PgInterval, Box<dyn Error>> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return Err("Invalid time format, expected MM:SS".into());
    }
    
    let minutes: i32 = parts[0].parse()?;
    let seconds: i32 = parts[1].parse()?;
    
    let total_seconds = minutes * 60 + seconds;
    
    Ok(PgInterval {
        months: 0,
        days: 0,
        microseconds: total_seconds as i64 * 1_000_000,
    })
}

/// Fetch and store missing player data
async fn fetch_missing_players(pool: &DbPool, player_ids: &HashSet<i32>) -> Result<(), Box<dyn Error>> {
    if player_ids.is_empty() {
        return Ok(());
    }

    println!("👥 Checking for missing players...");
    
    // Find which players are missing from the database
    let mut missing_players = Vec::new();
    for &player_id in player_ids {
        let exists = sqlx::query!(
            "SELECT player_id FROM players WHERE player_id = $1",
            player_id
        )
        .fetch_optional(pool)
        .await?
        .is_some();
        
        if !exists {
            missing_players.push(player_id);
        }
    }
    
    if missing_players.is_empty() {
        println!("✅ All players already exist in database");
        return Ok(());
    }
    
    println!("🔍 Found {} missing players, fetching their data...", missing_players.len());
    
    // Fetch each missing player
    for player_id in missing_players {
        match fetch_and_store_player(pool, player_id).await {
            Ok(()) => {
                println!("✅ Fetched player {}", player_id);
            }
            Err(e) => {
                eprintln!("⚠️  Failed to fetch player {}: {}", player_id, e);
            }
        }
    }
    
    Ok(())
}

/// Fetch and store a single player's data
async fn fetch_and_store_player(pool: &DbPool, player_id: i32) -> Result<(), Box<dyn Error>> {
    // Use the existing ingest functionality to fetch player data
    let params = vec![("player_id", player_id.to_string())];
    let params_refs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    
    // Fetch player summary which contains bio information
    match crate::ingest::fetch_endpoint("player_summary", &params_refs, pool.clone()).await {
        Ok(_) => {
            // Try to parse and store the player data
            let raw_data = crate::storage::get_raw_data(
                pool, 
                "player_summary", 
                &json!({"player_id": player_id.to_string()})
            ).await?;
            
            // Parse player bio from the summary data
            if let Ok(player_bio) = parse_player_bio_from_summary(&raw_data) {
                player_bio.upsert_to_db(pool).await?;
                Ok(())
            } else {
                Err(format!("Failed to parse player bio for player {}", player_id).into())
            }
        }
        Err(e) => Err(format!("Failed to fetch player summary for {}: {}", player_id, e).into())
    }
}

/// Parse player bio from player summary data
fn parse_player_bio_from_summary(summary_data: &serde_json::Value) -> Result<crate::models::PlayerBio, Box<dyn Error>> {
    // The player summary endpoint returns data in a specific structure
    // We need to extract the player bio information
    if let Some(player_data) = summary_data.get("people").and_then(|p| p.get(0)) {
        let player_bio: crate::models::PlayerBio = serde_json::from_value(player_data.clone())?;
        Ok(player_bio)
    } else {
        Err("Invalid player summary data structure".into())
    }
}

/// Validate that the scoring and defending teams match the game's teams
async fn validate_goal_teams(
    pool: &DbPool,
    game_id: i64,
    scoring_team_id: i64,
    defending_team_id: i64,
) -> Result<(), Box<dyn Error>> {
    // Get the home and away team IDs for this game
    let game_teams = sqlx::query!(
        "SELECT home_team_id, away_team_id FROM games WHERE game_id = $1",
        game_id
    )
    .fetch_optional(pool)
    .await?;
    
    let (home_team_id, away_team_id) = match game_teams {
        Some(teams) => (teams.home_team_id, teams.away_team_id),
        None => return Err(format!("Game {} not found in database", game_id).into()),
    };
    
    // Validate scoring team is either home or away
    if scoring_team_id != home_team_id as i64 && scoring_team_id != away_team_id as i64 {
        return Err(format!(
            "Scoring team {} is not a participant in game {} (home: {}, away: {})",
            scoring_team_id, game_id, home_team_id, away_team_id
        ).into());
    }
    
    // Validate defending team is the opposite of scoring team
    let expected_defending_team = if scoring_team_id == home_team_id as i64 {
        away_team_id as i64
    } else {
        home_team_id as i64
    };
    
    if defending_team_id != expected_defending_team {
        return Err(format!(
            "Defending team {} does not match expected opposing team {} for game {}",
            defending_team_id, expected_defending_team, game_id
        ).into());
    }
    
    Ok(())
}

/// Process goals from raw game data and store in the goals table
pub async fn process_goals_for_game(game_id: i64, pool: &DbPool) -> Result<usize, Box<dyn Error>> {
    let params = json!({"game_id": game_id.to_string()});
    
    // Get play-by-play data which contains goal events
    let play_by_play_data = match crate::storage::get_raw_data(pool, "game_play_by_play", &params).await {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to get play-by-play data: {}", e).into()),
    };
    
    // Extract game info
    let game_info = play_by_play_data.as_object()
        .ok_or("Invalid play-by-play data format")?;
    
    let home_team_id = game_info["homeTeam"]["id"].as_i64()
        .ok_or("Missing home team ID")?;
    let away_team_id = game_info["awayTeam"]["id"].as_i64()
        .ok_or("Missing away team ID")?;
    
    // Get plays array
    let plays = game_info["plays"].as_array()
        .ok_or("Missing plays array")?;
    
    // First pass: collect all player IDs from goal events
    let mut player_ids = HashSet::new();
    for play in plays {
        if play["typeCode"].as_i64() == Some(505) {
            if let Some(details) = play["details"].as_object() {
                if let Some(scorer_id) = details["scoringPlayerId"].as_i64() {
                    player_ids.insert(scorer_id as i32);
                }
                if let Some(assist1_id) = details.get("assist1PlayerId").and_then(|v| v.as_i64()) {
                    player_ids.insert(assist1_id as i32);
                }
                if let Some(assist2_id) = details.get("assist2PlayerId").and_then(|v| v.as_i64()) {
                    player_ids.insert(assist2_id as i32);
                }
                if let Some(goalie_id) = details.get("goalieInNetId").and_then(|v| v.as_i64()) {
                    player_ids.insert(goalie_id as i32);
                }
            }
        }
    }
    
    // Fetch missing players before processing goals
    fetch_missing_players(pool, &player_ids).await?;
    
    let mut goals_processed = 0;
    
    // Second pass: process each goal
    for play in plays {
        // Look for goal events (typeCode 505)
        if play["typeCode"].as_i64() == Some(505) {
            let details = play["details"].as_object()
                .ok_or("Missing goal details")?;
            
            // Extract goal information
            let period = play["periodDescriptor"]["number"].as_i64()
                .ok_or("Missing period number")?;
            let period_type = play["periodDescriptor"]["periodType"].as_str()
                .ok_or("Missing period type")?;
            let time_in_period_str = play["timeInPeriod"].as_str()
                .ok_or("Missing time in period")?;
            let time_in_period = parse_time_to_interval(time_in_period_str)?;
            let situation_code = play["situationCode"].as_str()
                .unwrap_or("1551"); // Default to even strength if missing
            
            // Extract team and player IDs
            let scoring_team_id = details["eventOwnerTeamId"].as_i64()
                .ok_or("Missing scoring team ID")?;
            let defending_team_id = if scoring_team_id == home_team_id {
                away_team_id
            } else {
                home_team_id
            };
            
            // Validate teams match the game
            validate_goal_teams(pool, game_id, scoring_team_id, defending_team_id).await?;
            
            let scorer_id = details["scoringPlayerId"].as_i64()
                .ok_or("Missing scorer ID")?;
            let primary_assist_id = details.get("assist1PlayerId").and_then(|v| v.as_i64());
            let secondary_assist_id = details.get("assist2PlayerId").and_then(|v| v.as_i64());
            let goalie_id = details.get("goalieInNetId").and_then(|v| v.as_i64());
            
            // Extract shot details
            let shot_type = details.get("shotType").and_then(|v| v.as_str());
            let x_coord = details.get("xCoord").and_then(|v| v.as_i64());
            let y_coord = details.get("yCoord").and_then(|v| v.as_i64());
            let zone_code = details.get("zoneCode").and_then(|v| v.as_str());
            
            // Determine strength
            let strength = match situation_code {
                "1551" => "EV",
                "1552" => "PP",
                "1553" => "SH",
                _ => "EV", // Default to even strength if unknown
            };
            
            // Determine if empty net
            let empty_net = details.get("emptyNet").and_then(|v| v.as_bool()).unwrap_or(false);
            
            // Check if goal already exists before inserting
            let exists = sqlx::query!(
                "SELECT id FROM events.goals WHERE game_id = $1 AND period = $2 AND time_in_period = $3 AND scorer_id = $4",
                game_id,
                period as i32,
                time_in_period,
                scorer_id as i32
            )
            .fetch_optional(pool)
            .await?
            .is_some();
            
            if !exists {
                // Check if this is a shootout goal
                let time_str = play["timeInPeriod"].as_str().unwrap_or("00:00");
                let is_shootout = is_shootout_goal(period as i32, time_str, Some(period_type));
                
                // Insert goal into database
                sqlx::query!(
                    r#"
                    INSERT INTO events.goals (
                        game_id, period, period_type, time_in_period, situation_code,
                        scoring_team_id, defending_team_id, scorer_id, primary_assist_id,
                        secondary_assist_id, goalie_id, strength, shot_type, x_coord,
                        y_coord, zone_code, empty_net, is_valid, called_back_reason
                    ) VALUES (
                        $1, $2, $3, $4, $5,
                        $6, $7, $8, $9,
                        $10, $11, $12, $13, $14,
                        $15, $16, $17, $18, $19
                    )
                    "#,
                    game_id,
                    period as i32,
                    period_type,
                    time_in_period,
                    situation_code,
                    scoring_team_id as i32,
                    defending_team_id as i32,
                    scorer_id as i32,
                    primary_assist_id.map(|id| id as i32),
                    secondary_assist_id.map(|id| id as i32),
                    goalie_id.map(|id| id as i32),
                    strength,
                    shot_type,
                    x_coord.map(|x| x as i32),
                    y_coord.map(|y| y as i32),
                    zone_code,
                    empty_net,
                    !is_shootout, // is_valid = true unless it's a shootout
                    if is_shootout { Some("shootout goal") } else { None }
                )
                .execute(pool)
                .await?;
                
                goals_processed += 1;
            }
        }
    }
    
    Ok(goals_processed)
}

/// Find games that have play-by-play data but no goals processed
pub async fn find_missing_goals(pool: &DbPool) -> Result<Vec<i64>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        WITH games_with_goals AS (
            SELECT DISTINCT game_id
            FROM events.goals
        ),
        games_with_pbp AS (
            SELECT DISTINCT (parameters->>'game_id')::bigint as game_id
            FROM raw_data
            WHERE endpoint = 'game_play_by_play'
            AND parameters->>'game_id' IS NOT NULL
        )
        SELECT g.game_id
        FROM games_with_pbp g
        LEFT JOIN games_with_goals wg ON wg.game_id = g.game_id
        WHERE wg.game_id IS NULL
        ORDER BY g.game_id
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter()
        .filter_map(|row| row.game_id)
        .collect())
}

/// Process missing goals in batches
pub async fn process_missing_goals(
    pool: &DbPool,
    batch_size: usize,
    dry_run: bool,
) -> Result<(usize, usize, usize), Box<dyn Error>> {
    println!("🔍 Finding games that need goal processing...");
    let missing_games = find_missing_goals(pool).await?;
    
    if missing_games.is_empty() {
        println!("✨ All goals are already processed!");
        return Ok((0, 0, 0));
    }

    println!("📊 Found {} games that need goal processing", missing_games.len());
    
    if dry_run {
        println!("🔍 DRY RUN - would process goals for games: {:?}", missing_games);
        return Ok((missing_games.len(), 0, 0));
    }

    let mut success_count = 0;
    let mut error_count = 0;
    let mut total_goals = 0;

    // Process in batches
    for chunk in missing_games.chunks(batch_size) {
        for &game_id in chunk {
            match process_goals_for_game(game_id, pool).await {
                Ok(goals) => {
                    success_count += 1;
                    total_goals += goals;
                    println!("✅ Processed {} goals for game {}", goals, game_id);
                }
                Err(e) => {
                    error_count += 1;
                    eprintln!("❌ Error processing goals for game {}: {}", game_id, e);
                }
            }
        }
    }
    
    println!("\n📊 Goal processing complete!");
    println!("   ✅ Games processed successfully: {}", success_count);
    println!("   ❌ Games with errors: {}", error_count);
    println!("   🏒 Total goals processed: {}", total_goals);
    
    Ok((success_count, error_count, total_goals))
}

/// Analyze play-by-play data to find evidence of called back goals
pub async fn analyze_goal_reviews(pool: &DbPool, game_id: i64) -> Result<(), Box<dyn Error>> {
    let params = json!({"game_id": game_id.to_string()});
    
    // Get play-by-play data
    let play_by_play_data = match crate::storage::get_raw_data(pool, "game_play_by_play", &params).await {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to get play-by-play data: {}", e).into()),
    };
    
    // Extract game info
    let game_info = play_by_play_data.as_object()
        .ok_or("Invalid play-by-play data format")?;
    
    // Get plays array
    let plays = game_info["plays"].as_array()
        .ok_or("Missing plays array")?;
    
    println!("🏒 Analyzing game {} for goal reviews/call backs...", game_id);
    
    let mut goal_count = 0;
    let mut review_events = Vec::new();
    let mut all_event_types = std::collections::HashSet::new();
    
    // Process each play to look for patterns
    for (index, play) in plays.iter().enumerate() {
        let type_code = play["typeCode"].as_i64();
        let type_desc = play["typeDescKey"].as_str().unwrap_or("unknown");
        
        all_event_types.insert(format!("{}: {}", type_code.unwrap_or(-1), type_desc));
        
        // Look for goal events (typeCode 505)
        if type_code == Some(505) {
            goal_count += 1;
            let scorer_name = play["details"]
                .get("scoringPlayerName")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let period = play["periodDescriptor"]["number"].as_i64().unwrap_or(0);
            let time = play["timeInPeriod"].as_str().unwrap_or("Unknown");
            
            println!("  ⚽ Goal #{}: {} - Period {} at {}", goal_count, scorer_name, period, time);
            
            // Check the next few plays for review-related events
            for next_idx in (index + 1)..std::cmp::min(index + 10, plays.len()) {
                let next_play = &plays[next_idx];
                let next_type_desc = next_play["typeDescKey"].as_str().unwrap_or("unknown");
                let next_type_code = next_play["typeCode"].as_i64();
                
                if next_type_desc.to_lowercase().contains("review") || 
                   next_type_desc.to_lowercase().contains("challenge") ||
                   next_type_desc.to_lowercase().contains("video") ||
                   next_type_desc.to_lowercase().contains("call") {
                    review_events.push(format!("    📹 Review event after goal: {} (code: {})", 
                                               next_type_desc, next_type_code.unwrap_or(-1)));
                }
            }
        }
        
        // Look for any review/challenge events
        if type_desc.to_lowercase().contains("review") || 
           type_desc.to_lowercase().contains("challenge") ||
           type_desc.to_lowercase().contains("video") ||
           type_desc.to_lowercase().contains("call") {
            review_events.push(format!("  📹 Review/Challenge event: {} (code: {})", 
                                       type_desc, type_code.unwrap_or(-1)));
        }
    }
    
    println!("  📊 Total goals found: {}", goal_count);
    
    if !review_events.is_empty() {
        println!("  🔍 Review/Challenge events found:");
        for event in review_events {
            println!("{}", event);
        }
    } else {
        println!("  ✅ No review/challenge events found");
    }
    
    println!("  📋 All event types in this game:");
    let mut sorted_types: Vec<_> = all_event_types.iter().collect();
    sorted_types.sort();
    for event_type in sorted_types {
        println!("    {}", event_type);
    }
    
    Ok(())
}

/// Find games that might have goal review events
pub async fn find_games_with_reviews(pool: &DbPool, limit: usize) -> Result<Vec<i64>, Box<dyn Error>> {
    // Get some recent games with play-by-play data
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT (parameters->>'game_id')::bigint as game_id
        FROM raw_data
        WHERE endpoint = 'game_play_by_play'
        AND parameters->>'game_id' IS NOT NULL
        ORDER BY game_id DESC
        LIMIT $1
        "#,
        limit as i32
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter()
        .filter_map(|row| row.game_id)
        .collect())
}

/// Search for games that contain specific event types or review-related events
pub async fn search_games_with_event_types(pool: &DbPool, search_terms: &[&str], limit: usize) -> Result<Vec<i64>, Box<dyn Error>> {
    // Use parameters to avoid SQL injection issues
    let mut query_conditions = Vec::new();
    let mut param_index = 2; // Start at 2 since $1 is used for limit
    
    for _ in search_terms {
        query_conditions.push(format!("data::text ILIKE ${}", param_index));
        param_index += 1;
    }
    let where_clause = query_conditions.join(" OR ");
    
    let query = format!(
        r#"
        SELECT DISTINCT (parameters->>'game_id')::bigint as game_id
        FROM raw_data
        WHERE endpoint = 'game_play_by_play'
        AND parameters->>'game_id' IS NOT NULL
        AND ({})
        ORDER BY game_id DESC
        LIMIT $1
        "#,
        where_clause
    );

    let mut query_builder = sqlx::query(&query);
    query_builder = query_builder.bind(limit as i32);
    
    for term in search_terms {
        query_builder = query_builder.bind(format!("%{}%", term));
    }
    
    let rows = query_builder.fetch_all(pool).await?;

    let mut game_ids = Vec::new();
    for row in rows {
        if let Some(game_id) = row.try_get::<Option<i64>, _>("game_id")? {
            game_ids.push(game_id);
        }
    }

    Ok(game_ids)
}

/// Get a comprehensive list of all event types across all games
pub async fn get_all_event_types(pool: &DbPool, limit: usize) -> Result<(), Box<dyn Error>> {
    println!("🔍 Analyzing event types across {} games...", limit);
    
    let games = find_games_with_reviews(pool, limit).await?;
    let mut all_event_types = std::collections::BTreeSet::new();
    let mut games_analyzed = 0;
    
    for game_id in games {
        let params = json!({"game_id": game_id.to_string()});
        
        if let Ok(play_by_play_data) = crate::storage::get_raw_data(pool, "game_play_by_play", &params).await {
            if let Some(plays) = play_by_play_data["plays"].as_array() {
                for play in plays {
                    let type_code = play["typeCode"].as_i64();
                    let type_desc = play["typeDescKey"].as_str().unwrap_or("unknown");
                    all_event_types.insert(format!("{}: {}", type_code.unwrap_or(-1), type_desc));
                }
                games_analyzed += 1;
            }
        }
    }
    
    println!("📊 Found {} unique event types across {} games:", all_event_types.len(), games_analyzed);
    for event_type in all_event_types {
        println!("  {}", event_type);
    }
    
    Ok(())
}

/// Examine goal events in detail to look for review/challenge information
pub async fn examine_goal_details(pool: &DbPool, game_id: i64) -> Result<(), Box<dyn Error>> {
    let params = json!({"game_id": game_id.to_string()});
    
    // Get play-by-play data
    let play_by_play_data = match crate::storage::get_raw_data(pool, "game_play_by_play", &params).await {
        Ok(data) => data,
        Err(e) => return Err(format!("Failed to get play-by-play data: {}", e).into()),
    };
    
    // Extract game info
    let game_info = play_by_play_data.as_object()
        .ok_or("Invalid play-by-play data format")?;
    
    // Get plays array
    let plays = game_info["plays"].as_array()
        .ok_or("Missing plays array")?;
    
    println!("🏒 Examining goal details for game {}...", game_id);
    
    let mut goal_count = 0;
    
    // Process each play to look for goals and examine their details
    for play in plays {
        if play["typeCode"].as_i64() == Some(505) {
            goal_count += 1;
            
            let scorer_name = play["details"]
                .get("scoringPlayerName")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let period = play["periodDescriptor"]["number"].as_i64().unwrap_or(0);
            let time = play["timeInPeriod"].as_str().unwrap_or("Unknown");
            
            println!("  ⚽ Goal #{}: {} - Period {} at {}", goal_count, scorer_name, period, time);
            
            // Print all details of the goal event
            if let Some(details) = play["details"].as_object() {
                println!("    🔍 Goal details:");
                for (key, value) in details {
                    if key.to_lowercase().contains("review") || 
                       key.to_lowercase().contains("challenge") ||
                       key.to_lowercase().contains("video") ||
                       key.to_lowercase().contains("call") ||
                       key.to_lowercase().contains("overturn") ||
                       key.to_lowercase().contains("cancel") ||
                       key.to_lowercase().contains("valid") {
                        println!("      🚨 REVIEW-RELATED: {} = {}", key, value);
                    } else {
                        println!("      {} = {}", key, value);
                    }
                }
            }
            
            // Also check if there are any other fields in the main play object that might indicate reviews
            println!("    🔍 All play fields:");
            for (key, value) in play.as_object().unwrap() {
                if key != "details" && key != "periodDescriptor" {
                    if key.to_lowercase().contains("review") || 
                       key.to_lowercase().contains("challenge") ||
                       key.to_lowercase().contains("video") ||
                       key.to_lowercase().contains("call") ||
                       key.to_lowercase().contains("overturn") ||
                       key.to_lowercase().contains("cancel") ||
                       key.to_lowercase().contains("valid") {
                        println!("      🚨 REVIEW-RELATED: {} = {}", key, value);
                    } else {
                        println!("      {} = {}", key, value);
                    }
                }
            }
            
            println!(); // Space between goals
        }
    }
    
    if goal_count == 0 {
        println!("  ℹ️  No goals found in this game");
    }
    
    Ok(())
}

/// Search for coach challenge events and goal reversals
pub async fn search_for_coach_challenges(pool: &DbPool, limit: usize) -> Result<(), Box<dyn Error>> {
    println!("🔍 Searching for coach challenge events in play-by-play data...");
    
    // Search for games containing challenge-related terms
    let challenge_terms = ["challenge", "Coach Challenge", "video review", "overturn", "goal reversed", "decision"];
    let games = search_games_with_event_types(pool, &challenge_terms, limit).await?;
    
    if games.is_empty() {
        println!("❌ No games found containing challenge-related terms");
        return Ok(());
    }
    
    println!("🏒 Found {} games with potential challenge events", games.len());
    
    for game_id in games {
        let params = json!({"game_id": game_id.to_string()});
        
        if let Ok(play_by_play_data) = crate::storage::get_raw_data(pool, "game_play_by_play", &params).await {
            if let Some(plays) = play_by_play_data["plays"].as_array() {
                println!("\n🏒 Analyzing game {} for challenge events...", game_id);
                
                let mut found_challenge_event = false;
                
                for (index, play) in plays.iter().enumerate() {
                    let type_desc = play["typeDescKey"].as_str().unwrap_or("unknown");
                    let type_code = play["typeCode"].as_i64();
                    
                    // Look for any unusual event types or descriptions
                    if type_desc.to_lowercase().contains("challenge") ||
                       type_desc.to_lowercase().contains("review") ||
                       type_desc.to_lowercase().contains("video") ||
                       type_desc.to_lowercase().contains("overturn") ||
                       type_desc.to_lowercase().contains("decision") ||
                       type_desc.to_lowercase().contains("reversed") ||
                       type_code.unwrap_or(-1) > 600 { // Look for unusual event codes
                        
                        found_challenge_event = true;
                        
                        println!("  🚨 POTENTIAL CHALLENGE EVENT:");
                        println!("    Type: {} (code: {})", type_desc, type_code.unwrap_or(-1));
                        println!("    Event index: {}", index);
                        
                        // Print full event details
                        println!("    Full event data:");
                        for (key, value) in play.as_object().unwrap() {
                            println!("      {} = {}", key, value);
                        }
                        
                        // Look at surrounding events
                        println!("    Surrounding events:");
                        for i in (index.saturating_sub(3))..=(index + 3).min(plays.len() - 1) {
                            if i == index {
                                continue;
                            }
                            let surrounding_play = &plays[i];
                            let surrounding_type = surrounding_play["typeDescKey"].as_str().unwrap_or("unknown");
                            let surrounding_code = surrounding_play["typeCode"].as_i64().unwrap_or(-1);
                            println!("      [{}] {}: {} ({})", 
                                    if i < index { "BEFORE" } else { "AFTER" },
                                    surrounding_code, 
                                    surrounding_type,
                                    surrounding_play["timeInPeriod"].as_str().unwrap_or("N/A"));
                        }
                        
                        println!();
                    }
                }
                
                if !found_challenge_event {
                    println!("  ✅ No obvious challenge events found in this game");
                }
            }
        }
    }
    
    Ok(())
}

/// Look for games where the goal count doesn't match our expectations
pub async fn find_goal_discrepancies(pool: &DbPool) -> Result<(), Box<dyn Error>> {
    println!("🔍 Looking for games where goals might have been called back...");
    
    // Get recent games with play-by-play data
    let games = find_games_with_reviews(pool, 20).await?;
    
    for game_id in games {
        let params = json!({"game_id": game_id.to_string()});
        
        // Get play-by-play data
        if let Ok(play_by_play_data) = crate::storage::get_raw_data(pool, "game_play_by_play", &params).await {
            // Get game data from database
            if let Ok(Some(game_row)) = sqlx::query!(
                "SELECT home_score, away_score FROM games WHERE game_id = $1",
                game_id
            ).fetch_optional(pool).await {
                
                // Count goals in play-by-play
                let mut pbp_goals = 0;
                if let Some(plays) = play_by_play_data["plays"].as_array() {
                    for play in plays {
                        if play["typeCode"].as_i64() == Some(505) {
                            pbp_goals += 1;
                        }
                    }
                }
                
                let total_score = game_row.home_score.unwrap_or(0) + game_row.away_score.unwrap_or(0);
                
                if pbp_goals != total_score {
                    println!("🚨 DISCREPANCY in game {}:", game_id);
                    println!("    Final score total: {}", total_score);
                    println!("    Goals in play-by-play: {}", pbp_goals);
                    println!("    Difference: {}", pbp_goals - total_score);
                    
                    if pbp_goals > total_score {
                        println!("    💡 This could indicate {} goal(s) were called back!", pbp_goals - total_score);
                    }
                }
            }
        }
    }
    
    Ok(())
}

/// Apply schema changes to add goal validity tracking
pub async fn add_goal_validity_tracking(pool: &DbPool) -> Result<(), Box<dyn Error>> {
    println!("🔧 Adding goal validity tracking columns to events.goals table...");
    
    // Add the new columns
    sqlx::query(r#"
        ALTER TABLE events.goals 
        ADD COLUMN IF NOT EXISTS is_valid BOOLEAN NOT NULL DEFAULT true,
        ADD COLUMN IF NOT EXISTS called_back_reason TEXT,
        ADD COLUMN IF NOT EXISTS challenge_result TEXT,
        ADD COLUMN IF NOT EXISTS review_timestamp TIMESTAMP
    "#).execute(pool).await?;
    
    // Add index for performance
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_goals_is_valid ON events.goals(is_valid)")
        .execute(pool).await?;
    
    // Add comments
    sqlx::query("COMMENT ON COLUMN events.goals.is_valid IS 'Whether the goal is valid and counts toward the final score (false if called back after review)'")
        .execute(pool).await?;
    
    sqlx::query("COMMENT ON COLUMN events.goals.called_back_reason IS 'Reason for goal being called back (e.g., \"offside\", \"goaltender interference\", \"high stick\")'")
        .execute(pool).await?;
    
    sqlx::query("COMMENT ON COLUMN events.goals.challenge_result IS 'Result of any coach challenge on this goal (e.g., \"successful challenge\", \"failed challenge\")'")
        .execute(pool).await?;
    
    sqlx::query("COMMENT ON COLUMN events.goals.review_timestamp IS 'When the goal was reviewed/challenged (if applicable)'")
        .execute(pool).await?;
    
    println!("✅ Goal validity tracking columns added successfully!");
    println!("📋 New columns:");
    println!("   - is_valid: Boolean (default true) - whether goal counts");
    println!("   - called_back_reason: Text - reason if goal was called back");
    println!("   - challenge_result: Text - result of any coach challenge");
    println!("   - review_timestamp: Timestamp - when goal was reviewed");
    
    Ok(())
}

/// Update existing goals to mark shootout goals as invalid
pub async fn fix_shootout_goals(pool: &DbPool) -> Result<(), Box<dyn Error>> {
    println!("🏒 Updating existing goals to mark shootout goals as invalid...");
    
    // Update goals where period is 5 (shootout)
    let updated_count = sqlx::query!(
        r#"
        UPDATE events.goals 
        SET is_valid = false, 
            called_back_reason = 'shootout goal'
        WHERE period = 5
        "#
    )
    .execute(pool)
    .await?
    .rows_affected();
    
    println!("✅ Updated {} shootout goals to is_valid = false", updated_count);
    
    // Also update any goals with time "00:00" in what might be shootout periods
    let additional_updated = sqlx::query!(
        r#"
        UPDATE events.goals 
        SET is_valid = false, 
            called_back_reason = 'shootout goal'
        WHERE time_in_period = '00:00'::interval
        AND period >= 4
        AND is_valid = true
        "#
    )
    .execute(pool)
    .await?
    .rows_affected();
    
    println!("✅ Updated {} additional potential shootout goals based on time pattern", additional_updated);
    
    // Show summary
    let valid_goals = sqlx::query!(
        "SELECT COUNT(*) as count FROM events.goals WHERE is_valid = true"
    )
    .fetch_one(pool)
    .await?;
    
    let invalid_goals = sqlx::query!(
        "SELECT COUNT(*) as count FROM events.goals WHERE is_valid = false"
    )
    .fetch_one(pool)
    .await?;
    
    println!("📊 Goal summary:");
    println!("   Valid goals (regulation/OT): {}", valid_goals.count.unwrap_or(0));
    println!("   Invalid goals (shootout): {}", invalid_goals.count.unwrap_or(0));
    
    Ok(())
}

/// Check for shootout goals and mark them appropriately during processing
fn is_shootout_goal(period_number: i32, time_in_period: &str, period_type: Option<&str>) -> bool {
    // Period 5 is typically shootout
    if period_number == 5 {
        return true;
    }
    
    // Check for SO period type
    if let Some(ptype) = period_type {
        if ptype.to_uppercase() == "SO" {
            return true;
        }
    }
    
    // Check for 00:00 time in later periods (shootout pattern)
    if time_in_period == "00:00" && period_number >= 4 {
        return true;
    }
    
    false
}