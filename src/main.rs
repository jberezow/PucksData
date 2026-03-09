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
    /// Fetch and upsert NHL entity metadata
    Fetch {
        #[command(subcommand)]
        entity: FetchEntity,
    },
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
async fn main() -> Result<(), pucksdata::AnyError> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    match cli.command {
        Commands::Fetch { entity } => match entity {
            FetchEntity::Teams => {
                let pool = db::get_pool().await?;
                let records = fetchers::teams::fetch_teams().await?;
                let count = records.len();
                loaders::teams::upsert_teams(pool, &records).await?;
                println!("Fetched {count} records, upserted {count}");
            }
            FetchEntity::Seasons => {
                let pool = db::get_pool().await?;
                let records = fetchers::seasons::fetch_seasons().await?;
                let count = records.len();
                loaders::seasons::upsert_seasons(pool, &records).await?;
                println!("Fetched {count} records, upserted {count}");
            }
            FetchEntity::Players => {
                let pool = db::get_pool().await?;
                let records = fetchers::players::fetch_players().await?;
                let count = records.len();
                loaders::players::upsert_players(pool, &records).await?;
                println!("Fetched {count} records, upserted {count}");
            }
            FetchEntity::Events(args) => {
                use indicatif::{ProgressBar, ProgressStyle};
                let pool = db::get_pool().await?;
                let pb = ProgressBar::new(3);
                pb.set_style(
                    ProgressStyle::with_template(
                        "[{elapsed_precise}] [{bar:20.cyan/blue}] {pos}/{len} {msg}"
                    )
                    .unwrap()
                    .progress_chars("=>-"),
                );

                pb.set_message("fetching team ID map...");
                let team_id_map = fetchers::games::fetch_team_id_to_franchise_id_map().await?;
                pb.inc(1);

                pb.set_message(format!("fetching play-by-play for game {}...", args.game_id));
                let pbp = fetchers::events::fetch_play_by_play(args.game_id).await?;
                pb.inc(1);

                pb.set_message("transforming and loading events...");
                let (events, goals, shots, hits, blocks, penalties, faceoffs, skip_warnings) =
                    fetchers::events::transform_events(&pbp, &team_id_map);
                for warning in &skip_warnings {
                    pb.suspend(|| eprintln!("{warning}"));
                }
                let (ec, gc, sc, hc, bc, pc, fc) = loaders::events::upsert_game_events(
                    pool,
                    args.game_id,
                    &events,
                    &goals,
                    &shots,
                    &hits,
                    &blocks,
                    &penalties,
                    &faceoffs,
                ).await?;
                pb.inc(1);
                pb.finish_with_message(format!(
                    "game {}: {} events, {} goals, {} shots, {} hits, {} blocks, {} penalties, {} faceoffs",
                    args.game_id, ec, gc, sc, hc, bc, pc, fc
                ));
            }
            FetchEntity::Games(args) => {
                use indicatif::{ProgressBar, ProgressStyle};
                let pool = db::get_pool().await?;

                if let Some(game_id) = args.scope.game {
                    // --game <id>: single game fetch
                    let game = fetchers::games::fetch_single_game(game_id).await?;
                    loaders::games::upsert_games(pool, &[game]).await?;
                    println!("Fetched 1 record, upserted 1");

                } else if let Some(season) = args.scope.season {
                    // --season <year>: all games for one season
                    let pb = ProgressBar::new(0);
                    pb.set_style(
                        ProgressStyle::with_template(
                            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}"
                        )
                        .unwrap()
                        .progress_chars("=>-"),
                    );
                    let games = fetchers::games::fetch_games_for_season_enriched(season, &pb).await;
                    let count = games.len();
                    loaders::games::upsert_games(pool, &games).await?;
                    pb.finish_with_message(format!("Fetched {count} records, upserted {count}"));

                } else {
                    // --all: iterate all seasons sequentially
                    let seasons = fetchers::games::fetch_seasons_list().await?;
                    let total_seasons = seasons.len();
                    let mut total_games = 0usize;

                    let pb = ProgressBar::new(0);
                    pb.set_style(
                        ProgressStyle::with_template(
                            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}"
                        )
                        .unwrap()
                        .progress_chars("=>-"),
                    );

                    for (i, season) in seasons.iter().enumerate() {
                        pb.println(format!(
                            "[{}/{}] Fetching season {}...",
                            i + 1, total_seasons, season
                        ));
                        pb.set_position(0);
                        let games = fetchers::games::fetch_games_for_season_enriched(*season, &pb).await;
                        let count = games.len();
                        if count > 0 {
                            loaders::games::upsert_games(pool, &games).await?;
                        }
                        total_games += count;
                    }
                    pb.finish_with_message(format!(
                        "Fetched {total_games} total games across {total_seasons} seasons, upserted {total_games}"
                    ));
                }
            }
        },
    }

    Ok(())
}
