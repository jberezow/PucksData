# Validated event-level on-ice reconstruction

Method: `nhl-on-ice-v1`. Available from 2010–11 for completed regular-season
and playoff games with stored NHL shifts. This is a reproducible interpretation
of source observations, not a claim that every lineup is ground truth.

## Run it

Apply migration 0033 for the new coverage views and metadata. It adds no tables,
changes no canonical rows and requires **no refetch or backfill**. The analysis
commands themselves only require the existing schema through 0032.

```sh
cargo build --release
./target/release/pucksdata shifts reconstruct --game 2025020001 --output game.json

./target/release/pucksdata shifts audit --season 20252026 --profile   --output audit.json   --snapshot-out source.jsonl   --games-out game-diagnostics.jsonl   --events-out event-lineups.jsonl

# Offline, without DATABASE_URL or a running PostgreSQL server:
./target/release/pucksdata shifts audit --season 20252026   --input source.jsonl --output replay.json
```

Output files are optional; omit `--output` to print the summary JSON. Files must
not already exist, preventing accidental overwrite of evidence. Failed runs can
leave partial files; a source export's header game count and strictly increasing
game IDs detect truncated or duplicated replay input. Choose new output paths
when retrying. `--profile` requires a live database.

Live analysis uses one repeatable-read, read-only transaction, UTC timestamps,
a 60-second statement timeout and batches of 32 games. It neither fetches NHL
data nor updates shifts, events, official stats, fetch outcomes or health state.
All games are observed at the same database snapshot even during another
season's backfill. Replay needs no network. Retain the source export and summary
together: NHL feeds and database snapshots can change.

The library exposes `on_ice::analyze(&GameSource)`, `audit::game` and
`audit::run`. Consumers such as PucksSequences can call the same engine or use
its JSON output. Keep the summary's method version, source snapshot time and
source hash alongside exported events. No permanent event-player bridge or
lineup columns on `events` are created.

## Temporal contract

Both events and shifts use elapsed period time, parsed by
`on_ice::clock`. Seconds must have two digits and be below 60; negative,
overflowed and malformed clocks are rejected, not rounded. Clock disagreement
between raw text and ingestion's parsed columns is an error.

