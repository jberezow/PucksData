//! CLI entry point — parses [`clap`] commands and dispatches to library functions.
use clap::{Args, Parser, Subcommand};
use pucksdata::{db, fetchers, loaders};

#[derive(Parser)]
#[command(name = "pucksdata", about = "NHL Data ETL Engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Rebuild materialized products invalidated by committed data corrections
    RefreshDerived,
    /// Fetch and upsert NHL entity metadata
    Fetch {
        #[command(subcommand)]
        entity: FetchEntity,
    },
    /// Run full historical backfill (events only; entity tables must be pre-populated)
    Backfill(BackfillArgs),
    /// Fill event gaps and audit recent completed-game corrections
    Sync(SyncArgs),
    /// Run as a long-lived daemon, calling sync on a configurable interval
    Daemon(DaemonArgs),
    /// Check DB health per season: game counts, event coverage %, goals-in-shots, backfill status
    Status(StatusArgs),
    /// Ingest NHL player-shift rows
    Shifts {
        #[command(subcommand)]
        command: ShiftsCommand,
    },
}

#[derive(Subcommand)]
enum ShiftsCommand {
    /// Load typed, unnormalized shift-chart rows for one season
    Backfill(ShiftBackfillArgs),
    /// Validate and audit a season from a read-only snapshot, or replay an export
    Audit(ShiftAuditArgs),
    /// Derive event lineups and diagnostics for one completed game
    Reconstruct {
        #[arg(long)]
        game: i64,
        #[arg(long)]
        output: Option<std::path::PathBuf>,
    },
}

#[derive(Args)]
struct ShiftAuditArgs {
    #[arg(long)]
    season: i32,
    /// Replay a source JSONL snapshot without connecting to the database
    #[arg(long, conflicts_with = "profile")]
    input: Option<std::path::PathBuf>,
    #[arg(long)]
    output: Option<std::path::PathBuf>,
    /// Export source inputs for reproducible offline replay
    #[arg(long)]
    snapshot_out: Option<std::path::PathBuf>,
    /// Export per-game validation and player TOI details (JSONL)
    #[arg(long)]
    games_out: Option<std::path::PathBuf>,
    /// Export every reconstructed event (JSONL)
    #[arg(long)]
    events_out: Option<std::path::PathBuf>,
    /// Include EXPLAIN ANALYZE for the first batch of source reads
    #[arg(long)]
    profile: bool,
}

#[derive(Args)]
struct ShiftBackfillArgs {
    /// NHL season in eight-digit form (e.g. 20252026)
    #[arg(long)]
    season: i32,

    /// Re-fetch and atomically replace every game in this season
    #[arg(long)]
    refresh: bool,
}

#[derive(Subcommand)]
enum FetchEntity {
    /// Fetch all NHL teams
    Teams,
    /// Fetch all NHL players
    Players,
    /// Fetch all NHL seasons
    Seasons,
    /// Fetch NHL games
    Games(GamesArgs),
    /// Fetch play-by-play events for a game
    Events(EventsArgs),
    /// Fetch official NHL season totals for skaters and goalies
    OfficialStats(OfficialStatsArgs),
    /// Fetch official final-boxscore player statistics
    OfficialGameStats(OfficialGameStatsArgs),
}

#[derive(Args)]
struct OfficialGameStatsArgs {
    /// Load one completed game by NHL game ID
    #[arg(long, conflicts_with = "from")]
    game: Option<i64>,
    /// Load completed games on or after this date (YYYY-MM-DD)
    #[arg(long, conflicts_with = "game")]
    from: Option<String>,
    /// Stop a date-range load on this date, inclusive
    #[arg(long, requires = "from")]
    to: Option<String>,
}

fn parse_date(value: Option<String>) -> Result<Option<time::Date>, pucksdata::AnyError> {
    value
        .map(|value| {
            time::Date::parse(
                &value,
                time::macros::format_description!("[year]-[month]-[day]"),
            )
            .map_err(Into::into)
        })
        .transpose()
}

#[derive(Args)]
struct OfficialStatsArgs {
    /// Restrict the load to a single season (e.g. 20242025)
    #[arg(long)]
    season: Option<i32>,
}

#[derive(Args)]
struct EventsArgs {
    /// Game ID to fetch play-by-play events for
    game_id: i64,
}

#[derive(Args)]
struct GamesArgs {
    #[command(flatten)]
    scope: GamesScope,
}

#[derive(Args)]
struct BackfillArgs {
    /// Restrict backfill to a single season (e.g. 20232024)
    #[arg(long)]
    season: Option<i32>,

    /// Re-fetch and atomically replace games already marked done
    #[arg(long, requires = "season")]
    refresh: bool,
}

