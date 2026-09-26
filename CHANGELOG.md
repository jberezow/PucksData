# Changelog

All notable changes to PucksData are documented here. The project follows
[Semantic Versioning](https://semver.org/) beginning with version 1.5.0.

## [Unreleased]

### Added

- Prospective normalized history for entity records, events, shifts, and official
  game/season statistics, with content hashes and repeat observations.
- Source response capture and ingestion attempt outcomes, plus a freshness view.
- Official player/game correction feed with explicit retractions.
- Durable transformation diagnostics and ingestion issues in status reports.
- Added season-scoped ingestion of typed, unnormalized NHL shift-chart rows
  from 2010–11 onward.

### Changed

- Bound daily player discovery to active seasons plus four rotating historical
  audits, while retaining the explicit full-archive player fetch.
- Selectively enrich schedules with a bounded audit for older metadata changes.
- Persist derived-product invalidations transactionally and recover interrupted
  refreshes even when a later sync has no new games.
- Capture source documents and observations in one atomic SQL statement, and
  log HTTP, capture, pool-wait and phase timings.
- Select correction retries from the latest attempts in one set-based query.
- Temporarily allow 60 minutes for the scheduled sync during rollout.

- Share recent event and official-stat correction audits between sync and daemon:
  three days normally, fourteen on Sundays, with configurable and explicit replay windows.
- Preserve known game metadata when an observation omits enrichment fields.
- Reject incomplete official game reports before replacing stored snapshots.
- Propagate partial sync failures and retain the last successful sync timestamp.
- Retry transient HTTP failures with bounded backoff and `Retry-After` handling.
- Serialize mutating commands through a pooler-safe writer lease and reject
  detectably incomplete modern play-by-play snapshots.
- Replace stored shift source JSON with typed event number, detail code, and
  optional description fields, preserving existing rows through a forward migration.
- Verify shift response completeness and field types before replacement.
- Pace shift-chart ingestion conservatively to avoid saturating the NHL Stats
  REST endpoint during season backfills.

### Upgrade notes

Stop old ingestion writers before applying migration 0034, grant the runtime
access to the new schemas, and deploy the updated binary. History begins with
new observations; the migration does not manufacture historical knowledge.
Then apply migrations 0035–0036 and their runtime grants before deploying the
incremental sync. See [ingestion history](docs/ingestion-history.md) for both
rollout steps, audit cadences and recovery guarantees.

## [1.8.0] - 2026-09-22

### Added

- Added complete current-roster snapshots, preserving roster group and player
  position at the time each snapshot is observed.
- Added finalized per-game skater and goalie statistics, including goalie
  goals and assists, with a long-form player-game analytics view.
- Added player headshot metadata.
- Added skater height, weight, age, and draft fields to official season totals.
- Added indexed event-scope columns and player-season rollups for faster
  downstream analytics.

### Changed

- Materialized the dataset-health and player-season analytics rollups and made
  their refresh paths safe for concurrent readers.
- Batched game upserts to reduce database round trips.
- Improved project documentation and branding.

### Fixed

- Treat empty historical NHL reports as an absent source instead of attempting
  to parse them as populated reports.

## [1.7.0] - 2026-09-04

### Upgrade notes

`events.strength` changes meaning in this release, and downstream consumers
should re-read it rather than assume continuity.

- It is now stated from the perspective of the team that owns the event, not
  the home team. A value of `pp` means the owning team had the advantage.
- It is now nullable, and is `NULL` wherever no NHL source establishes a
  strength: events with no owning team, every season before 2005-06, and
  penalties before 2009-10. It is no longer silently `ev`.
- The decoded skater counts and goalie flags are likewise `NULL` before
  2009-10 rather than defaulting to five a side.
- `events.strength_source` records which NHL source produced each value.
  Consult it, and `analytics.coverage`, before aggregating across eras.

Before this release the database reported zero power-play goals for every
season from 1917-18 to 2008-09. It now reproduces the NHL's official
power-play and shorthanded totals exactly for 2005-06 onward.

### Changed

- Corrected the home/away interpretation of NHL `situationCode` values.
- Defined event `strength` from the event owner's perspective and made it
  nullable when the NHL source does not establish a strength.
- Historical replays now replace each game's prior event snapshot atomically,
  removing events that disappear in later NHL feed revisions.
- Rewrote `analytics.coverage_observed` to scan the events table once rather
  than once per event type, bringing it inside the read-only role's statement
  timeout.
- Read archived report strength for blocked shots from the blocking team's
  perspective, and stopped deriving strength from penalty rows, which state
  the manpower before the penalty is applied.

### Added

- Preserved validated NHL `situationCode` values on newly ingested events.
- Added `strength_source` provenance and historical strength enrichment from
  NHL scoring summaries and archived play-by-play reports.
- Added season-scoped `backfill --refresh` for authoritative re-ingestion.
- Added `fetch official-stats`, loading official NHL skater and goalie season
  totals into the `analytics` schema. These answer season-level questions the
  event schema cannot, including games played and goalie records from 1917-18,
  and provide a reconciliation oracle for event-derived figures.
- Recorded a coverage caveat for 2009-10, whose NHL play-by-play feed is
  incomplete at source.
- Added an `analytics` schema publishing dataset coverage: the first season
  each event type and derived measure is available, the concepts the schema
  does not contain, and a view that detects drift against the stored data.

## [1.6.1] - 2026-09-03

### Fixed

- Corrected the scheduled canary's disposable PostgreSQL configuration.

## [1.6.0] - 2026-09-02

### Added

- Added read-only dataset and season health views in the `observability`
  schema, plus JSON status output for downstream consumers.
- Added a scheduled production sync workflow with health summaries and
  short-lived report artifacts.
- Added indexes for player event roles and event participants.
- Added a disposable PostgreSQL test workflow for local and CI use.

### Changed

- Classified known upstream API gaps separately from actionable ingestion
  failures.
- Improved scheduled-sync failure signals and health reporting.

## [1.5.1] - 2026-08-26

### Changed

- Upgraded SQLx to 0.9 and updated compatibility code.
- Made setup documentation environment-agnostic.

### Added

- Added a live NHL API-to-PostgreSQL canary workflow.
- Added compatibility for the NHL season endpoint's current response schema.

## [1.5.0] - 2026-08-25

### Added

- GitHub Actions quality gate with a real PostgreSQL integration-test service.
- Complete package metadata, MIT license, architecture overview, and operating guide.

### Changed

- Reconciled the v1.4 documentation history with the default branch.
- Standardized the Rust source tree with `rustfmt`.
- Removed local agent and planning tooling from the published repository tree.

## Legacy milestone tags

Tags `v1.0` through `v1.4` track the original project milestones. New releases
use three-component semantic versions.

[1.5.0]: https://github.com/jberezow/pucksdata/compare/v1.4...v1.5.0
[1.5.1]: https://github.com/jberezow/pucksdata/compare/v1.5.0...v1.5.1
[1.6.0]: https://github.com/jberezow/pucksdata/compare/v1.5.1...v1.6.0
[1.6.1]: https://github.com/jberezow/pucksdata/compare/v1.6.0...v1.6.1
[1.7.0]: https://github.com/jberezow/pucksdata/compare/v1.6.1...v1.7.0
[1.8.0]: https://github.com/jberezow/pucksdata/compare/v1.7.0...v1.8.0
[Unreleased]: https://github.com/jberezow/pucksdata/compare/v1.8.0...HEAD