Regulation periods last 1,200 seconds; regular-season period 4 lasts 300
seconds; each playoff OT period lasts 1,200 seconds. Shootouts are not timed
OT. These lengths follow the [NHL rulebook](https://media.nhl.com/site/asset/public/ext/2025-26/2025-26Rules.pdf).
The model does not assume modern three-on-three applies to every historical OT:
actual counts are compared with the event's NHL situation code.

A clock retains `(period, period_seconds, game_seconds)`. Period 1 at 20:00 and
period 2 at 00:00 share an elapsed game second but are different positions.
Intervals never intersect across periods. Event IDs only order output; they do
not establish a relationship with a shift ending at the same second.

At an event timestamp:

- `before`: players active immediately before it, within that period.
- `after`: players active immediately after it, within that period.
- `definite`: players in both limits.
- `possible`: players in either limit, plus zero-duration source candidates.
- Each side lists separate skater, goalie and unknown-role player IDs, deduplicated
  by player. `source_shift_ids` and `source_flags` retain the underlying evidence.

At a change from A to B at 00:30, both A and B are possible; neither is definite.
The engine does not choose one using the event type or situation code. Adjacent
intervals for the same player establish continuous presence, even at their
shared boundary. Zero-duration rows contribute no TOI and only an instantaneous
possible identity. Before/after are limiting states, **not an enumeration of all
possible orders** of simultaneous changes. Period-start/end events retain this
uncertainty rather than borrowing a lineup from the adjacent period.

## Validation and reconciliation

Every issue has a stable code, severity, source shift IDs, player/period when
known, and detail. Input rows are never altered or deleted.

Errors exclude a row from derived interval calculations: missing/invalid
identity, foreign game, unexpected type/team/period, malformed endpoints,
stored/raw clock disagreement, reversed or out-of-period intervals.
NHL team IDs resolve through `nhl_team_identities` to the franchise IDs in
`games`; raw IDs are never joined directly to franchise IDs.

Warnings retain calculable intervals: duration-text problems, duration mismatch,
zero duration, duplicate intervals, same-player overlaps, unknown role, unusually
long skater shifts and suspicious counts. Thresholds are diagnostic, not league
rules: skater intervals over 180 seconds and more than 100 accepted shift rows
per player/game. Multi-OT games can legitimately trigger them. Positive-length
segments with fewer than three skaters, more than six skaters, multiple goalies
or more than six total players are reported. Empty nets alone are not anomalies.
Counts are player counts, not shift-row counts. A suspicious segment is one issue;
duplicate/overlap issues are per row pair, not unique bad-row counts.

Missing team/period data, shift periods without events, conflicting roles or
official rows, and players assigned to both teams are also reported. A rejected
row taints reconstruction throughout its known valid period; an unplaceable
period taints the game. This deliberately avoids assuming that an invalid row
would have affected only a conveniently narrow window. Missing expected
team/period coverage also taints that period.

Roles use per-game official skater/goalie records first. Current player metadata
is an explicit fallback, counted in the report. Unknown/conflicting roles remain
unknown. Official observations include their revision and timestamp in the
replay input.

For every player appearing in shifts or official game stats, TOI exposes:

- Sum of accepted interval lengths, union of those intervals per period, and
  overlap excess. Union counts a player once and does **not** rewrite the source.
- Sum of parseable source duration strings and number that cannot be parsed.
- Official TOI and signed difference: interval union minus official seconds.
- Exact, 1-second, 2–5-second, 6–30-second or larger discrepancy; missing official,
  missing shifts, invalid/partial intervals, conflicting or invalid official
  reference, and official zero with no shifts.

A missing positive-TOI player's shift time is null, not zero. Partial intervals
can expose a provisional difference but are excluded from the comparable
distribution. The distribution includes complete accepted interval sets with a
unique nonnegative official reference, including official zero/no-shift rows.
Median is the lower middle absolute difference; p95 uses nearest rank. Individual
source warnings are still relevant even when TOI matches exactly.

## Reading reconstruction quality

`resolved` means the accepted intervals imply one player set and there are no
structural incompleteness reasons. It does **not** certify source correctness.
`ambiguous` means possible and definite identities differ. `incomplete` and
`ambiguous_incomplete` indicate rejected/missing data, unknown roles, too few
possible skaters or impossible definite player counts. Other statuses explicitly
identify invalid event clocks, unsupported scope and absent shifts.

Situation-code comparison is a separate axis. Only NHL `situation_code`
provenance qualifies; historical inferred strength is not an independent oracle.
The raw code is decoded again and checked against its stored component columns.

| Result | Meaning |
| --- | --- |
| `exact` | One derived identity set; skater counts and number of goalies equal the code. Can still accompany an incomplete status. |
| `boundary_compatible` | Every expected count lies within the definite/possible bounds. This neither identifies players nor proves a joint event ordering. |
| `mismatch` | At least one expected count lies outside those bounds; fields are named. |
| `unavailable` / `invalid_code` | No eligible reference / malformed reference. |
| `not_comparable` | Unsupported/invalid/missing reconstruction or unknown player roles. |

`before_counts_match` and `after_counts_match` expose whether either limiting
state matches the complete count tuple, without selecting that state. Context
tags identify period boundaries, coincident changes, OT, expected goalie absence,
delayed-penalty events, penalty/goal/stoppage events, source warnings, and a
penalty-shot-like count pattern. They overlap; do not sum them as disjoint causes.
A delayed-penalty event is not proof that every nearby empty net was caused by
that penalty.

Penalty shots can occur while the game clock is stopped. Continuous intervals
cannot identify that special one-shooter/one-goalie setup; the count pattern is
flagged and disagreements remain visible. No players are removed to force a match.
Boundary-compatible counts also cannot validate which five skaters were present.

For an initial conservative PucksSequences subset, require `resolved` **and**
`exact`, inspect source flags and game/player TOI, and keep exclusions visible.
The summary reports this joint subset both with and without source warnings.
That is an operational filter, not an accuracy probability. Do not use the union
of possible players as though they were all on ice together.

## Coverage, artifacts and performance

`observability.shift_game_coverage` and `shift_season_coverage` expose stored
availability separately from the latest fetch attempt. Stored rows take
precedence over a failed/unavailable refresh. No recorded attempt does not mean
no historical attempt. Pre-2010 games are unsupported; their eligible count is
zero and loaded fraction is null. Existing event-health flags are unchanged.

The audit covers completed type-2/type-3 games in the local game catalog, not an
independent certification that the schedule itself is complete. It separately
reports missing events and official TOI. Summary maps are sparse: absent
categories have count zero. Event coverage uses every stored event in that
scope, including terminal events; it is not filtered to make agreement look
better.

`--snapshot-out` exports a header followed by each game's analysis inputs.
`--games-out` exports validation and player TOI details; `--events-out` exports
all event reconstructions. Source hashes are SHA-256 over normalized serialized
analysis inputs, including resolved metadata and observation timestamps, not
over every unrelated column in the database. Season hash is SHA-256 over ordered
game-hash hex strings. The summary includes all game hashes and bounded examples.
Library callers should preserve reader ordering when comparing input hashes.

The reader uses existing game-leading indexes. Reconstruction sweeps ordered
boundaries once per period, with small active-player sets; it does not join every
season event to every shift. Source memory is bounded to 32 games, plus one
game's derived events; the audit retains compact per-game summaries and TOI
difference statistics. `--profile` captures real EXPLAIN ANALYZE/BUFFERS plans
for the first batch. Timing fields are milliseconds; network reads and CPU
analysis are reported separately. See the [2025–26 audit](audits/20252026-shifts.md).

## Optional independent report sample

`scripts/check_on_ice_html.py` compares an event export with **cached modern**
NHL HTML on-ice reports and the corresponding JSON PBP roster. It does not fetch
or ingest shifts, update the database, or change reconstructed lineups.

```sh
curl -fL -A 'Mozilla/5.0'   https://www.nhl.com/scores/htmlreports/20252026/PL020077.HTM -o sample.html
curl -fL https://api-web.nhle.com/v1/gamecenter/2025020077/play-by-play -o sample-pbp.json
python3 scripts/check_on_ice_html.py --events event-lineups.jsonl   --report sample.html --pbp sample-pbp.json --output sample-comparison.json
```

It matches only unique period/elapsed-time/event-type keys, reports skipped
duplicates and unknown jerseys, resolves jersey numbers through the per-game
roster, and compares actual player-ID sets and goalie roles. It preserves
ambiguity, records input hashes, and refuses empty validation. Retain all cached
inputs to replay exactly. A small purposive sample from another NHL presentation
is useful corroboration, not independent video ground truth or a random
season-wide accuracy estimate. The helper is scoped to the modern table layout.
