//! Small frequent refresh: no historical player audits or derived-table rebuilds.
use crate::{fetchers, loaders};
use sqlx::Row;

/// Poll live games and retry the latest failed observation, even if an older
/// successful observation exists. Accepted final reports are rechecked hourly.
pub async fn candidates(pool: &sqlx::PgPool) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
    sqlx::query(
        r#"
        SELECT g.game_id,g.season,g.game_date,g.game_type,g.home_team_id,g.away_team_id
        FROM games g
        LEFT JOIN LATERAL (
            SELECT outcome,finished_at FROM ingestion.attempts
            WHERE dataset='official_games' AND entity_key=g.game_id::text
            ORDER BY attempt_id DESC LIMIT 1
        ) latest ON TRUE
        WHERE g.game_type IN (2,3)
          AND g.game_date BETWEEN (clock_timestamp() AT TIME ZONE 'UTC')::date-3
                              AND (clock_timestamp() AT TIME ZONE 'UTC')::date
          AND (g.start_time_utc IS NULL OR g.start_time_utc <= clock_timestamp())
          AND (g.game_state NOT IN ('OFF','OVER','FINAL') OR g.game_state IS NULL
               OR latest.outcome IS DISTINCT FROM 'complete'
               OR latest.finished_at <= clock_timestamp()-interval '1 hour')
        ORDER BY g.game_id
    "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn run(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let mapping = fetchers::games::fetch_team_id_to_franchise_id_map().await?;
    let games = candidates(pool).await?;
    let mut failures = 0;
    for game in games {
        let id: i64 = game.get("game_id");
        let result: Result<(), crate::AnyError> = async {
            let boxscore = fetchers::games::fetch_game_boxscore(id).await?;
            if mapping.get(&boxscore.home_team.id) != Some(&game.get::<i64, _>("home_team_id"))
                || mapping.get(&boxscore.away_team.id) != Some(&game.get::<i64, _>("away_team_id"))
            {
                return Err("game team identity changed; full reconciliation required".into());
            }
            let stats = fetchers::games::StatsGameRecord {
                id,
                season: game.get("season"),
                game_date: game.get::<time::Date, _>("game_date").to_string(),
                game_type: game.get("game_type"),
                home_team_id: boxscore.home_team.id,
                away_team_id: boxscore.away_team.id,
                home_score: boxscore.home_team.score,
                away_score: boxscore.away_team.score,
            };
            let record = fetchers::games::transform_game(&stats, Some(&boxscore), &mapping)?;
            let completed = record
                .game_state
                .as_deref()
                .is_some_and(super::sync::is_game_completed);
            loaders::games::upsert_games(pool, &[record], &indicatif::ProgressBar::hidden())
                .await?;
            if completed {
                super::official_games::run_official_games(pool, Some(id), None, None).await?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            failures += 1;
            tracing::warn!(game_id=id, error=%error, "game refresh deferred until next pass");
        }
    }
    if failures > 0 {
        return Err(format!(
            "{failures} game refreshes deferred; other accepted games were committed"
        )
        .into());
    }
    Ok(())
}
