# PucksData

NHL Data ETL Engine — fetches play-by-play from the NHL Stats API, normalizes it into PostgreSQL, and keeps it current via daemon or one-shot sync.

## What It Does

PucksData fetches NHL game data (teams, seasons, players, games, and play-by-play events) from the public NHL Stats API, transforms the nested JSON into a clean relational schema, and loads it into PostgreSQL. It supports historical backfill across all seasons, incremental daily sync, and a long-running daemon mode. A health-check command provides per-season coverage reporting and automated remediation.

## Who It's For

Developers building hockey analytics applications on top of a clean, complete relational database of NHL play-by-play data.

## Prerequisites

- **Rust** (stable toolchain) — `rustup` recommended
- **PostgreSQL** — any instance (local, Docker, Neon, etc.)
- **sqlx-cli** — for running migrations: `cargo install sqlx-cli --no-default-features --features postgres`

## Setup

1. Clone the repo:
   ```bash
   git clone https://github.com/your-org/pucksdata.git
   cd pucksdata
   ```

2. Create a `.env` file with your database connection string:
   ```
   DATABASE_URL=postgresql://user:pass@host/dbname
   ```

3. Run migrations:
   ```bash
   sqlx migrate run
   ```

4. Seed entity tables (order matters):
   ```bash
   pucksdata fetch teams
   pucksdata fetch seasons
   pucksdata fetch players
   pucksdata fetch games --all
   ```

5. Run your first sync:
   ```bash
   pucksdata sync
   ```

## Commands

### `fetch`

Fetch and upsert NHL entity metadata.

| Subcommand | Flags | Description |
|------------|-------|-------------|
| `fetch teams` | (none) | Fetch all NHL franchise records |
| `fetch seasons` | (none) | Fetch all NHL season records |
| `fetch players` | (none) | Enumerate and fetch player landing pages for all seasons |
| `fetch games` | `--game <ID>`, `--season <YEAR>`, `--all` (mutually exclusive, one required) | Fetch game metadata |
| `fetch events` | `<GAME_ID>` (positional) | Fetch play-by-play events for a single game |

```bash
pucksdata fetch games --season 20242025
pucksdata fetch events 2024020001
```

### `backfill`

Run full historical event ingestion via the checkpoint table. Processes all games that have metadata but no events.

| Flag | Description |
|------|-------------|
| `--season <YEAR>` | Restrict to a single season (e.g., 20232024) |

```bash
pucksdata backfill
pucksdata backfill --season 20232024
```

### `sync`

Incremental sync: finds completed games with no events and loads play-by-play.

| Flag | Description |
|------|-------------|
| `--from <YYYY-MM-DD>` | Override gap detection: re-process all completed games on or after this date |

```bash
pucksdata sync
pucksdata sync --from 2025-01-15
```

### `daemon`

Long-lived process that runs sync on a configurable interval. Handles SIGTERM/Ctrl-C for graceful shutdown.

| Flag | Description |
|------|-------------|
| `--interval-secs <N>` | Sync interval in seconds (default: 21600 = 6 hours). Also reads `SYNC_INTERVAL_SECS` env var. |
| `--backfill-on-start` | Run a full backfill before entering the sync loop |

```bash
pucksdata daemon --interval-secs 3600 --backfill-on-start
```

### `status`

Per-season health check: game counts, event coverage percentage, goals-in-shots consistency, backfill status. Exits with code 1 if any season is unhealthy.

| Flag | Description |
|------|-------------|
| `--season <YEAR>` | Restrict to a single season |
| `--fix` | Fetch game metadata and run backfill to remediate coverage gaps |

```bash
pucksdata status
pucksdata status --season 20242025 --fix
```

## Docker

A Dockerfile is included for containerized deployment:

```bash
docker build -t pucksdata .
docker run --rm -e DATABASE_URL="$DATABASE_URL" pucksdata sync
```

## License

MIT