#[derive(Args)]
struct SyncArgs {
    /// Override gap detection: re-process all completed games on or after this date (YYYY-MM-DD).
    /// Without this flag, the sync watermark is derived structurally from the database.
    #[arg(long, value_name = "DATE")]
    from: Option<String>,
}

#[derive(Args)]
struct DaemonArgs {
    /// Sync interval in seconds (default: 21600 = 6 hours).
    /// Also read from SYNC_INTERVAL_SECS env var if flag absent.
    #[arg(long, value_name = "SECS")]
    interval_secs: Option<u64>,

    /// Run a full backfill before entering the sync loop.
    #[arg(long)]
    backfill_on_start: bool,
}

#[derive(Args)]
struct StatusArgs {
    /// Restrict status output to a single season (e.g. 20252026)
    #[arg(long)]
    season: Option<i32>,

    /// Fetch game metadata and run backfill to remediate coverage gaps
    #[arg(long)]
    fix: bool,

    /// Emit the health report as JSON
    #[arg(long, conflicts_with = "fix")]
    json: bool,

    /// Return success after producing an unhealthy report
    #[arg(long, requires = "json")]
    no_fail: bool,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct GamesScope {
    /// Fetch a single game by ID
    #[arg(long)]
    game: Option<i64>,
    /// Fetch all games for a season (e.g. 20232024)
    #[arg(long)]
    season: Option<i32>,
    /// Fetch all games across all seasons
    #[arg(long)]
    all: bool,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    if let Err(error) = pucksdata::logging::init() {
        eprintln!("Cannot initialize logging: {error}");
        std::process::exit(1);
    }
    if let Err(error) = run(cli).await {
        tracing::error!(error = %error, "command failed");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), pucksdata::AnyError> {
    let capture = matches!(
        &cli.command,
        Commands::RefreshDerived
            | Commands::Fetch { .. }
            | Commands::Backfill(_)
            | Commands::Shifts {
                command: ShiftsCommand::Backfill(_)
            }
            | Commands::Status(StatusArgs { fix: true, .. })
    );
    // Boxing keeps the large dispatcher future off the entry-point stack.
    if capture {
        let pool = db::get_pool().await?;
        let key = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
        pucksdata::process::attempts::command(pool, &key, Box::pin(dispatch(cli.command))).await
    } else {
        Box::pin(dispatch(cli.command)).await
    }
}

async fn dispatch(command: Commands) -> Result<(), pucksdata::AnyError> {
    match command {
        Commands::RefreshDerived => {
            pucksdata::process::analytics::refresh_derived(db::get_pool().await?).await?;
        }
        Commands::Fetch { entity } => match entity {
            FetchEntity::Teams => {
                let pool = db::get_pool().await?;
                let records = fetchers::teams::fetch_teams().await?;
                let count = records.len();
                let pb = pucksdata::ui::make_progress_bar(count as u64, "teams");
                loaders::teams::upsert_teams(pool, &records, &pb)
                    .await
                    .inspect_err(|_| pb.finish_and_clear())?;
                pb.finish_and_clear();
                // The writer preflight already refreshed source team identities.
            }
            FetchEntity::OfficialStats(args) => {
                let pool = db::get_pool().await?;
                let summary =
                    pucksdata::process::official_stats::run_official_stats(pool, args.season)
                        .await?;
                if summary.failures > 0 {
                    return Err(format!("{} official season loads failed", summary.failures).into());
                }
            }
            FetchEntity::OfficialGameStats(args) => {
                let pool = db::get_pool().await?;
                let summary = pucksdata::process::official_games::run_official_games(
                    pool,
                    args.game,
                    parse_date(args.from)?,
                    parse_date(args.to)?,
                )
                .await?;
                println!(
                    "Wrote official stats for {} games ({} skaters, {} goalies)",
                    summary.games, summary.skaters, summary.goalies
                );
            }
            FetchEntity::Seasons => {
                let pool = db::get_pool().await?;
                let records = fetchers::seasons::fetch_seasons().await?;
                let count = records.len();
                let pb = pucksdata::ui::make_progress_bar(count as u64, "seasons");
                loaders::seasons::upsert_seasons(pool, &records, &pb)
                    .await
                    .inspect_err(|_| pb.finish_and_clear())?;
                pb.finish_and_clear();
            }
            FetchEntity::Players => {
                pucksdata::process::players::refresh(db::get_pool().await?).await?;
            }
            FetchEntity::Events(args) => {
                let pool = db::get_pool().await?;
                let team_id_map = fetchers::games::fetch_team_id_to_franchise_id_map().await?;
                let count =
                    pucksdata::process::backfill::load_one_game(pool, args.game_id, &team_id_map)
                        .await?;
                pucksdata::process::analytics::refresh_derived(pool).await?;
                println!("game {}: {count} events", args.game_id);
            }
            FetchEntity::Games(args) => {
                let pool = db::get_pool().await?;

                if let Some(game_id) = args.scope.game {
                    let game = fetchers::games::fetch_single_game(game_id).await?;
                    loaders::games::upsert_games(pool, &[game], &indicatif::ProgressBar::hidden())
                        .await?;
                    println!("Fetched 1 record, upserted 1");
                } else if let Some(season) = args.scope.season {
                    let pb_fetch = pucksdata::ui::make_progress_bar(0, "games fetched");
                    let games =
                        fetchers::games::fetch_games_for_season_enriched(season, &pb_fetch).await?;
                    let count = games.len();
                    pb_fetch.finish_and_clear();

                    let pb_upsert = pucksdata::ui::make_progress_bar(count as u64, "games written");
                    loaders::games::upsert_games(pool, &games, &pb_upsert)
                        .await
                        .inspect_err(|_| pb_upsert.finish_and_clear())?;
                    pb_upsert.finish_and_clear();
                } else {
                    pucksdata::process::games::refresh_all(pool).await?;
                }
            }
        },
        Commands::Backfill(args) => {
            let pool = db::get_pool().await?;
            pucksdata::process::backfill::run_backfill_with_refresh(
                pool,
                args.season,
                args.refresh,
            )
            .await?;
        }
        Commands::Sync(args) => {
            let pool = db::get_pool().await?;
            let from_date = parse_date(args.from)?;
            pucksdata::process::sync::run_sync(pool, from_date).await?;
        }
        Commands::Daemon(args) => {
            let pool = db::get_pool().await?;
            let interval_secs = args
                .interval_secs
                .or_else(|| {
                    std::env::var("SYNC_INTERVAL_SECS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(21600); // 6 hours default
            pucksdata::process::daemon::run_daemon(pool, interval_secs, args.backfill_on_start)
                .await?;
        }
        Commands::Status(args) => {
            let pool = db::get_pool().await?;
            let healthy = if args.json {
                pucksdata::process::status::run_status_json(pool, args.season).await?
            } else {
                pucksdata::process::status::run_status(pool, args.season, args.fix).await?
            };
            if !healthy && !args.no_fail {
                return Err("dataset health requires attention".into());
            }
        }
        Commands::Shifts { command } => {
            let args = match command {
                ShiftsCommand::Audit(args) => {
                    let pool = if args.input.is_none() {
                        Some(db::get_pool().await?)
                    } else {
                        None
                    };
                    let report = pucksdata::on_ice::audit::run(
                        pool,
                        pucksdata::on_ice::audit::AuditOptions {
                            season: args.season,
                            input: args.input,
                            snapshot_out: args.snapshot_out,
                            games_out: args.games_out,
                            events_out: args.events_out,
                            profile: args.profile,
                        },
                    )
                    .await?;
                    pucksdata::on_ice::audit::write_report(args.output.as_deref(), &report)?;
                    return Ok(());
                }
                ShiftsCommand::Reconstruct { game, output } => {
                    let report =
                        pucksdata::on_ice::audit::game(db::get_pool().await?, game).await?;
                    pucksdata::on_ice::audit::write_report(output.as_deref(), &report)?;
                    return Ok(());
                }
                ShiftsCommand::Backfill(args) => args,
            };
            let pool = db::get_pool().await?;
            db::require_connections(pool, 2, "shift backfill")?;
            let summary =
                pucksdata::process::shifts::run_backfill(pool, args.season, args.refresh).await?;
            println!(
                "raw shift load: {} candidates, {} attempted, {} succeeded, {} unavailable, {} failed, {} shifts",
                summary.candidates,
                summary.attempted,
                summary.succeeded,
                summary.unavailable_games.len(),
                summary.failed,
                summary.shifts,
            );
            if !summary.unavailable_games.is_empty() {
                tracing::warn!(
                    "NHL shift feed returned no shift rows for games: {}",
                    summary
                        .unavailable_games
                        .iter()
                        .map(i64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                tracing::warn!(
                    "unavailable games were not replaced; games without stored shifts will be retried on the next run"
                );
            }
            for failure in summary.failures.iter().take(25) {
                tracing::warn!("{failure}");
            }
            if summary.failures.len() > 25 {
                tracing::warn!("... and {} more failures", summary.failures.len() - 25);
            }
            if summary.stopped_early {
                tracing::warn!(
                    "stopped after an upstream timeout or server error; rerun later to resume"
                );
            }
            if summary.failed > 0 {
                return Err(format!("{} shift games failed", summary.failed).into());
            }
        }
    }

    Ok(())
}
