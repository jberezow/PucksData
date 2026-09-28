//! Refresh the complete season game catalogue.
use crate::{fetchers, loaders};

pub async fn refresh_all(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let seasons = fetchers::games::fetch_seasons_list().await?;
    let total_seasons = seasons.len();
    let mut total_games = 0usize;

    for (i, season) in seasons.iter().enumerate() {
        tracing::info!(
            "[{}/{}] Fetching season {}...",
            i + 1,
            total_seasons,
            season
        );

        let pb_fetch = crate::ui::make_progress_bar(0, "games fetched");
        let games = fetchers::games::fetch_games_for_season_enriched(*season, &pb_fetch).await?;
        let count = games.len();
        pb_fetch.finish_and_clear();

        if count > 0 {
            let pb_upsert = crate::ui::make_progress_bar(count as u64, "games written");
            loaders::games::upsert_games(pool, &games, &pb_upsert)
                .await
                .inspect_err(|_| pb_upsert.finish_and_clear())?;
            pb_upsert.finish_and_clear();
        }
        total_games += count;
    }
    tracing::info!(
        "Fetched {total_games} total games across {total_seasons} seasons, upserted {total_games}"
    );
    Ok(())
}
