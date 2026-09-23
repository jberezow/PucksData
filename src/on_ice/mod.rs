//! Read-only, reproducible shift validation and event-level on-ice reconstruction.
//! Neither raw shifts nor NHL events are repaired or supplemented by this module.
pub mod audit;
pub mod clock;
pub mod read;
mod reconstruct;
mod toi;
pub mod types;
mod validate;

pub use types::*;

pub fn source_sha256(source: &GameSource) -> String {
    use sha2::{Digest, Sha256};
    // Source ordering is normalized by the reader/replay validator.
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(source).expect("serializable source"))
    )
}

pub fn analyze(source: &GameSource) -> GameReport {
    let validated = validate::validate(source);
    let events = reconstruct::reconstruct(source, &validated);
    let toi = toi::reconcile(source, &validated);
    let mut counts = std::collections::BTreeMap::new();
    for issue in &validated.issues {
        *counts.entry(issue.code).or_insert(0) += 1;
    }
    GameReport {
        method: METHOD_VERSION.to_string(),
        source_sha256: source_sha256(source),
        game: source.game.clone(),
        supported: source.game.season >= FIRST_SEASON && matches!(source.game.game_type, 2 | 3),
        source_rows: source.shifts.len(),
        accepted_rows: validated.intervals.len(),
        rejected_rows: source.shifts.len() - validated.intervals.len(),
        role_fallback_players: validated.fallback_players.len(),
        validation: validated.issues,
        validation_counts: counts,
        toi,
        events,
    }
}
