//! Small frequent refresh: no historical player audits or derived-table rebuilds.
use crate::{fetchers, loaders};
use sqlx::Row;

/// Poll live games and retry the latest failed observation, even if an older
/// successful observation exists. Accepted final reports are rechecked every six hours.
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
          AND g.game_date BETWEEN (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date-3
                              AND (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date
          AND (g.start_time_utc IS NULL OR g.start_time_utc <= CURRENT_TIMESTAMP)
          AND (g.game_state NOT IN ('OFF','OVER','FINAL') OR g.game_state IS NULL
               OR latest.outcome IS DISTINCT FROM 'complete'
               OR latest.finished_at <= CURRENT_TIMESTAMP-interval '6 hours')
        ORDER BY g.game_id
    "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn run(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let mapping = fetchers::games::fetch_team_id_to_franchise_id_map().await?;
    let games = candidates(pool).await?;
    let mut failures = Vec::new();
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
            if error.is::<crate::error::Deferred>() {
                tracing::warn!(game_id=id, error=%error, "game refresh deferred until next pass");
            } else {
                tracing::error!(game_id=id, error=%error, "game refresh failed");
            }
            failures.push(error);
        }
    }
    if !failures.is_empty() {
        return Err(crate::error::batch_error(
            "game refreshes incomplete; other accepted games were committed",
            failures,
        ));
    }
    Ok(())
}

/// Daily consumer maintenance without archive events, historical player audits,
/// missing-player scans, or materialized-view rebuilds. Keep schedule discovery
/// independent of the frequent pass so new seasons and rescheduled games arrive.
pub async fn daily(pool: &sqlx::PgPool) -> Result<(), crate::AnyError> {
    let today = time::OffsetDateTime::now_utc().date();
    let from = today - time::Duration::days(super::sync::correction_days()? as i64);
    let teams = fetchers::teams::fetch_teams().await?;
    loaders::teams::upsert_teams(pool, &teams, &indicatif::ProgressBar::hidden()).await?;
    super::attempts::track(
        pool,
        "schedule",
        "current",
        super::sync::refresh_current_season_games(pool, from),
    )
    .await?;

    // A roster failure must not prevent independently valid game reports arriving.
    let players = super::attempts::track(pool, "players_rosters", "current", async {
        let fetched =
            fetchers::players::fetch_players_for_seasons(&super::sync::active_seasons(today))
                .await?;
        loaders::players::upsert_players(pool, &fetched.players).await?;
        let rosters = fetched
            .current_rosters
            .ok_or("current roster observation unavailable")?;
        if !rosters.is_complete() {
            return Err("current roster observation incomplete; snapshot preserved".into());
        }
        loaders::rosters::insert_roster_snapshot(pool, &rosters).await?;
        Ok(())
    })
    .await;
    let official = super::attempts::track(
        pool,
        "official_audit",
        "current",
        super::official_games::sync_current_official_games(
            pool,
            from,
            &super::sync::active_seasons(today),
        ),
    )
    .await;
    players?;
    let summary = official?;
    record_daily_success(pool, summary.games).await?;
    Ok(())
}

/// A separate watermark lets consumers require successful game maintenance
/// without claiming that archive events or derived analytics were refreshed.
pub async fn record_daily_success(pool: &sqlx::PgPool, games: usize) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ingestion.sync_state(key,last_sync_at,last_sync_games,updated_at)
         VALUES ('official_games',clock_timestamp(),$1,clock_timestamp())
         ON CONFLICT(key) DO UPDATE SET last_sync_at=EXCLUDED.last_sync_at,
             last_sync_games=EXCLUDED.last_sync_games,updated_at=EXCLUDED.updated_at",
    )
    .bind(i32::try_from(games).map_err(|error| sqlx::Error::Protocol(error.to_string()))?)
    .execute(pool)
    .await?;
    Ok(())
}
