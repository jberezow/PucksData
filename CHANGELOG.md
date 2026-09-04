# Changelog

All notable changes to PucksData are documented here. The project follows
[Semantic Versioning](https://semver.org/) beginning with version 1.5.0.

## [Unreleased]

### Changed

- Corrected the home/away interpretation of NHL `situationCode` values.
- Defined event `strength` from the event owner's perspective and made it
  nullable when the NHL source does not establish a strength.
- Historical replays now replace each game's prior event snapshot atomically,
  removing events that disappear in later NHL feed revisions.
- Read archived report strength for blocked shots from the blocking team's
  perspective, and stopped deriving strength from penalty rows, which state
  the manpower before the penalty is applied.

### Added

- Preserved validated NHL `situationCode` values on newly ingested events.
- Added `strength_source` provenance and historical strength enrichment from
  NHL scoring summaries and archived play-by-play reports.
- Added season-scoped `backfill --refresh` for authoritative re-ingestion.
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
[Unreleased]: https://github.com/jberezow/pucksdata/compare/v1.6.1...HEAD
