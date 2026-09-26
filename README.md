<h1 align="center">
  <img src="docs/assets/pucksdata-logo.png" alt="PucksData" width="900">
</h1>

<p align="center">
  <a href="https://github.com/jberezow/pucksdata/actions/workflows/ci.yml"><img src="https://github.com/jberezow/pucksdata/actions/workflows/ci.yml/badge.svg?branch=prime" alt="CI status"></a>
  <a href="https://github.com/jberezow/pucksdata/actions/workflows/canary.yml"><img src="https://github.com/jberezow/pucksdata/actions/workflows/canary.yml/badge.svg?branch=prime" alt="NHL API canary status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="MIT License"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2021-orange.svg" alt="Rust 2021"></a>
</p>

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

- Teams, seasons, player identity and headshot metadata, current roster snapshots,
  games, and play-by-play metadata
- Typed tables for goals, shots, hits, blocks, penalties, and faceoffs
- Typed NHL player-shift rows, loaded one season at a time
- Idempotent bulk upserts and transactional event writes
- Resumable historical backfills with per-game progress tracking
- Completed-game gap repair and recent correction audits
- Prospective source capture, normalized revisions, and recorded ingestion outcomes
- Scheduled daemon mode with advisory locking and graceful shutdown
- Per-season health reporting and automated gap remediation
- SQLx offline metadata and a non-root Docker runtime

## Coverage

Coverage depends on the statistic, season, and source. Scheduled game inventory
includes unplayed games, so it is not a denominator for completed-game event
coverage. Use `pucksdata status` and `analytics.coverage_observed` against your
database for current counts; consult `analytics.coverage` for known source limits.
International and national-team games are outside the NHL-franchise schema.

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
MIGRATION_DATABASE_URL=postgresql://owner:password@host/database?sslmode=require
SYNC_INTERVAL_SECS=21600
```

`MIGRATION_DATABASE_URL` may use the same connection string as `DATABASE_URL`
for local development. Hosted deployments should use a schema-owner connection
for migrations and a restricted ingestion connection at runtime.

For existing deployments, follow the [ingestion history rollout](docs/ingestion-history.md#rollout)
and the [incremental sync upgrade](docs/ingestion-history.md#incremental-sync-upgrade)
before starting the updated runtime. For a new database, apply the schema and build:

```bash
./scripts/run-migrations.sh
cargo build --release
```

When upgrading a populated database from migration 0020 or earlier, stage the
event-scope migration so the large update commits in batches:

```bash
./scripts/run-migrations.sh --target-version 21
./scripts/run-event-scope-backfill.sh
./scripts/run-migrations.sh
```

The migration wrapper uses the owner-level `MIGRATION_DATABASE_URL`; the
backfill wrapper uses `DATABASE_URL`. Both load their value directly from
`.env` when it is not already exported, without sourcing the file. This matters
when a connection URL contains shell metacharacters such as `&`.

Deploy the updated ingestion binary before the final command, and pause older
ingestion processes while the backfill and final migrations run. Fresh databases
can use a single `sqlx migrate run` because there are no existing events to fill.

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
| `fetch players` | Discover players from rosters and season statistics, fetch their landing pages, and persist a complete current-roster snapshot |
| `fetch games --game <ID>` | Fetch one game's metadata |
| `fetch games --season <YEAR>` | Fetch all games in one season |
| `fetch games --all` | Fetch games across every available season |
| `fetch events <GAME_ID>` | Fetch and store one game's play-by-play events |
| `fetch official-stats` | Fetch official NHL skater and goalie season totals for every season |
| `fetch official-stats --season <YEAR>` | Fetch official season totals for one season |
| `fetch official-game-stats --game <ID>` | Fetch final official player statistics for one completed game |
| `fetch official-game-stats --from <DATE> [--to <DATE>]` | Load or audit completed games in an inclusive date range |

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
pucksdata backfill --season 20052006 --refresh
```

Use `--refresh` to re-fetch a complete season and atomically replace previously
ingested event snapshots. A season is required for this operation.

### `sync`

Refresh entity metadata, fill completed-game event gaps, and re-fetch recent
events and official player/game statistics for corrections:

```bash
pucksdata sync
pucksdata sync --from 2026-01-01
```

Both one-shot sync and the daemon use a trailing three-day correction window,
expanded to fourteen days on Sundays (UTC). `PUCKSDATA_CORRECTION_DAYS` overrides
this with a value from 1 to 366. `--from` explicitly replays eligible completed
games from that date, including games that already have events. Failed event
attempts remain eligible for retry. Partial or failed syncs exit unsuccessfully
and do not advance the last-success timestamp.

See [ingestion history](docs/ingestion-history.md) for rollout, freshness queries,
historical snapshots, and the downstream correction contract.

### `daemon`

