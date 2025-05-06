use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn read_from_cache(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub fn write_to_cache(path: &Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

pub fn get_game_cache_path(game_id: &str, file_type: &str) -> PathBuf {
    let path = format!("data/raw/games/{}/{}.json", game_id, file_type);
    PathBuf::from(path)
}

