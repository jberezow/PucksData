# Current NHL franchise attribution

PucksData follows the NHL's current official franchise attribution. NHL team
identities remain separate: original Winnipeg is source team `33` (`WIN`),
Atlanta is `11` (`ATL`), and current Winnipeg is `52` (`WPG`). All three now map
to franchise `35`. Phoenix `27` and Arizona `53` remain franchise `28`; Utah
remains franchise `40`.

The NHL [announced the consolidation on September 24, 2026](https://www.nhl.com/jets/news/jets-and-nhl-announce-consolidation-of-winnipeg-jets-history).
The [team endpoint](https://api.nhle.com/stats/rest/en/team?limit=-1) now assigns
`33 -> 35`. This is current classification, not a new affiliation-history model.

## Winnipeg audit and repair

Run from the repository root. The helper uses Python's standard library, curl,
and psql. Connection variables can be supplied in the environment or `.env`;
credentials are not included in reports. Default mode uses a read-only database
transaction and does not record an ingestion attempt or capture source bodies.

```sh
python scripts/reconcile_winnipeg.py --output docs/audits/winnipeg-before.json
```

The helper fetches all source-team-33 games with checked pagination and confirms
the NHL Records population of 1,338 regular-season and 62 playoff games. It
excludes other game types (currently two 1994 exhibition games against European
clubs). It checks exact game IDs, dates, seasons, game types, participants and
event owners against stored data. Missing stored games are reported as coverage
gaps; the repair does not invent them. Unexpected stored games or ambiguous
participants/owners prevent application.

Review the counts and discrepancies, then apply with a database role that can
update games, events, identities and `analytics.coverage` and acquire table locks:

```sh
python scripts/reconcile_winnipeg.py --apply --database-env MIGRATION_DATABASE_URL \
  --output docs/audits/winnipeg-repaired.json
cargo run --release -- refresh-derived
python scripts/reconcile_winnipeg.py --output docs/audits/winnipeg-after.json
```

Application takes the existing ingestion advisory lease and locks the affected
tables against concurrent writes, while permitting reads. It checks again inside
the transaction, then changes only verified game participants, original-Jets
event ownership and identity `33`. It also corrects the coverage description.
Existing invalidation triggers schedule derived refreshes. The transaction
checks that game/event counts, all other columns on affected games/events, and
control games are unchanged before committing. A failure rolls everything back.
Repeating the repair makes no further attribution changes.

The final audit must show franchise `35`, zero games/events needing correction,
and zero invalid participants/owners. Keep missing-game coverage distinct from
attribution correctness. No schema migration or historical re-download is needed.
Existing migrations still describe the original seed snapshot; fresh installations
should run this reconciliation before ingestion while that seed contains `33 -> 28`.

## Future upstream changes

CLI writers and each sync/daemon iteration fetch and validate the NHL identity
catalogue under the existing writer lease. A known identity changing franchise,
disappearing, or losing its franchise fails before fact ingestion. Safe metadata
updates and new identities are accepted. Mapping consumers within that command
reuse the accepted map instead of fetching different versions mid-run.

`fetch teams` cannot bypass this check by overwriting an existing affiliation.
Standalone library fetchers remain read-only and can query live NHL mappings;
custom library writers should use `process::team_attribution::with_current_mapping`
inside `process::attempts::exclusive`, as the CLI does.

On a future reassignment, audit the affected source identities and catalogues,
prepare a similarly bounded reconciliation, validate it against a populated test
database, then update identities and facts together. Do not copy this Winnipeg
repair for another franchise without reviewing its population and ambiguity rules.
Never perform a global replacement of franchise `28`: that would move the
Coyotes' own games and events too.

## Validation

```sh
TEST_DATABASE_URL=postgresql://postgres:postgres@localhost:55432/pucksdata_test \
  python -m unittest discover -s scripts -p test_reconcile_winnipeg.py -v
```

The database tests create and remove their own disposable database, apply all
migrations, and exercise partial refreshes, null/opponent event owners, controls,
repeat runs, invalid inputs, concurrent-writer rejection and rollback after writes.