Run synchronization immediately and then on a fixed interval. The default interval is six hours and can be changed with a flag or `SYNC_INTERVAL_SECS`.

```bash
pucksdata daemon
pucksdata daemon --interval-secs 3600
pucksdata daemon --backfill-on-start
```

Only one daemon can hold its PostgreSQL advisory lock at a time. All mutating
commands also share a pooler-safe writer lease through fetch and commit. Runtime
pools need at least two connections for CLI writes and three for the daemon
(default: five). SIGTERM and Ctrl-C abort the current idempotent operation and
exit cleanly.

### `status`

Report game counts, event coverage, goals-in-shots consistency, backfill state,
and recent ingestion issues. An unhealthy result exits with status code 1, making this command suitable for monitoring.

```bash
pucksdata status
pucksdata status --season 20252026
pucksdata status --json
```

Use `--json --no-fail` when an unhealthy report should remain informational,
such as in the scheduled synchronization workflow.

Use `--fix` only after reviewing the read-only report:

```bash
pucksdata status --season 20252026 --fix
```

### `shifts`

Shift ingestion is intentionally season-scoped. The NHL JSON shift feed begins
in 2010–11 and includes non-shift goal annotations; PucksData stores only its
`typeCode = 517` rows in `public.shifts`. Fields are converted to typed columns,
but ingestion does not correct,
deduplicate, translate, or classify the intervals.

Load one season:

```bash
pucksdata shifts backfill --season 20252026
```

Re-fetch and atomically replace every game in the season:

```bash
pucksdata shifts backfill --season 20252026 --refresh
```

The shift backfill does not run from the normal event daemon during its pilot.
Without `--refresh`, rerunning it resumes at eligible games with no shift rows.
Valid responses with no shift rows are reported separately as `unavailable`,
not as successful loads or failures. They never replace stored rows, and games
without stored shifts remain eligible for retry on the next run. This covers
gaps in the NHL JSON feed, such as the observed gap for games
2024021235–2024021291. A run containing only unavailable games exits successfully;
malformed/incomplete responses and request/database errors still fail the run.
Shift ingestion uses only the JSON feed.

Backfills load up to five games concurrently and stop starting new requests
after an upstream timeout, rate limit, or server error. A transaction-scoped
advisory lock prevents overlapping backfills through transaction poolers;
heartbeats maintain it during ingestion. Stop older shift loaders before
upgrading, because the new lock uses a different key.

The table preserves source IDs, period, shift number, event number, detail code,
optional descriptions, and the original clock strings alongside nullable parsed
seconds. NHL team IDs are not translated to franchise IDs. Source event numbers
are not assumed to identify play-by-play events. Names, team display metadata,
and the JSON source object are not stored in `public.shifts`. Successful HTTP
response bodies are retained separately by the ingestion history layer.

Ingestion verifies response completeness, field types, and game identity before
atomically replacing a game. It preserves inconsistent intervals for later
analysis; it does not certify time on ice or on-ice reconstruction.

For an existing deployment, stop shift ingestion before applying migration 0031,
then deploy the updated loader. The migration extracts the additional typed
fields from stored JSON before dropping it; no API refetch is needed. Dropping
the column does not immediately reclaim the existing table's disk space.

## Seasonal operation

PucksData does not need to run continuously during the offseason.

1. Sync refreshes both the current and upcoming season schedules during September.
   To load another season explicitly:

   ```bash
   pucksdata fetch games --season 20262027
   ```

2. Run the daemon during the season. Six-hour intervals suit current-data applications; daily syncs are sufficient for general analysis.
   The daemon and scheduled workflow share the same event and official-stat
   correction policy described above. Shift backfills remain separately operated.
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

See [schema conventions and migration direction](docs/schema-conventions.md) for
identifier meanings, schema boundaries, consumer contracts and the 0037–0038 rollout.

The migrations create:

- Entity tables: `teams`, `seasons`, `players` (including optional NHL headshot
  URLs), and `games`
- Current roster observations: `roster_snapshots`, `roster_memberships`, and `analytics.current_rosters`
- A shared `events` parent table
- Event detail tables: `goals`, `shots`, `hits`, `blocks`, `penalties`, and `faceoffs`
- Operational tables: `backfill_progress`, `sync_state`, and `ingestion.attempts`
- Prospective normalized revisions and source documents in `history`, with source observations in `ingestion`
- Read-only health views in the `observability` schema
- Dataset coverage metadata and official NHL season totals in the `analytics` schema
- Official skater and goalie game totals, plus a long-form downstream scoring view, in the `analytics` schema
- Materialized skater hit and blocked-shot season totals for low-cost downstream snapshots

Goals are also represented in `shots`, so the shots table covers every shot on net. Ingestion uses upsert semantics throughout and is designed to recover safely after partial failures.

