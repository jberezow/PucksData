// src/process/daemon.rs
// Implements the pucksdata daemon: an interval-based sync loop with graceful
// SIGTERM/Ctrl-C shutdown and single-instance enforcement via an advisory lock.
//
// Requirements: DAEMON-01 (interval loop), DAEMON-02 (MissedTickBehavior::Skip),
//               DAEMON-03 (SIGTERM/Ctrl-C graceful exit), QUAL-SYNC-03 (stateless loop body).

/// Run the pucksdata daemon.
///
/// - Acquires an advisory lock (single-instance enforcement via QUAL-SYNC-02).
/// - Optionally runs a full backfill before entering the loop (--backfill-on-start).
/// - Calls run_sync() on each interval tick (DAEMON-01).
/// - Uses MissedTickBehavior::Skip so a slow sync never causes burst catch-up (DAEMON-02).
/// - Handles SIGTERM and Ctrl-C with a clean log message before exiting (DAEMON-03).
/// - Never accumulates errors across ticks — each error is logged and dropped (QUAL-SYNC-03).
pub async fn run_daemon(
    pool: &sqlx::PgPool,
    interval_secs: u64,
    backfill_on_start: bool,
) -> Result<(), crate::AnyError> {
    // Advisory lock — held for process lifetime (QUAL-SYNC-02).
    // Underscore prefix keeps the guard alive without "unused variable" lint.
    // Returns Err immediately if another daemon is already running.
    let _lock = crate::process::sync::acquire_daemon_lock(pool).await?;

    // Optional startup backfill — propagate error rather than entering loop.
    if backfill_on_start {
        crate::process::backfill::run_backfill(pool, None).await?;
    }

    // Interval construction: set Skip BEFORE the first tick fires (DAEMON-02).
    // MissedTickBehavior::Burst (the tokio default) must NOT be used.
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // CancellationToken as extension point for future alert hooks (v1.3).
    let _shutdown_token = tokio_util::sync::CancellationToken::new();
    // TODO(v1.3-alerts): pass shutdown_token.child_token() into alert hooks here

    run_loop(pool, interval_secs, &mut interval).await
}

#[cfg(unix)]
async fn run_loop(
    pool: &sqlx::PgPool,
    interval_secs: u64,
    interval: &mut tokio::time::Interval,
) -> Result<(), crate::AnyError> {
    // SIGTERM stream — created OUTSIDE the loop (Pitfall 5: recreating the stream
    // on each iteration drops previously-pending signals).
    let mut sigterm = tokio::signal::unix::signal(
        tokio::signal::unix::SignalKind::terminate(),
    )?;

    loop {
        tokio::select! {
            // Tick branch: run_sync() is called inside the BODY (not as a future),
            // guaranteeing data integrity — partial syncs are never interrupted (Pitfall 1).
            _ = interval.tick() => {
                tick_sync(pool, interval_secs).await;
            }

            // Ctrl-C handler.
            _ = tokio::signal::ctrl_c() => {
                eprintln!("received Ctrl-C — shutting down");
                break;
            }

            // SIGTERM handler.
            _ = sigterm.recv() => {
                eprintln!("received SIGTERM — shutting down");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
async fn run_loop(
    pool: &sqlx::PgPool,
    interval_secs: u64,
    interval: &mut tokio::time::Interval,
) -> Result<(), crate::AnyError> {
    loop {
        tokio::select! {
            // Tick branch: run_sync() is called inside the BODY (not as a future),
            // guaranteeing data integrity — partial syncs are never interrupted (Pitfall 1).
            _ = interval.tick() => {
                tick_sync(pool, interval_secs).await;
            }

            // Ctrl-C handler.
            _ = tokio::signal::ctrl_c() => {
                eprintln!("received Ctrl-C — shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Execute one sync tick.
///
/// Logs start time, calls run_sync(), logs next-sync time on success or logs
/// the error and continues on failure.  Error is NEVER accumulated
/// (QUAL-SYNC-03 / Pitfall 6).
async fn tick_sync(pool: &sqlx::PgPool, interval_secs: u64) {
    eprintln!(
        "[{}] starting sync",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    match crate::process::sync::run_sync(pool, None).await {
        Ok(_summary) => {
            let next = chrono::Utc::now()
                + chrono::Duration::seconds(interval_secs as i64);
            eprintln!("next sync at {} UTC", next.format("%H:%M"));
        }
        Err(e) => {
            // Log and continue — do NOT accumulate errors (QUAL-SYNC-03 / Pitfall 6).
            eprintln!("sync failed, continuing: {}", e);
        }
    }
}
