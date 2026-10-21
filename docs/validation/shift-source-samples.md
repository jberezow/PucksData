# Historical shift-source sample audit, 2026-10-21

## Findings and scope

This is a two-game purposive **upstream source** sample, not a production
coverage audit or season-wide estimate. The source JSON and both official HTML
reports were retrieved successfully (HTTP 200) on 2026-10-21 at 05:32–05:33 UTC.
Input SHA-256 hashes, exact URLs and results are retained in
[the result manifest](shift-source-samples-20261021.json).

- **2024021235, Carolina at Buffalo, 2025-04-08:** NHL JSON returned `total: 0`
  and an empty data array. Home and away HTML supplied 719 regulation shift rows
  for 38 players, all mapped through the game's PBP roster. This is a currently
  confirmed example from the already documented 2024021235–2024021291 upstream
  gap in [OPERATIONS](../../OPERATIONS.md). It is not evidence that the current
  database lacks this game's shifts.
- **2025020077, Islanders at Ottawa, 2025-10-18:** JSON contained 781 response
  rows, including 771 type-517 shift rows. All 771 matched HTML exactly as a
  multiset of team ID, player ID, period, shift number, start, end and duration.
  The ten non-shift rows were excluded explicitly, not counted as missing shifts.

The external [Edgework methodology](https://edgeworkstats.com/methodology)
reports a 2025–26 JSON gap and an HTML recovery. Its season count is not verified
here and does not establish this database's coverage. The confirmed missing
sample above belongs to **2024–25**, not 2025–26.

## Repeatable offline comparison

`scripts/check_shift_toi_sample.py` reuses the existing HTML document parser.
It performs no network access, database access, ingestion or row replacement.
It refuses empty HTML, mismatched report side/game/date, unknown or ambiguous
jersey mappings, malformed/incoherent HTML clocks, incomplete JSON and foreign
game rows. It preserves duplicate multiplicity and input hashes; output uses
exclusive creation to avoid overwriting evidence. Its intentionally narrow scope
is final regulation-only modern TOI reports. It rejects OT rather than inventing
period handling. This is not a production HTML fallback or a replacement for
Rust validation, TOI reconciliation or event reconstruction.

Cache each game's four inputs at the URLs in the manifest, preserving original
bytes. Then, substituting the selected game ID:

```sh
python3 scripts/check_shift_toi_sample.py \
  --json data/shift-audit-20261021/2025020077.json \
  --pbp data/shift-audit-20261021/2025020077-pbp.json \
  --home data/shift-audit-20261021/2025020077-home.html \
  --away data/shift-audit-20261021/2025020077-away.html \
  --output data/shift-audit-20261021/2025020077-comparison.json
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Cached source files remain under ignored `data/`, as prescribed by the existing
snapshot policy; they are not included in this change. Hashes can detect later
NHL corrections but cannot reconstruct old bytes: retain those original files
with the output for exact replay. Re-downloading after corrections may produce
legitimate differences. Synthetic unit fixtures test the comparator offline and
are not presented as independently observed NHL data.

## What remains unverified

No current production coverage export was available. No shift-specific entries
were returned by the games service's retained logs for 2026-10-01 through
2026-10-21 05:30 UTC; routine event ingestion does not run shift backfills, so
this absence is not evidence of coverage. No database connection or query was
made, no backfill ran and no fallback was enabled.

The minimum next evidence is a current read-only export of
`observability.shift_season_coverage` plus the `shift_game_coverage` row for
2024021235 (including stored row count and latest attempt separately). Prefer an
already retained export. If absent, obtain it during an existing maintenance
window rather than polling. A stored snapshot may survive an unavailable refresh.

If a stored gap is confirmed, use the existing `shifts audit --snapshot-out`
and offline `--input` flow in [on-ice reconstruction](../on-ice-reconstruction.md)
for current loaded games before designing recovery. These two source comparisons
do not certify player TOI against official skater/goalie records, event-level
lineups, full-season coverage, downstream totals, or other HTML layouts/eras.
A fallback needs separate validation and review; this change does not propose one.
