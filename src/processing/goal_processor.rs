use crate::storage::DbPool;
use serde_json::json;
use sqlx::postgres::types::PgInterval;
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
                // Insert goal into database
                sqlx::query!(
                    r#"
                    INSERT INTO events.goals (
                        game_id, period, period_type, time_in_period, situation_code,
                        scoring_team_id, defending_team_id, scorer_id, primary_assist_id,
                        secondary_assist_id, goalie_id, strength, shot_type, x_coord,
                        y_coord, zone_code, empty_net
                    ) VALUES (
                        $1, $2, $3, $4, $5,
                        $6, $7, $8, $9,
                        $10, $11, $12, $13, $14,
                        $15, $16, $17
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
                    empty_net
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