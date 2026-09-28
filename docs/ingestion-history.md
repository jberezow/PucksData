# Ingestion history and corrections

Migration 0034 adds prospective history and attempt tracking. Current tables
remain the operational interface; the new schemas preserve evidence for audits
and downstream research. No production migration is implied by this document.

## Rollout

1. Stop all existing ingestion writers, including scheduled jobs and daemons.
2. Apply migrations using the schema-owner connection:
   `./scripts/run-migrations.sh`.
3. Grant the ingestion role access to the new objects, in addition to its existing
   table permissions. Replace `ingestion_role` below with your runtime role.
4. Deploy the updated binary and resume ingestion. Check attempt outcomes after
   the first sync.

```sql
GRANT USAGE ON SCHEMA history, ingestion TO ingestion_role;
GRANT SELECT, INSERT ON ALL TABLES IN SCHEMA history TO ingestion_role;
GRANT SELECT, INSERT, UPDATE ON ingestion.attempts TO ingestion_role;
GRANT SELECT, INSERT ON ingestion.source_observations, ingestion.diagnostics TO ingestion_role;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA history, ingestion TO ingestion_role;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA history TO ingestion_role;
```

Readers need `USAGE` on the schemas and `SELECT` on the particular tables/views
they consume. Repeat analytics and observability grants for newly created views.
History triggers run with the writer's permissions: missing grants fail the
write. Old event, official-stat, or shift loaders do not record normalized
snapshots, so do not resume old writers after the migration.

The migration does not backfill history. Before the first official-game
replacement, the loader preserves any existing rows as an `existing-v1` baseline
recorded now. This allows the first refresh to expose removals and preserve player
revision continuity; it does not establish when those old values were observed.
Subsequent accepted loads use `normalized-v1`. Existing roster snapshots retain
their own observation history.

Mutating CLI commands and syncs hold one pooler-safe writer lease through fetch
and commit, with a heartbeat. Read-only commands remain available. Allow at least
two database connections for most CLI ingestion, and three for the daemon or
CLI shift backfills. The default pool allows five.

## Incremental sync upgrade

Stop ingestion writers, apply migrations 0035 and 0036, grant the following
permissions to the runtime role, and then deploy the new binary. This also
applies to an installation that has already completed the 0034 rollout.
No production migration is applied by the PR or by the scheduled workflow.

```sql
GRANT SELECT, INSERT, UPDATE ON ingestion.player_audits,
    ingestion.schedule_checks TO ingestion_role;
GRANT SELECT, INSERT, DELETE ON ingestion.derived_invalidations TO ingestion_role;
GRANT USAGE, SELECT ON SEQUENCE ingestion.derived_invalidations_invalidation_id_seq
    TO ingestion_role;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA ingestion TO ingestion_role;
```

Dependency triggers execute with the writer's permissions. Apply these grants
before resuming any writer, including an older binary. Existing history, source
capture, schema-usage and materialized-view refresh permissions remain required.
The new migration seeds all three derived products as pending, so the first sync
rebuilds them once even when it finds no new events.

Daily and daemon syncs now use the following policy:

- Player discovery includes current rosters and current-season regular/playoff
  stats, with both outgoing and incoming seasons in September. Each run also
  audits up to four historical seasons, oldest successful audit first, excluding
  seasons audited in the last 30 days. IDs are deduplicated before profile fetches.
  At one successful run per day the existing archive is audited in roughly a
  month. Failed player/roster refreshes do not advance audit checkpoints; a completed
  player phase can checkpoint even if a later sync phase fails. The explicit
  `fetch players` command still refreshes the entire archive, and event-based
  missing-player repair remains part of sync.
- Schedule discovery still reads the complete active-season catalogs. Boxscores
  are fetched for new games, changed catalog fields, games in the correction
  window through 14 days ahead, and past games with unresolved states (at most
  once per UTC day when their catalog is unchanged). Up to 100 additional games
  per season are audited per run, oldest check first, when unchecked or last
  checked at least seven days ago. The budget means this is not a seven-day
  freshness guarantee: a 1,500-game season takes about 15 daily runs to sweep.
  These checks catch venue, start-time and state changes absent from the catalog.
  An empty checkpoint table does not force a full refresh of existing games;
  genuinely new catalogs still require initial enrichment. Explicit game fetches
  retain full enrichment. Checkpoints follow accepted game writes, so crashes
  can repeat work but cannot skip an uncommitted replacement.
