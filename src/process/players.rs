//! Refresh players and accept only complete roster observations.
use crate::{fetchers, loaders};

pub async fn refresh(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let fetched = fetchers::players::fetch_players(pool).await?;
    let count = fetched.players.len();

    // The bulk upsert has no meaningful per-record progress.
    let spinner = crate::ui::make_spinner(&format!("Writing {count} players..."));
    loaders::players::upsert_players(pool, &fetched.players)
        .await
        .inspect_err(|_| spinner.finish_and_clear())?;
    spinner.finish_and_clear();
    tracing::info!("Wrote {count} players");

    let rosters = fetched
        .current_rosters
        .ok_or("current roster observation unavailable")?;
    if !rosters.is_complete() {
        return Err("current roster observation incomplete; snapshot preserved".into());
    }
    let snapshot_id = loaders::rosters::insert_roster_snapshot(pool, &rosters).await?;
    tracing::info!(
        snapshot_id,
        teams = rosters.fetched_team_count,
        memberships = rosters.memberships.len(),
        "roster snapshot written"
    );

    Ok(())
}
