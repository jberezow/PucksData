# Event data

`public.events` stores the shared play-by-play fields: source event ID, game,
period, clock, location, owning franchise and strength. Detail tables join to
`events.id` through their `event_id` column.

| Detail table | Source event | Main attribution |
| --- | --- | --- |
| `goals` | `goal` | Scorer, assists and goalie |
| `shots` | `shot-on-goal` and `goal` | Shooter and goalie |
| `missed_shots` | `missed-shot` | Shooter, goalie, shot type and source miss reason |
| `hits` | `hit` | Hitter and recipient |
| `blocks` | `blocked-shot` | Shooter and blocker |
| `penalties` | `penalty` | Committing and drawing players |
| `faceoffs` | `faceoff` | Winner and loser |
| `giveaways` | `giveaway` | Player charged with the giveaway |
| `takeaways` | `takeaway` | Player credited with the takeaway |

Missed shots are not shots on goal. Goals already appear in `shots`, so adding
the two table counts would double-count goals. Shootout attempts are excluded
from normalized event facts.

The stable source identity is `(game_id, event_id_in_game)`. A correction reload
replaces a game's parent and detail rows transactionally; internal `events.id`
values can change. Readers should not retain those internal IDs across reloads.

## Coverage and missing attribution

A base event can exist without player attribution. Nullable detail fields mean
the source did not supply that information; they do not establish that a player
had zero events. Older ingestions may also have base missed-shot, giveaway and
takeaway events without corresponding detail rows.

Inspect those separately:

```sql
SELECT season, game_type, event_type,
       base_event_count, typed_event_count, attributed_event_count,
       missing_typed_event_count, games_with_base_events
FROM analytics.event_fact_coverage
ORDER BY season, game_type, event_type;
```

`typed_event_count` counts extracted detail rows, including rows whose player is
unknown. `attributed_event_count` counts rows with a shooter or credited player.
Neither proves that every scheduled game or every upstream event is available.
Use `analytics.coverage` for declared source eras and
`analytics.coverage_observed` for broader database coverage.

## Corrections and history

Normal fetches and backfills include all detail tables in the same transaction
as their parent events. Removed source events remove their previous details;
failed replacements leave the previous game intact.

Accepted event snapshots use `normalized-events-v2`, adding `missed_shot`,
`giveaway` and `takeaway` payload fields. Older revisions retain their original
shape and timestamps. The first enriched snapshot may therefore be a new
revision even when the underlying NHL response has not changed. See
[ingestion history](ingestion-history.md) for the distinction between source
observations and normalized revisions.

## Enriching previously loaded games

After upgrading, ordinary event refreshes populate the new detail tables. To
reuse archived responses without contacting the NHL, preview a bounded replay:

```bash
pucksdata replay-event-details --season 20252026 --limit 100
pucksdata replay-event-details --season 20252026 --game-id 2025020001 --apply
```

The default limit is 100 games, ordered by game ID. Use `--after-game-id` with
the last reported game ID to continue through a season, or `--game-id` to inspect
one game. The JSON result identifies eligible, unchanged and rejected games;
any rejection produces a nonzero exit status after the report is printed.
Replay only
inserts missing missed-shot, giveaway and takeaway details. It verifies the
current accepted snapshot, complete event inventory, matchup and existing facts
against the archived response before writing. It preserves parent IDs and
previously enriched strength fields. Conflicting facts, ambiguous archives or
missing accepted-source evidence are rejected; replay does not repair them.

Each accepted replay links its new revision to the original source receipt in
`ingestion.event_replays`. The source receipt keeps its original timestamp;
the newly extracted facts are recorded at replay time. A repeated apply does
not manufacture another revision. Dry runs do not create attempts or history.

For games without usable archives, an explicit event fetch or season refresh
can obtain a new source observation. That refresh records knowledge acquired
now and cannot recreate earlier observation history.