- Derived products refresh only after relevant committed dependency writes or
  a failed/interrupted refresh. Dependency invalidations are recorded in the
  same transaction as those writes. Rollbacks leave no invalidation, and only
  invalidations visible before a successful refresh are acknowledged. This
  preserves newer or late-committing writes, and allows a later zero-work sync
  or backfill to repair an interrupted refresh. No-op schedule updates and
  unrelated player/shift writes do not invalidate the three existing products.
- Event and official-game correction windows, failed-game retries, complete
  roster requirements, source observations and normalized history guarantees
  are unchanged. Source capture uses one atomic SQL statement per response;
  duplicate bodies still produce separate timestamped observations.

The scheduled job has a 60-minute limit for rollout headroom. This is a safety
margin, not a runtime target. Compare several normal and Sunday runs before
reducing it again. Source capture concurrency and the database pool size are
unchanged; tune them only if timing evidence shows sustained contention.

### Runtime measurements

`[phase]` lines identify dataset, entity and attempt. `[timing]` lines report
elapsed seconds, actual HTTP request/retry counts, HTTP worker milliseconds,
source-capture SQL worker milliseconds and source-capture pool-wait milliseconds.
Worker times sum concurrent operations and can exceed wall time. Each attempt
reports its own work; a parent does not double-count nested attempt metrics.
Player upserts and schedule writes also log elapsed seconds. Schedule logs show
catalog size, immediate selections, periodic selections and boxscore requests;
derived logs explicitly report skipped products.

Compare these phase measurements with the September 24 timeout and subsequent
successful runs. Do not treat the reduced request counts as a measured runtime
speedup: NHL and database latency still vary, initial catalogs need more work,
and Sundays audit more completed games.

## Failure and refresh behavior

`sync`, daemon syncs, and the scheduled workflow use one correction policy:
refresh recent completed regular-season and playoff games, including existing
events and official player/game totals. The window starts three days before
today, or fourteen days before today on Sundays, using UTC dates.
`PUCKSDATA_CORRECTION_DAYS=1..366` overrides that default. `sync --from DATE`
replays eligible completed games from the specified date, not just gaps.

Failed event attempts remain retryable. Expected empty shift feeds preserve
stored rows and remain separately reported as unavailable; shifts still require
an explicit season-scoped backfill. Official season totals also retain their
separate fetch command.

Game upserts preserve known nullable enrichment fields when a new observation
omits them. Transient enrichment failures propagate rather than silently
producing a successful schedule refresh. Official game replacement checks report
identities and player inventory against the completed boxscore before writing.
Modern play-by-play (2009–10 onward) requires a completed state, a game-end marker,
nonempty events, unique source IDs, and no transformation warnings. Historical
transformation warnings are retained in `ingestion.diagnostics`. These checks
reject detectable truncation and inconsistency, not every possible source error.

An unsuccessful sync does not advance `sync_state.last_sync_at`. Attempt records
distinguish complete, partial, failed, unavailable, and unfinished work. A `running` record
may belong to an active or interrupted process; check the worker before replaying.

```sql
SELECT dataset, entity_key, last_attempt_at, last_success_at, outcome, error_message
FROM observability.ingestion_freshness
ORDER BY last_attempt_at DESC;
```

Successful steps can commit before a later step fails. Freshness is therefore
reported per recorded operation, not as a claim that the entire database shares
one consistent observation time. `status --json` includes up to 100
`ingestion_issues`: latest failed/partial attempts and attempts still running after
two hours. Global status includes pipeline and derived-refresh issues;
season-scoped status restricts these to that season's game datasets.

## Recorded evidence

| Object | Meaning |
|---|---|
| `history.snapshots` | Accepted normalized payloads, keyed by dataset/entity/revision, with SHA-256 and method version |
| `history.observations` | Repeated observations of a normalized snapshot, including unchanged payloads |
| `history.source_documents` | Deduplicated successful HTTP response bodies keyed by SHA-256 |
| `ingestion.source_observations` | Response URL, receipt time, hash, and ingestion attempt |
| `ingestion.attempts` | Start, finish, outcome, engine version, and error for tracked work |
| `ingestion.diagnostics` | Aggregated transformation warning counts and bounded examples |

