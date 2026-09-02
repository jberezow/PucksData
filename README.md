# PucksData

[![CI](https://github.com/jberezow/pucksdata/actions/workflows/ci.yml/badge.svg?branch=prime)](https://github.com/jberezow/pucksdata/actions/workflows/ci.yml)
[![NHL API canary](https://github.com/jberezow/pucksdata/actions/workflows/canary.yml/badge.svg?branch=prime)](https://github.com/jberezow/pucksdata/actions/workflows/canary.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)

PucksData is a production-oriented Rust ETL engine that fetches NHL play-by-play data, normalizes it into PostgreSQL, and keeps it current through one-shot syncs or a long-running daemon.

It is designed as the data foundation for hockey analytics, machine-learning experiments, and fantasy applications. PucksData owns ingestion, normalization, and data quality; downstream analysis and presentation remain separate concerns.

## Architecture

```mermaid
flowchart LR
    NHL[Unofficial NHL APIs] --> Fetch[Concurrent Rust fetchers]
    Fetch --> Normalize[Typed normalization]
    Normalize --> PG[(PostgreSQL)]
    CLI[CLI] --> Fetch
    CLI --> Ops[Backfill · Sync · Daemon]
    Ops --> PG
    Health[Status and repair] --> PG
```

The pipeline provides:

- Teams, seasons, players, games, and play-by-play metadata
- Typed tables for goals, shots, hits, blocks, penalties, and faceoffs
- Idempotent bulk upserts and transactional event writes
- Resumable historical backfills with per-game progress tracking
- Incremental completed-game synchronization
- Scheduled daemon mode with advisory locking and graceful shutdown
- Per-season health reporting and automated gap remediation
- SQLx offline metadata and a non-root Docker runtime

## Validated dataset

A populated PucksData database was audited in August 2026 against the NHL API:

- Complete 2025–26 NHL club inventory: 104 preseason, 1,312 regular-season, and 105 playoff records
- 100% event coverage for every completed 2025–26 regular-season and playoff game
- 470,142 play-by-play events across 1,498 played club games
- Exact reconciliation between event types and typed child tables
- Zero goals missing their corresponding shots row

The API also lists 30 `game_type = 9` international games for February 2026. Those national-team games are intentionally outside the NHL-franchise schema.

## Prerequisites

- A stable [Rust toolchain](https://rustup.rs/)
- PostgreSQL 14 or newer
- [`sqlx-cli`](https://crates.io/crates/sqlx-cli) for applying migrations

Install the migration CLI with PostgreSQL support:

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

## Quick start

Clone the repository:

```bash
git clone https://github.com/jberezow/pucksdata.git
cd pucksdata
```

Create `.env` from the template and provide a PostgreSQL connection string:

```bash
cp .env.example .env
```

```dotenv
DATABASE_URL=postgresql://user:password@host/database?sslmode=require
SYNC_INTERVAL_SECS=21600
```

Apply the schema and build the binary:

```bash
sqlx migrate run
cargo build --release
```

Initialize the database before the first historical backfill:

```bash
cargo run --release -- fetch teams
cargo run --release -- fetch seasons
cargo run --release -- fetch games --all
cargo run --release -- fetch players
cargo run --release -- backfill
```

These commands are idempotent and can be restarted safely. Install the binary with `cargo install --path .` if you prefer to invoke `pucksdata` directly.

## Commands

Run `pucksdata --help` or `pucksdata <COMMAND> --help` for the generated command reference.

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

Season values use the NHL's eight-digit format:

```bash
pucksdata fetch games --season 20252026
pucksdata fetch events 2025020001
```

### `backfill`

Process historical games through the checkpointed event-ingestion pipeline. Completed games are not duplicated when the command is restarted.

```bash
pucksdata backfill
pucksdata backfill --season 20252026
```

### `sync`

Refresh entity metadata, find completed games without events, and ingest the gaps:

```bash
pucksdata sync
pucksdata sync --from 2026-01-01
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
pucksdata status --season 20252026
pucksdata status --json
```

Use `--fix` only after reviewing the read-only report:

```bash
pucksdata status --season 20252026 --fix
```

## Seasonal operation

PucksData does not need to run continuously during the offseason.

1. During September, load the upcoming schedule explicitly because automatic season rollover occurs in October:

   ```bash
   pucksdata fetch games --season 20262027
   ```

2. Run the daemon during the season. Six-hour intervals suit current-data applications; daily syncs are sufficient for general analysis.
3. After the Stanley Cup Final, run one final sync and health check:

   ```bash
   pucksdata sync
   pucksdata status --season 20262027
   ```

4. Stop the daemon when live updates are no longer needed.

## Docker

Build the multi-stage production image:

```bash
docker build -t pucksdata .
```

The default container command starts the daemon:

```bash
docker run --rm --env-file .env pucksdata
```

Override it for one-shot operation:

```bash
docker run --rm --env-file .env pucksdata sync
docker run --rm --env-file .env pucksdata status
```

Migrations are not run automatically by the runtime image; apply them before starting the container.

## Development

The repository stores SQLx query metadata in `.sqlx/` and enables `SQLX_OFFLINE=true`, so compilation does not require a live database.

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo test --all-targets
```

Database-backed tests use `TEST_DATABASE_URL` exclusively and skip when it is
unset. They never fall back to the application `DATABASE_URL`. The target
database name must contain `test` unless the explicit
`PUCKSDATA_ALLOW_UNSAFE_TEST_DATABASE=1` override is set.

Run the complete suite against a disposable local PostgreSQL container:

```bash
./scripts/test-database.sh
```

The script starts PostgreSQL on port `55432`, applies every migration, runs all
tests, and removes the container and its temporary storage even when a test
fails. CI uses the same isolated-database approach and never receives production
credentials.

The daily `NHL API and ingestion canary` separately exercises the live NHL season endpoints, validates their response shape, and writes the results to disposable PostgreSQL. It can also be started manually from the Actions tab and never connects to a production database.

The `Scheduled database sync` workflow runs `sync` against the configured database each day and can also be started manually. It publishes a concise job summary and retains the JSON health report as a short-lived workflow artifact. The workflow requires a repository Actions secret named `DATABASE_URL` containing an ingestion-role connection string.

## Data model

The migrations create:

- Entity tables: `teams`, `seasons`, `players`, and `games`
- A shared `events` parent table
- Event detail tables: `goals`, `shots`, `hits`, `blocks`, `penalties`, and `faceoffs`
- Operational tables: `backfill_progress` and `sync_state`
- Read-only health views in the `observability` schema

Goals are also represented in `shots`, so the shots table covers every shot on net. Ingestion uses upsert semantics throughout and is designed to recover safely after partial failures.

## Scope and limitations

- The NHL APIs are public but unofficial and unversioned. Historical seasons, especially pre-2010 data, can contain structural gaps.
- International and national-team competitions are outside the NHL-franchise schema.
- Live-game polling is not implemented; synchronization targets completed games.
- Derived metrics such as expected goals, WAR, and fantasy scoring belong in downstream consumers.

## License

PucksData is available under the [MIT License](LICENSE).
