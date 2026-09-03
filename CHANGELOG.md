# Changelog

All notable changes to PucksData are documented here. The project follows
[Semantic Versioning](https://semver.org/) beginning with version 1.5.0.

## [Unreleased]

### Changed

- Corrected the home/away interpretation of NHL `situationCode` values.
- Defined event `strength` from the event owner's perspective and made it
  nullable for events without an owning team.

### Added

- Preserved validated NHL `situationCode` values on newly ingested events.

## [1.5.0] - Unreleased

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
