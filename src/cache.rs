use std::fs;
use std::path::PathBuf;

pub fn read_from_cache(file_path: &PathBuf) -> Option<String> {
    fs::read_to_string(file_path).ok()
}

pub fn write_to_cache(
    file_path: &PathBuf,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file_path, content)?;
    Ok(())
}
