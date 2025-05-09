use crate::api;
use crate::cache;

const GAME_STORY_API_URL: &str = "https://api-web.nhle.com/v1/wsc/game-story/";
const GAME_BOXSCORE_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{}/boxscore";
const GAME_PLAY_BY_PLAY_API_URL: &str = "https://api-web.nhle.com/v1/gamecenter/{}/play-by-play";

fn fetch_and_cache(game_id: &str, endpoint: &str, url_template: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = cache::get_game_cache_path(game_id, endpoint);

    if let Some(_) = cache::read_from_cache(&file_path) {
        println!("✅ Found cached {} data at {:?}", endpoint, file_path);
        return Ok(());
    }

    println!("🌐 Fetching {} data from NHL API...", endpoint);

    let url = url_template.replace("{}", game_id);
    let json = api::fetch_api_json(&url)?;

    cache::write_to_cache(&file_path, &json)?;

    println!("💾 Saved {} data to {:?}", endpoint, file_path);

    Ok(())
}

pub fn fetch_game_story(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    fetch_and_cache(game_id, "story", GAME_STORY_API_URL)
}

pub fn fetch_game_boxscore(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    fetch_and_cache(game_id, "boxscore", GAME_BOXSCORE_API_URL)
}

pub fn fetch_game_play_by_play(game_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    fetch_and_cache(game_id, "playbyplay", GAME_PLAY_BY_PLAY_API_URL)
}
