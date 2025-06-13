use crate::storage::DbPool;
use crate::processing::process_structured_data_for_game;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;

/// Find game IDs that exist in raw_data but not in the games table
pub async fn find_missing_games(pool: &DbPool) -> Result<Vec<i64>, sqlx::Error> {
    // Query to find game IDs in raw_data that have complete data
    // We consider a game complete if it has boxscore data
    let rows = sqlx::query!(
        r#"
        WITH raw_game_ids AS (
            SELECT DISTINCT (parameters->>'game_id')::bigint as game_id
            FROM raw_data
            WHERE endpoint = 'game_boxscore'
            AND parameters->>'game_id' IS NOT NULL
        )
        SELECT r.game_id
        FROM raw_game_ids r
        LEFT JOIN games g ON g.game_id = r.game_id
        WHERE g.game_id IS NULL
        ORDER BY r.game_id
        "#
    )
    .fetch_all(pool)
    .await?;

    // Filter out any NULL values and collect into Vec<i64>
    Ok(rows.into_iter()
        .filter_map(|row| row.game_id)
        .collect())
}

/// Process missing games in batches
pub async fn process_missing_games(
    pool: &DbPool,
    batch_size: usize,
    dry_run: bool,
) -> Result<(usize, usize, usize), Box<dyn std::error::Error>> {
    println!("🔍 Finding games that need processing...");
    let missing_games = find_missing_games(pool).await?;
    
    if missing_games.is_empty() {
        println!("✨ All games are already processed!");
        return Ok((0, 0, 0));
    }

    println!("📊 Found {} games that need processing", missing_games.len());
    
    if dry_run {
        println!("🔍 DRY RUN - would process games: {:?}", missing_games);
        return Ok((missing_games.len(), 0, 0));
    }

    let mut success_count = 0;
    let mut error_count = 0;
    let start_time = Instant::now();

    // Create progress bar
    let pb = ProgressBar::new(missing_games.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );

    // Process in batches
    for chunk in missing_games.chunks(batch_size) {
        for &game_id in chunk {
            match process_structured_data_for_game(game_id, pool).await {
                Ok(true) => {
                    success_count += 1;
                    pb.println(format!("✅ Processed game {}", game_id));
                }
                Ok(false) => {
                    error_count += 1;
                    pb.println(format!("⚠️  No data found for game {}", game_id));
                }
                Err(e) => {
                    error_count += 1;
                    pb.println(format!("❌ Error processing game {}: {}", game_id, e));
                }
            }
            pb.inc(1);
        }
    }

    pb.finish_and_clear();
    
    let duration = start_time.elapsed();
    println!("\n📊 Processing complete in {:.2}s", duration.as_secs_f32());
    println!("✅ Successfully processed: {}", success_count);
    println!("❌ Errors/Skips: {}", error_count);
    println!("📝 Total games attempted: {}", missing_games.len());

    Ok((missing_games.len(), success_count, error_count))
} 