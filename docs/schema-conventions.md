# Schema conventions and migration direction

The immediate priority is preserving PucksPool's deployed reads while making
identities and ownership explicit. Migrations 0037–0038 establish that foundation;
they do not rename hockey storage tables or existing consumer columns.

## Schema responsibilities

| Schema | Responsibility |
| --- | --- |
| `public` | Existing hockey storage and legacy contracts during transition |
| `analytics` | Supported consumer reads: official facts, derived products, identity projections and coverage metadata |
| `ingestion` | Attempts, checkpoints, retry state, diagnostics and pending maintenance |
| `history` | Source documents and accepted normalized revisions and observations |
| `observability` | Read-only health, freshness and coverage reports |

`analytics` is a consumer boundary, not a promise that every relation is a
calculated statistic. Official NHL facts belong here as supported reads. Moving
their backing storage can wait. `ingestion.source_observations` intentionally
remains the receipt ledger connecting source captures to ingestion attempts;
the response content itself lives in `history.source_documents`.

`backfill_progress`, `sync_state` and `shift_fetch_status` now live in `ingestion`.
Their original `public` names remain read-only compatibility views. Existing
observability dependencies follow the original table objects automatically.

## Naming and identity rules

Use lowercase snake_case, plural nouns for entity collections, and descriptive
names for catalogs, state and products. A name should communicate row grain;
do not mechanically pluralize established contracts. Fully qualify relations in
new SQL. Use explicit view projections rather than `SELECT *`.

| Meaning | Preferred name | Legacy ambiguity |
| --- | --- | --- |
| NHL franchise identity | `franchise_id` | `teams.team_id`, game home/away team IDs and roster team IDs are franchise IDs |
| NHL source team identity | `nhl_team_id` | `shifts.team_id` is a source team ID, not a franchise ID |
| Eight-digit season, such as 20252026 | `season_code` | `games.season` and `seasons.season_year` contain this code |
| Internal season surrogate | Internal only | `seasons.season_id` is not the season code |
| Source event identity | Game ID plus source event ID | `events.id` is an internal row key that can change during replacement |

For new interfaces, distinguish source keys from internal keys, include units
in numeric duration names, and document whether a clock is period-relative or
game-relative. Use `timestamptz` for instants and `_at` names; retain explicit
`_utc` where already contractual. Date-only hockey schedule dates remain dates.

Four additive interfaces make these rules usable now:

| Interface | Grain and semantics |
| --- | --- |
| `analytics.franchises` | One franchise with current display identity; historical NHL identities remain in `public.nhl_team_identities` |
| `analytics.season_catalog` | One eight-digit season code; omits the internal surrogate |
| `analytics.game_inventory` | One game, including scheduled games; explicit home/away franchise IDs |
| `analytics.raw_shift_rows` | One `(game_id, source_shift_id)` row; both source team ID and nullable mapped franchise ID |

Raw shift rows retain unknown identities and unvalidated intervals. This view
does not promise deduplication, interval repair or on-ice reconstruction.

History dataset strings are logical identifiers independent of table locations.
Migration 0037 preserves all existing dataset names and payload shapes. Future
column renames must also preserve or deliberately version the normalized payload;
stable dataset names alone cannot prevent artificial revisions from renamed keys.

## Protecting PucksPool

The checked-in [contract](../tests/contracts/puckspool.json) records relation
names and ordered column names/types used by PucksPool at commit
`e8edeb9fb7415a7a3c72ad017d3b42ca1e779a9f`. Its existing public tables and analytics
interfaces remain unchanged. This is a reviewed source contract, not a claim
that the deployed application was exercised end to end.

CI verifies those shapes and upgrades a populated database from migration 0036.
It compares existing read results and the captured PucksPool scoring, draft and
schedule SQL before and after migration, using a restricted reader. Fixtures
include official skater/goalie facts and a scoring retraction. It also checks
legacy grants, column grants, grant options, default privilege isolation,
canonical upserts, operational table identities, history continuity and cascades.

Run the upgrade check with a disposable PostgreSQL instance:

```sh
TEST_DATABASE_URL=postgresql://postgres:postgres@localhost:5432/pucksdata_test \
  python3 scripts/check_schema_upgrade.py
```

The connection needs CREATEDB/CREATEROLE and PostgreSQL client programs. The
script creates and removes its own randomly named database and roles. Use a
test server, not production. Normal `cargo test --all-targets` additionally checks
contract shapes and the new views when `TEST_DATABASE_URL` is configured.

## Rollout for 0037–0038

1. Complete the [0034–0036 rollout](ingestion-history.md#rollout) if necessary.
   Pause the scheduled sync and all other ingestion writers; allow active runs
   to finish. These migrations require table locks, so use a quiet window.
2. Apply migrations with the schema-owner connection using `sqlx migrate run`.
   Each new migration must be transactional, including function/trigger changes.
3. Ensure the runtime has `USAGE` on `ingestion`. Original table privileges move
   with the three operational tables; existing history and invalidation grants
   remain necessary. For a separate runtime role, substitute its actual name:

   ```sql
   GRANT USAGE ON SCHEMA ingestion TO ingestion_role;
   GRANT SELECT, INSERT, UPDATE, DELETE ON ingestion.backfill_progress,
       ingestion.sync_state, ingestion.shift_fetch_status TO ingestion_role;
   ```

4. Deploy the matching PucksData binary before resuming ingestion. Old binaries
   cannot upsert through the compatibility views. A binary-only rollback is
   therefore insufficient after 0038; keep writers paused until the binary and
   canonical table locations agree.
5. Verify PucksPool's draft, schedule and scoring reads and the first sync's
   status. PucksPool requires no application deployment or schema change for
   this upgrade. Its current grants remain attached to unchanged relations.

Legacy operational SELECT grants (including column-only grants and grant
options) are copied to the public views. Those readers need no new `ingestion`
schema access. New analytics views receive SELECT for `pucksstudio_read` if that
role exists; other readers can receive narrowly scoped grants when adopting them:

```sql
GRANT USAGE ON SCHEMA analytics TO consumer_role;
GRANT SELECT ON analytics.franchises, analytics.season_catalog,
    analytics.game_inventory, analytics.raw_shift_rows TO consumer_role;
```

## Next substantial migration

First extend the explicit analytics contracts for the remaining consumer needs
and migrate PucksPool reads in its own tested release. Inventory other consumers
and grants before moving physical hockey storage into a domain schema such as
`hockey`. Keep the legacy interfaces until those consumers have migrated.

Then rename physical keys to their actual identity domains, preserving foreign
keys, history payload semantics and consumer view shapes. Compare populated
upgrade results and query plans before rollout. Only retire compatibility views
after a documented deprecation period and confirmation that deployed consumers
no longer use them. PucksQL can adopt the new contracts as development resumes;
it does not need to delay this preparatory work.
