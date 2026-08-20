# PucksData

PucksData is a Rust ETL engine that fetches NHL play-by-play data, normalizes it into PostgreSQL, and keeps it current through one-shot syncs or a long-running daemon.

It is intended for developers building hockey analytics, machine-learning, or fantasy applications on top of a clean relational dataset. PucksData owns ingestion and data quality; downstream analysis and presentation remain separate concerns.

## Features

- Teams, seasons, players, games, and play-by-play metadata
- Typed tables for goals, shots, hits, blocks, penalties, and faceoffs
- Idempotent PostgreSQL upserts
- Resumable historical backfills with per-game progress tracking
- Incremental completed-game synchronization
- Scheduled daemon mode with single-instance locking and graceful shutdown
- Per-season health reporting and automated gap remediation
- Docker deployment with a non-root runtime image

## Prerequisites

- A stable [Rust toolchain](https://rustup.rs/)
- PostgreSQL; Neon, a local server, and containerized PostgreSQL are supported
- [`sqlx-cli`](https://crates.io/crates/sqlx-cli) for applying migrations

Install the migration CLI with only PostgreSQL support:

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

On Ubuntu or WSL2, compilation also requires the standard native build packages:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev
```

## Setup

Clone and build the project:

```bash
git clone https://github.com/jberezow/pucksdata.git
cd pucksdata
cargo build --release
```

Copy the environment template and set your PostgreSQL connection string:

```bash
cp .env.example .env
```

```dotenv
DATABASE_URL=postgresql://user:password@host/database?sslmode=require
SYNC_INTERVAL_SECS=21600
```

Use a direct database connection rather than a transaction-pooler endpoint when applying migrations or regenerating SQLx's offline query cache.

Apply the schema:

```bash
sqlx migrate run
```

The compiled executable is `target/release/pucksdata`. To install it on your current Cargo path instead, run:

```bash
cargo install --path .
```

Seed the entity tables before running a historical backfill:

```bash
pucksdata fetch teams
pucksdata fetch seasons
pucksdata fetch games --all
pucksdata fetch players
```

Then load missing events:

```bash
pucksdata sync
```

## Commands

Run `pucksdata --help` or `pucksdata <COMMAND> --help` for the complete generated command reference.

### `fetch`

Fetch and upsert NHL entity or play-by-play data.

| Command | Description |
|---|---|
| `fetch teams` | Fetch all NHL franchise records |
| `fetch seasons` | Fetch all available NHL seasons |
| `fetch players` | Discover players from rosters and season statistics, then fetch their landing pages |
| `fetch games --game <ID>` | Fetch one game's metadata |
| `fetch games --season <YEAR>` | Fetch all games in one season |
| `fetch games --all` | Fetch games across every available season |
| `fetch events <GAME_ID>` | Fetch and store one game's play-by-play events |

Season values use the NHL's eight-digit format, such as `20242025`:

```bash
pucksdata fetch games --season 20242025
pucksdata fetch events 2024020001
```

### `backfill`

Process historical games through the checkpointed event-ingestion pipeline. Completed games are not duplicated when the command is restarted.

```bash
pucksdata backfill
pucksdata backfill --season 20232024
```

Entity tables must already be populated.

### `sync`

Refresh entity metadata, find completed games without events, and ingest the gaps:

```bash
pucksdata sync
```

Use `--from` to reprocess completed games on or after a date instead of relying solely on structural gap detection:

```bash
pucksdata sync --from 2025-01-15
```

### `daemon`

Run synchronization immediately and then on a fixed interval. The default interval is six hours and can be changed with a flag or `SYNC_INTERVAL_SECS`.

```bash
pucksdata daemon
pucksdata daemon --interval-secs 3600
pucksdata daemon --backfill-on-start
```

Only one daemon can hold the PostgreSQL advisory lock at a time. SIGTERM and Ctrl-C abort the current idempotent operation and exit cleanly.

### `status`

Report game counts, event coverage, goals-in-shots consistency, and backfill state by season. An unhealthy result exits with status code 1, making this command suitable for monitoring.

```bash
pucksdata status
pucksdata status --season 20242025
```

Use `--fix` to refresh game metadata and backfill unhealthy seasons:

```bash
pucksdata status --fix
pucksdata status --season 20242025 --fix
```

## Docker

Build the multi-stage production image:

```bash
docker build -t pucksdata .
```

The default container command starts the daemon:

```bash
docker run --rm --env-file .env pucksdata
```

Override the command for one-shot operation:

```bash
docker run --rm --env-file .env pucksdata sync
docker run --rm --env-file .env pucksdata status
```

Migrations are not run automatically by the runtime image; apply them before starting the container.

## Development

The repository stores SQLx query metadata in `.sqlx/` and enables `SQLX_OFFLINE=true` in `.cargo/config.toml`, so compilation does not require a live database.

Run the local quality checks with:

```bash
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Database-backed tests return early when `DATABASE_URL` is unset. Set it to a disposable migrated database to exercise the integration paths; several tests create and remove records.

## Data model

The migrations create:

- Entity tables: `teams`, `seasons`, `players`, and `games`
- A shared `events` parent table
- Event detail tables: `goals`, `shots`, `hits`, `blocks`, `penalties`, and `faceoffs`
- Operational tables: `backfill_progress` and `sync_state`

Goals are also represented in `shots`, so the shots table covers all shots on net. Ingestion uses upsert semantics throughout and is designed to be safely rerun after partial failures.

## Operational notes

- The NHL API is public but unofficial and unversioned. Historical seasons, especially pre-2010 data, can contain structural gaps that the backfill pipeline records and skips.
- Progress bars are written to stderr and per-game operational logs to stdout, allowing clean redirection of backfill logs.
- The default database pool is capped at five connections and tuned for serverless PostgreSQL suspension behavior.
- Live-game polling is not currently implemented; synchronization targets completed games.

## License

MIT
