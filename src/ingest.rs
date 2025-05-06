use crate::api;
use crate::cache;

const GAME_STORY_API_URL: &str = "https://api-web.nhle.com/v1/wsc/game-story/";

pub fn fetch_game_story(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = cache::get_game_cache_path(game_id, "story");

    if let Some(_) = cache::read_from_cache(&file_path) {
        println!("✅ Found cached game story at {:?}", file_path);
        return Ok(());
    }

    println!("🌐 Fetching game story from NHL API...");

    let url = format!("{}{}", GAME_STORY_API_URL, game_id);
    let json = api::fetch_api_json(&url)?;

    cache::write_to_cache(&file_path, &json)?;

    println!("💾 Saved game story to {:?}", file_path);

    Ok(())
}
