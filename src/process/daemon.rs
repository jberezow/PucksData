//! Long-lived daemon — interval-based sync loop with SIGTERM/Ctrl-C graceful shutdown.

/// Run periodic synchronization until SIGTERM or Ctrl-C is received.
pub async fn run_daemon(
    pool: &sqlx::PgPool,
    interval_secs: u64,
    backfill_on_start: bool,
) -> Result<(), crate::AnyError> {
    if interval_secs == 0 {
        return Err("daemon interval must be greater than zero".into());
    }
    crate::db::require_connections(pool, 2, "daemon")?;
    let lease = crate::process::sync::acquire_daemon_lock(pool).await?;
    lease
        .run(async {
            tokio::select! {
                result = run_until_shutdown(pool, interval_secs, backfill_on_start) => result,
                result = shutdown_signal() => {
                    result?;
                    tracing::info!("shutdown received; stopping daemon");
                    Ok(())
                }
            }
        })
        .await
}

async fn run_until_shutdown(
    pool: &sqlx::PgPool,
    interval_secs: u64,
    backfill_on_start: bool,
) -> Result<(), crate::AnyError> {
    if backfill_on_start {
        crate::process::attempts::command(
            pool,
            "daemon backfill-on-start",
            crate::process::backfill::run_backfill(pool, None),
        )
        .await?;
    }

    // A slow sync should skip missed ticks rather than trigger burst catch-up.
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        tick_sync(pool, interval_secs).await;
    }
}

#[cfg(unix)]
async fn shutdown_signal() -> Result<(), std::io::Error> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> Result<(), std::io::Error> {
    tokio::signal::ctrl_c().await
}

/// Execute one sync tick, leaving failures for the next interval to retry.
async fn tick_sync(pool: &sqlx::PgPool, interval_secs: u64) {
    tracing::info!("starting sync");

    match crate::process::sync::run_sync(pool, None).await {
        Ok(_summary) => {
            tracing::info!(interval_secs, "sync completed");
        }
        Err(e) => {
            tracing::error!(error = %e, "sync failed; retrying next interval");
        }
    }
}
