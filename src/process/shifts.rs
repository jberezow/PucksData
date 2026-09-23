//! Season-scoped ingestion of typed, unnormalized NHL shift rows.

use std::future::Future;

use tokio::task::JoinSet;

const FIRST_SHIFT_SEASON: i32 = 20102011;
const MAX_CONCURRENT_GAMES: usize = 5;

#[derive(Debug, Default)]
pub struct ShiftRunSummary {
    pub candidates: usize,
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub unavailable_games: Vec<i64>,
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
/// resumes at games without rows. `--refresh` replaces snapshots only when
/// the source returns shift rows; unavailable games retain their stored rows.
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
    // Session locks are unsafe behind a transaction pooler. Keep a dedicated
    // transaction open so the lock stays on one backend and release it before
    // returning. Use a new key to avoid orphaned legacy session locks.
    // Stop any older shift loaders before starting this version.
    let mut run_lock = pool.begin().await?;
    // Only this lock transaction gets a predictable idle deadline. Heartbeats
    // below keep it alive; an abandoned connection still expires automatically.
    sqlx::query("SET LOCAL idle_in_transaction_session_timeout = '60s'")
        .execute(&mut *run_lock)
        .await?;
    let acquired: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('pucksdata:shifts:backfill', 0))",
    )
    .fetch_one(&mut *run_lock)
    .await?;
    if !acquired {
        run_lock.rollback().await?;
        return Err("another shift backfill is already running".into());
    }

    let games = match query_games(pool, season, refresh).await {
        Ok(games) => games,
        Err(error) => {
            run_lock.rollback().await?;
            return Err(error.into());
        }
    };
    let result: Result<ShiftRunSummary, crate::AnyError> = {
        let work = load_games(games, |game_id| {
            let pool = pool.clone();
            async move {
                let shifts = crate::fetchers::shifts::fetch_game_shifts(game_id).await?;
                crate::loaders::shifts::replace_game_shifts(&pool, game_id, &shifts)
                    .await
                    .map_err(Into::into)
            }
        });
        tokio::pin!(work);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                summary = &mut work => break Ok(summary),
                _ = heartbeat.tick() => {
                    let check = tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        sqlx::query("SELECT 1").execute(&mut *run_lock),
                    ).await;
                    match check {
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => break Err(format!(
                            "shift run lock connection lost; stopped ingestion: {error}"
                        ).into()),
                        Err(_) => break Err(
                            "shift run lock heartbeat timed out; stopped ingestion".into()
                        ),
                    }
                }
            }
        }
        // Dropping work on a heartbeat failure aborts its in-flight tasks.
        // Per-game transactions protect previously committed snapshots.
    };
    match result {
        Ok(summary) => {
            run_lock.commit().await?;
            Ok(summary)
        }
        Err(error) => {
            // This connection may already be dead; retain the original error.
            let _ = run_lock.rollback().await;
            Err(error)
        }
    }
}

async fn load_games<F, Fut>(games: Vec<i64>, load: F) -> ShiftRunSummary
where
    F: Fn(i64) -> Fut,
    Fut: Future<Output = Result<usize, crate::AnyError>> + Send + 'static,
{
    let progress = crate::ui::make_progress_bar(games.len() as u64, "shift games loaded");
    let mut summary = ShiftRunSummary {
        candidates: games.len(),
        ..ShiftRunSummary::default()
    };

    let mut games = games.into_iter();
    let mut tasks = JoinSet::new();
    for game_id in games.by_ref().take(MAX_CONCURRENT_GAMES) {
        let future = load(game_id);
        tasks.spawn(async move { (game_id, future.await) });
        summary.attempted += 1;
    }

    while let Some(outcome) = tasks.join_next().await {
        match outcome {
            Ok((_, Ok(count))) => {
                summary.succeeded += 1;
                summary.shifts += count;
            }
            Ok((game_id, Err(error)))
                if error
                    .downcast_ref::<crate::fetchers::shifts::ShiftsUnavailable>()
                    .is_some() =>
            {
                summary.unavailable_games.push(game_id);
            }
            Ok((game_id, Err(error))) => {
                summary.failed += 1;
                summary.failures.push(format!("game {game_id}: {error}"));
                if upstream_unavailable(&error) {
                    summary.stopped_early = true;
                }
            }
            Err(error) => {
                summary.failed += 1;
                summary.failures.push(format!("shift task failed: {error}"));
                summary.stopped_early = true;
            }
        }
        progress.inc(1);
        // Keep a bounded window, like event backfill. After an upstream
        // failure, drain in-flight games without starting any more requests.
        if !summary.stopped_early {
            if let Some(game_id) = games.next() {
                let future = load(game_id);
                tasks.spawn(async move { (game_id, future.await) });
                summary.attempted += 1;
            }
        }
    }
    progress.finish_and_clear();
    summary.unavailable_games.sort_unstable();
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn fills_and_refills_a_five_game_window() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let summary = load_games((0..12).collect(), |_| {
            let active = active.clone();
            let peak = peak.clone();
            async move {
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(count, Ordering::SeqCst);
                tokio::task::yield_now().await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(10)
            }
        })
        .await;
        assert_eq!(peak.load(Ordering::SeqCst), 5);
        assert_eq!(summary.attempted, 12);
        assert_eq!(summary.succeeded, 12);
        assert_eq!(summary.shifts, 120);
        assert!(!summary.stopped_early);
    }

    #[tokio::test]
    async fn upstream_failure_stops_refills_and_drains_in_flight_games() {
        let summary = load_games((0..12).collect(), |game_id| async move {
            if game_id == 0 {
                return Err(crate::api::ApiError::Other(503).into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(10)
        })
        .await;
        assert!(summary.stopped_early);
        assert_eq!(summary.candidates, 12);
        assert_eq!(summary.attempted, 5);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.succeeded, 4);
        assert_eq!(summary.shifts, 40);
    }

    #[tokio::test]
    async fn all_unavailable_games_complete_without_failures() {
        let games: Vec<i64> = (2024021235..=2024021291).collect();
        let summary = load_games(games.clone(), |game_id| async move {
            Err(crate::fetchers::shifts::ShiftsUnavailable { game_id }.into())
        })
        .await;
        assert_eq!(summary.candidates, 57);
        assert_eq!(summary.attempted, 57);
        assert_eq!(summary.unavailable_games, games);
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.shifts, 0);
        assert!(summary.failures.is_empty());
        assert!(!summary.stopped_early);
    }

    #[tokio::test]
    async fn source_gaps_are_reported_without_stopping_or_counting_as_success() {
        let summary = load_games((0..12).collect(), |game_id| async move {
            match game_id {
                0 | 5 | 11 => Err(crate::fetchers::shifts::ShiftsUnavailable { game_id }.into()),
                1 => Err("malformed shift response".into()),
                _ => Ok(10),
            }
        })
        .await;
        assert_eq!(summary.attempted, 12);
        assert_eq!(summary.succeeded, 8);
        assert_eq!(summary.unavailable_games, vec![0, 5, 11]);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failures, vec!["game 1: malformed shift response"]);
        assert_eq!(summary.shifts, 80);
        assert!(!summary.stopped_early);
    }

    #[test]
    fn stops_on_server_failures_but_not_source_gaps() {
        let unavailable: crate::AnyError = crate::api::ApiError::Other(503).into();
        let missing: crate::AnyError = crate::api::ApiError::NotFound.into();
        assert!(upstream_unavailable(&unavailable));
        assert!(!upstream_unavailable(&missing));
    }
}
