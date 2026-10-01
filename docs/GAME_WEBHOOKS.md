# Committed game update events

`game.data.updated` means a complete official player/game snapshot was committed.
It is not a live score or merely a final game-state notification. Initial accepted
reports and corrections create events; unchanged observations and rejected partial
reports do not. Events are inserted by a database trigger in the same transaction
as normalized history, including writes from the existing full sync. One game's
failure cannot suppress another game's committed event.

The first version supports one configured consumer. Adding multiple consumers
requires per-subscription delivery receipts; do not switch the URL to fan out an
existing outbox. Events are prospective: migration does not manufacture notifications
for all historical data. Existing daily sync and downstream reconciliation remain
necessary during rollout and as recovery mechanisms.

## Polling and delivery

Run `pucksdata sync-games` every five minutes. It uses the existing ingestion lease,
refreshes known recent game metadata from boxscores, then attempts complete official
reports. Incomplete reports retry on subsequent passes; successful final reports
are rechecked hourly for three days. Normal full sync still discovers schedule
changes, audits older failures/corrections, updates roster metadata, and refreshes
analytics. `sync-games` does not advance the full-sync success watermark and does
not claim event/shift/derived data was refreshed. A full sync holding the writer
lease can defer a fast pass; the next cron retries.

Run `pucksdata deliver-webhooks` every minute in a separate service. It does not need
the ingestion lease and can deliver successfully committed games while other
imports are incomplete. Configure only this service with:

- `PUCKSDATA_WEBHOOK_URL`: one trusted receiver URL; redirects are disabled.
- `PUCKSDATA_WEBHOOK_SECRET`: a randomly generated secret of at least 32 bytes.
- `DATABASE_URL`: ingestion credentials with access to the outbox.

Use HTTPS for public delivery, or the platform's private network. Never put secrets
in CLI arguments or commit them to source. HTTP timeout is five seconds; passes
stop claiming events after forty seconds or 100 attempts. Row locks serialize
concurrent delivery. Non-2xx responses and transport failures retry from 30 seconds
up to one hour, without dropping the event. The operator can inspect `last_error`
and `attempts`; response bodies and destination credentials are not logged.

Suggested Railway commands (UTC, independent of daylight saving):

| Service | Cron | Command |
| --- | --- | --- |
| Recent games | `*/5 * * * *` | `timeout --signal=TERM --kill-after=10s 4m pucksdata sync-games` |
| Delivery | `* * * * *` | `timeout --signal=TERM --kill-after=5s 50s pucksdata deliver-webhooks` |

Use restart policy NEVER for these cron services. Keep the existing full-sync
schedule. Runtime caps allow subsequent cron executions even after a stalled run.

## Wire contract

```json
{"schema_version":1,"event_id":"b278ae09-e3ba-4482-95de-e13406517b3b","type":"game.data.updated","game_id":2026020001,"revision":1,"season":20262027,"game_date":"2026-10-01"}
```

POST the exact UTF-8 JSON body with `Content-Type: application/json`,
`X-PucksData-Timestamp` (Unix seconds), and `X-PucksData-Signature` (`v1=` plus
lowercase hexadecimal HMAC-SHA256 of `timestamp + "." + raw_body`). Each delivery
attempt has a fresh timestamp/signature but retains its event ID and body.
Consumers should enforce a five-minute clock tolerance, reject invalid signatures,
and acknowledge only after durable enqueue. At-least-once delivery permits both
duplicates and out-of-order revisions. Read current committed facts; never overwrite
newer state with values from an old event. `analytics.official_game_revisions`
provides the latest complete accepted revision without granting history-table access.

## Rollout and checks

1. Apply migration 0041 before enabling emitters/dispatchers. It adds only new
   objects and copies the existing official-stat writer and reader grants.
2. Deploy the consumer's durable authenticated intake and worker before delivery.
   Configure the same secret on both ends. Leave delivery disabled until ready.
3. Deploy the updated engine, enable the two cron services, then confirm a real
   committed event is delivered and its consumer job completes. Perform the usual
   full reconciliation to cover data accepted before this migration.
4. Monitor undelivered count/oldest age and repeated `last_error`, plus recent-game
   attempts. Alert on delivery backlog older than ten minutes and independently
   check morning/noon freshness. These are monitoring requirements, not alerts
   automatically installed by this change.

To pause notifications, disable delivery; events remain queued. Stop fast polling
independently if necessary. Keep the additive migration and normal sync running;
rolling back code must not remove accepted history or pending events.