The `events` table copies `season`, `game_type`, and `game_date` from its parent
game during ingestion. Consumers can therefore restrict the large event table
before joining event-type facts; the `(season, game_type, event_type)` index is
the primary access path for season-scoped event analysis.

Each event records `strength` (`ev`, `pp`, or `sh`) from the event owner's
perspective and identifies its NHL source in `strength_source`. From 2009-10
onward, validated `situationCode` values also provide exact skater and goalie
state. For 2005-06 through 2008-09, ingestion supplements the JSON feed with
the NHL scoring summary for goals and archived play-by-play reports for other
events. Values remain `NULL` when none of those sources establishes strength;
unknown data is not treated as even strength.

`analytics.official_skater_seasons` and `analytics.official_goalie_seasons`
hold the league's own published season totals, loaded by `fetch official-stats`.
They are not derived from play-by-play, and are kept in separate tables so the
two kinds of number stay distinguishable. They answer questions the event
schema cannot — games played from 1917-18, shots from 1967-68, goalie wins and
shutouts from 1917-18 — and serve as the reconciliation oracle for
event-derived figures.

`analytics.official_skater_games` and `analytics.official_goalie_games` hold
the league's current final-boxscore values at player/game grain. They complement
the event tables with facts such as plus/minus, game-winning goals, goalie
decisions, and shutouts. Re-observing an identical row updates its observation
time without changing `source_revision`; a changed published value advances the
revision. `analytics.official_player_game_stats` exposes the supported scoring
facts as a stable long-form contract for downstream applications.
`analytics.official_player_game_changes` additionally exposes revision-scoped
changes and retractions; see the [consumer contract](docs/ingestion-history.md#consuming-corrections).

`analytics.skater_physical_season_totals` aggregates hits and blocked shots
from the event archive. These categories are absent from the NHL season-total
endpoint, so the materialized rollup provides an indexed season/player lookup
without requiring downstream applications to scan historical games.

The NHL began recording different facts in different eras, so no single season
range covers every statistic. `analytics.coverage` publishes the first season
each event type and derived measure is available, and distinguishes available measures from concepts the
schema does not contain. Consumers
should consult it before answering a question that spans seasons: shot events
begin in 1997-98, and hits, faceoffs, blocks, giveaways and takeaways begin in
2009-10. `analytics.coverage_observed` compares that contract with the seasons
actually present so drift is detectable.

Downstream applications can use a dedicated read-only role. In addition to
permissions on the statistical tables they query, grant access to the health
views and coverage metadata with:

```sql
GRANT USAGE ON SCHEMA observability TO reader_role;
GRANT SELECT ON ALL TABLES IN SCHEMA observability TO reader_role;
GRANT USAGE ON SCHEMA analytics TO reader_role;
GRANT SELECT ON ALL TABLES IN SCHEMA analytics TO reader_role;
```

Replace `reader_role` with the application role. Repeat the `SELECT` grants
after migrations that add new views or tables to either schema.

## Scope and limitations

- The NHL APIs are public but unofficial and unversioned. Historical seasons, especially pre-2010 data, can contain structural gaps.
- International and national-team competitions are outside the NHL-franchise schema.
- Live-game polling is not implemented; synchronization targets completed games.
- Derived metrics such as expected goals, WAR, and fantasy scoring belong in downstream consumers.

## Shift analytics contract

Migration 0032 adds `nhl_team_identities` and `shift_fetch_status` for read-only
consumers. It does not change `shifts` or require any shift
backfill to be repeated. The identity table is seeded from the NHL team endpoint;
`pucksdata fetch teams` refreshes the mapping, including new source identities.
Raw shift team IDs must be resolved through this table before joining franchise
IDs in `games` or `teams`.

The latest fetch outcome (`loaded`, `unavailable`, `failed`) is separate from the
stored snapshot. Successful writes update status in the snapshot transaction;
empty/failed attempts preserve existing shifts. No historical outcomes are
invented. Games without rows remain retryable.

Apply migration 0032 before running the updated loader. Readers need SELECT on
`shifts`, `nhl_team_identities` and `shift_fetch_status`; the migration grants these
to the legacy reader role if it exists. Other reader roles require an explicit
grant; verify privileges for the actual role used by each consumer. The coverage
contract now advertises raw shifts from 2010–11, without
claiming that every game or interval is usable for line reconstruction.

## On-ice reconstruction

`pucksdata shifts reconstruct --game ID` derives event lineups with explicit
boundary ambiguity, source validation and official TOI reconciliation.
`pucksdata shifts audit --season 20252026` reports season coverage, count agreement
and unresolved anomalies; optional exports allow offline replay.

Migration 0033 adds coverage views and metadata only. Existing shifts and events
stay unchanged, and no backfill needs to be repeated. See the
[method and commands](docs/on-ice-reconstruction.md) before treating derived
lineups as reliable inputs to downstream sequence analysis.

## License

PucksData is available under the [MIT License](LICENSE).