Normalized datasets are `games`, `players`, `teams`, `seasons`, and
`nhl_team_identities` at entity grain; `events`, `official_games`, and `shifts`
at game grain; and `official_skater_seasons` / `official_goalie_seasons` keyed
by `player_id:season:game_type`. Roster membership uses the existing
`roster_snapshots` and `roster_memberships` contract.

Entity writes are captured by triggers. Game snapshots are recorded in the same
transaction as accepted replacement, so rollback removes both. Identical
normalized payloads create another observation without another revision.
Volatile observation timestamps and replaceable event surrogate IDs are omitted
from game payload comparisons. Entity deletion creates `{"deleted": true}`.

During tracked ingestion, successful HTTP bodies are captured before parsing;
a body that fails validation can therefore remain available for diagnosis.
HTTP errors do not create successful-body documents. Read-only reconstruction
and audit commands do not archive their network reads. Response capture is
committed separately from normalized replacement, so a failed load can retain
its source evidence. Normalized snapshots and repeat observations carry `attempt_id` when written
within tracked ingestion, linking accepted state to its attempt's source evidence.
Library callers must use `attempts::track` or `provenance::scope` to capture raw
responses; standalone loader calls can record normalized state without an attempt.

History is append-only through mutation guards. Plan storage and backups for
both normalized revisions and raw bodies; no automatic retention policy is
provided. Hashes identify stored content, not an NHL-issued version or signature.

## Querying historical states

```sql
SELECT entity_key, revision, recorded_at, content_sha256, payload
FROM history.as_of('games', TIMESTAMPTZ '2026-10-15 12:00:00+00');
```

`history.as_of` returns the latest recorded revision for each entity by the
cutoff, including deletion markers. Filter those markers explicitly when a
consumer needs only present entities. Entities not yet captured are absent,
not evidence that they did not exist.

`recorded_at` is a local timestamp taken inside the write transaction. It is
neither the NHL publication time nor the transaction commit time. A transaction
can commit after the cutoff even though its recorded timestamp precedes it.
This is an audit of recorded states, not a strict reconstruction of database
visibility at that instant. Downstream prediction experiments must account for
this limitation and cannot infer pre-capture knowledge from today's backfill.

To inspect source evidence for a failed attempt:

```sql
SELECT o.url, o.observed_at, o.content_sha256, d.body
FROM ingestion.source_observations o
JOIN history.source_documents d USING (content_sha256)
WHERE o.attempt_id = 123
ORDER BY o.observation_id;
```

## Consuming corrections

`analytics.official_player_game_stats` keeps its existing current-fact contract.
The additive `analytics.official_player_game_changes` view compares consecutive
accepted game snapshots and includes:

- `set`: a new or changed value, including an authentic zero;
- `retracted`: a formerly present fact is absent, with `stat_value = NULL` and
  its prior value retained as `previous_value`.

The first captured snapshot emits its present facts as `set`. For preexisting
official rows, the `existing-v1` baseline lets the first refresh emit retractions;
changes predating that baseline cannot be recovered. A game's `game_revision` is distinct
from the existing player row's `source_revision`. Player revisions remain
monotonic if a player disappears and later reappears in a captured game snapshot.

```sql
SELECT game_id, game_revision, player_id, stat_code,
       change_kind, previous_value, stat_value
FROM analytics.official_player_game_changes
WHERE game_id = 2025020001
ORDER BY game_revision, player_id, stat_code;
```

Consumers should reconcile complete game snapshots or apply changes idempotently
using `(game_id, game_revision, player_id, stat_code)`. A retraction reverses a
previous contribution; it must not be presented as a newly published zero.
Persist consumer progress transactionally with ledger updates. Snapshot identity
values are allocated before commit, so a global `snapshot_id > last_seen`
cursor alone can miss concurrently committing work; use per-game reconciliation
or overlapping polling with idempotency.

Consumers must adopt this contract downstream to act on retractions. Publishing
the view does not change their scoring implementations automatically.

## Stable dataset identifiers

Migration 0037 passes an explicit logical dataset name to each entity history
trigger. Existing dataset names, entity keys and payloads are unchanged, so a
physical table rename or schema move no longer starts a new revision stream.
Column renames still require an explicit payload mapping to preserve hashes and
consumer semantics. See [schema conventions](schema-conventions.md) for the
0037–0038 rollout and future migration direction.
