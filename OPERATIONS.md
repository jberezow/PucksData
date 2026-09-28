# Operating PucksData

This guide covers recurring ingestion, database upgrades and deployment. For a
new installation, start with the [README](README.md#quick-start).

## Connections and configuration

`DATABASE_URL` is the runtime connection; `MIGRATION_DATABASE_URL` is the
schema-owner connection used by `scripts/run-migrations.sh`. They can be the
same for local development. Hosted deployments should use a restricted ingestion
role at runtime. Both wrapper scripts below read their connection from `.env`
when it is not exported, without sourcing the file; URL characters such as `&`
are preserved.

`DB_POOL_MAX_CONNECTIONS` defaults to **5**. Ordinary CLI writes require at least
**2** connections; shift backfills and the daemon require at least **3**. Advisory leases reserve
connections while work runs, so leave capacity for ingestion as well. Size each
process's pool against your database's total connection budget.

`SYNC_INTERVAL_SECS` defaults to `21600` (six hours).
`PUCKSDATA_CORRECTION_DAYS` optionally sets a correction window of 1–366 days;
otherwise sync uses three days, widened to fourteen on Sundays (UTC).

Operational logs go to stderr, leaving command results on stdout. `RUST_LOG`
controls verbosity (default `pucksdata=info`). `PUCKSDATA_LOG_FORMAT` accepts
`text` (default) or `json` for structured log collection.

## Upgrading an existing database

Pause older ingestion processes while applying schema changes, and deploy a
compatible binary before resuming them. Migrations are not run by the container.
Use the [history rollout](docs/ingestion-history.md#rollout),
[incremental sync upgrade](docs/ingestion-history.md#incremental-sync-upgrade)
and [schema rollout](docs/schema-conventions.md) for their
specific permissions and ordering requirements.

### Event scope: databases at migration 0020 or earlier

Stage the event-scope update so existing events are filled in batches:

```bash
./scripts/run-migrations.sh --target-version 21
./scripts/run-event-scope-backfill.sh
./scripts/run-migrations.sh
```

Deploy the updated ingestion binary before the final command. The migration
wrapper uses `MIGRATION_DATABASE_URL`; the backfill wrapper uses `DATABASE_URL`.
Fresh databases can apply all migrations together because there are no existing
events to fill.

### Shift storage and readers

Stop shift ingestion before migration 0031 and deploy the updated loader
afterward. The migration extracts typed fields from stored JSON before dropping
it; no API refetch is needed. Dropping the column does not immediately reclaim
the table's disk space.

Migration 0032 adds `nhl_team_identities` and `shift_fetch_status` without
requiring a shift backfill. Apply it before running the corresponding loader.
Readers need SELECT on both tables and `shifts`; verify the actual reader role's
grants. Migration 0033 adds coverage views and metadata without changing stored
shifts or events.

Older shift loaders use a different advisory-lock key. Stop them before starting
the current loader, which holds a transaction-scoped lease with heartbeats
through ingestion.

### Franchise attribution

The seed mapping for the original Winnipeg Jets predates the NHL's attribution
change. Fresh installations and older databases need the
[Winnipeg audit and repair](docs/team-attribution.md#winnipeg-audit-and-repair)
before ingestion. The audit distinguishes missing data from incorrect attribution;
the repair preserves the Coyotes' own history.

## Sync and daemon

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

See [ingestion history](docs/ingestion-history.md) for freshness queries,
historical snapshots and the correction contract.

### `daemon`

Run synchronization immediately and then on a fixed interval. The default interval is six hours and can be changed with a flag or `SYNC_INTERVAL_SECS`.

```bash
pucksdata daemon
pucksdata daemon --interval-secs 3600
pucksdata daemon --backfill-on-start
```

Only one daemon can hold its PostgreSQL advisory lock at a time. All mutating
commands also share a pooler-safe writer lease through fetch and commit. Runtime
pools need at least two connections for ordinary CLI writes and three for shift
backfills or the daemon (default: five). SIGTERM and Ctrl-C abort the current idempotent operation and
exit cleanly.

## Health and repair

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

## Shift ingestion

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

The shift backfill does not run from the normal event daemon.
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
heartbeats maintain it during ingestion.

The table preserves source IDs, period, shift number, event number, detail code,
optional descriptions, and the original clock strings alongside nullable parsed
seconds. NHL team IDs are not translated to franchise IDs. Source event numbers
are not assumed to identify play-by-play events. Names, team display metadata,
and the JSON source object are not stored in `public.shifts`. Successful HTTP
response bodies are retained separately by the ingestion history layer.

Ingestion verifies response completeness, field types, and game identity before
atomically replacing a game. It preserves inconsistent intervals for later
analysis; it does not certify time on ice or on-ice reconstruction.

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

## Scheduled workflows

The daily `NHL API and ingestion canary` exercises live NHL season endpoints,
validates response shapes and writes to disposable PostgreSQL. It can also be
started from the Actions tab and never connects to production.

`Scheduled database sync` runs `sync` daily, publishes a job summary and retains
a JSON health report as a short-lived artifact. It requires a repository Actions
secret named `DATABASE_URL` containing an ingestion-role connection string.

## Reader access

In addition to grants on the statistical tables a consumer queries, grant
access to health views and coverage metadata:

```sql
GRANT USAGE ON SCHEMA observability TO reader_role;
GRANT SELECT ON ALL TABLES IN SCHEMA observability TO reader_role;
GRANT USAGE ON SCHEMA analytics TO reader_role;
GRANT SELECT ON ALL TABLES IN SCHEMA analytics TO reader_role;
```

Replace `reader_role` with the application's role. Repeat the SELECT grants
after migrations add views or tables. History and correction consumers should
also follow the [correction contract](docs/ingestion-history.md#consuming-corrections).
