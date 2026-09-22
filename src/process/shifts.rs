//! Season-scoped ingestion of typed, unnormalized NHL shift rows.

const FIRST_SHIFT_SEASON: i32 = 20102011;
const REQUEST_START_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Debug, Default)]
pub struct ShiftRunSummary {
    pub candidates: usize,
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub shifts: usize,
    pub stopped_early: bool,
    pub failures: Vec<String>,
}

fn upstream_unavailable(error: &crate::AnyError) -> bool {
    error
        .downcast_ref::<crate::api::ApiError>()
        .is_some_and(|error| match error {
            crate::api::ApiError::NetworkError(_) => true,
            crate::api::ApiError::Other(status) => *status == 429 || *status >= 500,
            crate::api::ApiError::NotFound => false,
        })
}

async fn query_games(
    pool: &sqlx::PgPool,
    season: i32,
    refresh: bool,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT g.game_id
           FROM games g
           WHERE g.season = $1
             AND g.season >= $2
             AND g.game_type IN (2, 3)
             AND g.game_state IN ('OFF', 'OVER', 'FINAL')
             AND ($3 OR NOT EXISTS (
                 SELECT 1 FROM shifts s WHERE s.game_id = g.game_id
             ))
           ORDER BY g.game_date, g.game_id"#,
    )
    .bind(season)
    .bind(FIRST_SHIFT_SEASON)
    .bind(refresh)
    .fetch_all(pool)
    .await
}

/// Load one season. Games are committed independently, so a normal rerun
/// resumes at games without rows. `--refresh` replaces every game's snapshot.
pub async fn run_backfill(
    pool: &sqlx::PgPool,
    season: i32,
    refresh: bool,
) -> Result<ShiftRunSummary, crate::AnyError> {
    if season < FIRST_SHIFT_SEASON {
        return Err(format!(
            "shift ingestion begins with season {FIRST_SHIFT_SEASON}; received {season}"
        )
        .into());
    }
    let lock = sqlx::postgres::PgAdvisoryLock::new("pucksdata_shift_backfill");
    let connection = pool.acquire().await?;
    let _lock_guard = match lock.try_acquire(connection).await? {
        sqlx::Either::Left(guard) => guard,
        sqlx::Either::Right(_) => return Err("another shift backfill is already running".into()),
    };

    let games = query_games(pool, season, refresh).await?;
    let progress = crate::ui::make_progress_bar(games.len() as u64, "shift games loaded");
    let mut summary = ShiftRunSummary {
        candidates: games.len(),
        ..ShiftRunSummary::default()
    };

    for game_id in games {
        let result: Result<usize, crate::AnyError> = async {
            let shifts = crate::fetchers::shifts::fetch_game_shifts(game_id).await?;
            crate::loaders::shifts::replace_game_shifts(pool, game_id, &shifts)
                .await
                .map_err(Into::into)
        }
        .await;
        summary.attempted += 1;
        match result {
            Ok(count) => {
                summary.succeeded += 1;
                summary.shifts += count;
            }
            Err(error) => {
                summary.failed += 1;
                summary.failures.push(format!("game {game_id}: {error}"));
                if upstream_unavailable(&error) {
                    summary.stopped_early = true;
                    progress.inc(1);
                    break;
                }
            }
        }
        progress.inc(1);
        // Sustained parallel season loads eventually make the NHL endpoint
        // time out without returning 429. Keep this deliberately polite.
        tokio::time::sleep(REQUEST_START_INTERVAL).await;
    }
    progress.finish_and_clear();
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_on_server_failures_but_not_source_gaps() {
        let unavailable: crate::AnyError = crate::api::ApiError::Other(503).into();
        let missing: crate::AnyError = crate::api::ApiError::NotFound.into();
        assert!(upstream_unavailable(&unavailable));
        assert!(!upstream_unavailable(&missing));
    }
}
