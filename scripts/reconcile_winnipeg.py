#!/usr/bin/env python3
"""Audit current Winnipeg attribution; --apply repairs verified source games atomically.

No third-party Python dependencies. Uses curl and psql. This is deliberately a
targeted correction for NHL team 33, not an unrestricted franchise replacement.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
API = "https://api.nhle.com/stats/rest/en/"


def database_url(name: str) -> str:
    if os.environ.get(name):
        return os.environ[name]
    for line in (ROOT / ".env").read_text().splitlines():
        key, sep, value = line.partition("=")
        if sep and key.strip() == name:
            return value.strip().strip("\"'")
    raise RuntimeError(f"{name} is not configured")


def fetch(url: str) -> dict:
    result = subprocess.run(
        ["curl", "--fail", "--silent", "--show-error", "--max-time", "30",
         "--retry", "2", "--user-agent", "pucksdata-attribution-audit/1", url],
        capture_output=True, text=True,
    )
    if result.returncode:
        raise RuntimeError(f"NHL request failed: {result.stderr}")
    return json.loads(result.stdout)


def source_catalog() -> tuple[list[dict], list[dict]]:
    teams = fetch(API + "team?limit=-1")["data"]
    games: list[dict] = []
    total = None
    while total is None or len(games) < total:
        response = fetch(API + f"game?limit=500&start={len(games)}&sort=id&dir=asc&"
                         "cayenneExp=homeTeamId%3D33%20or%20visitingTeamId%3D33")
        if total is not None and total != response["total"]:
            raise RuntimeError("NHL catalogue changed during pagination; retry")
        total = response["total"]
        if total < 0 or not response["data"]:
            raise RuntimeError("Empty or truncated NHL catalogue")
        games.extend(response["data"])
    if len(games) != total:
        raise RuntimeError("NHL catalogue count mismatch")
    return teams, games


def expected_games(teams: list[dict], games: list[dict]) -> list[dict]:
    mapping = {row["id"]: row.get("franchiseId") for row in teams}
    if len(mapping) != len(teams):
        raise RuntimeError("Duplicate NHL team identity")
    for team, franchise in {33: 35, 11: 35, 52: 35, 27: 28, 53: 28}.items():
        if mapping.get(team) != franchise:
            raise RuntimeError(f"Unexpected NHL affiliation for team {team}; review required")
    if len({g["id"] for g in games}) != len(games):
        raise RuntimeError("Duplicate NHL game identity")
    expected = []
    for game in games:
        if (game["homeTeamId"] == 33) == (game["visitingTeamId"] == 33):
            raise RuntimeError("NHL catalogue returned an unrelated or ambiguous game")
        if game["gameType"] not in (2, 3):
            continue
        if not 19791980 <= game["season"] <= 19951996:
            raise RuntimeError("Original Winnipeg game is outside its NHL seasons")
        home, away = mapping.get(game["homeTeamId"]), mapping.get(game["visitingTeamId"])
        opponent = away if game["homeTeamId"] == 33 else home
        if opponent is None or opponent in (28, 35):
            raise RuntimeError("Ambiguous opponent attribution; cannot infer event owner")
        expected.append(dict(game_id=game["id"], season=game["season"],
                             game_date=game["gameDate"], game_type=game["gameType"],
                             home=home, away=away))
    if Counter(g["game_type"] for g in expected) != {2: 1338, 3: 62}:
        raise RuntimeError("Original Winnipeg population differs from NHL Records totals; review required")
    return expected


def expected_relation(games: list[dict]) -> str:
    # Hex encoding avoids interpolating source strings into SQL syntax.
    payload = json.dumps(games).encode().hex()
    return f"""SELECT * FROM jsonb_to_recordset(
        convert_from(decode('{payload}', 'hex'), 'UTF8')::jsonb)
        AS x(game_id bigint, season integer, game_date date, game_type smallint,
             home bigint, away bigint)"""


REPORT_SQL = """
WITH affected_events AS MATERIALIZED (
 SELECT e.* FROM expected x CROSS JOIN LATERAL (
   SELECT * FROM events WHERE game_id=x.game_id OFFSET 0
 ) e
)
SELECT jsonb_build_object(
 'identity_franchise', (SELECT franchise_id FROM nhl_team_identities WHERE nhl_team_id=33),
 'source_games', (SELECT count(*) FROM expected),
 'stored_games', (SELECT count(*) FROM games JOIN expected USING(game_id)),
 'missing_game_ids', (SELECT coalesce(jsonb_agg(x.game_id ORDER BY x.game_id),'[]') FROM expected x
                      LEFT JOIN games g USING(game_id) WHERE g.game_id IS NULL),
 'unexpected_game_ids', (SELECT coalesce(jsonb_agg(g.game_id ORDER BY g.game_id),'[]') FROM games g
    WHERE g.season BETWEEN 19791980 AND 19951996 AND g.game_type IN (2,3)
      AND (g.home_team_id IN (28,35) OR g.away_team_id IN (28,35))
      AND NOT EXISTS(SELECT 1 FROM expected x WHERE x.game_id=g.game_id)),
 'invalid_games', (SELECT count(*) FROM games g JOIN expected x USING(game_id)
    WHERE ROW(g.season,g.game_date,g.game_type) IS DISTINCT FROM ROW(x.season,x.game_date,x.game_type)
       OR NOT (g.home_team_id=x.home OR (x.home=35 AND g.home_team_id=28))
       OR NOT (g.away_team_id=x.away OR (x.away=35 AND g.away_team_id=28))),
 'games_to_update', (SELECT count(*) FROM games g JOIN expected x USING(game_id)
    WHERE ROW(g.home_team_id,g.away_team_id) IS DISTINCT FROM ROW(x.home,x.away)),
 'events', (SELECT count(*) FROM affected_events),
 'events_to_update', (SELECT count(*) FROM affected_events WHERE event_owner_team_id=28),
 'events_already_current', (SELECT count(*) FROM affected_events WHERE event_owner_team_id=35),
 'invalid_event_owners', (SELECT count(*) FROM affected_events e JOIN expected x USING(game_id)
    WHERE e.event_owner_team_id NOT IN (28,x.home,x.away)),
 'game_facts_checksum', (SELECT md5(string_agg((to_jsonb(g)-ARRAY['home_team_id','away_team_id'])::text,
                            '' ORDER BY g.game_id)) FROM games g JOIN expected USING(game_id)),
 'event_facts_checksum', (SELECT md5(string_agg((to_jsonb(e)-'event_owner_team_id')::text,
                            '' ORDER BY e.id)) FROM affected_events e),
 'control_games_checksum', (SELECT md5(string_agg(to_jsonb(g)::text,'' ORDER BY g.game_id)) FROM games g
    WHERE (g.home_team_id IN (28,35) OR g.away_team_id IN (28,35))
      AND NOT EXISTS(SELECT 1 FROM expected x WHERE x.game_id=g.game_id)),
 'by_season', (SELECT jsonb_agg(to_jsonb(s) ORDER BY season,game_type) FROM (
    SELECT x.season,x.game_type,count(g.game_id) AS games,
           count(*) FILTER(WHERE g.home_team_id=28 OR g.away_team_id=28) AS games_to_update
    FROM expected x LEFT JOIN games g USING(game_id) GROUP BY x.season,x.game_type) s)
) AS report
"""


def sql_for(games: list[dict], apply: bool) -> str:
    expected = expected_relation(games)
    if not apply:
        return ("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL statement_timeout='120s';\n"
                f"WITH expected AS ({expected}) SELECT * FROM ({REPORT_SQL}) audit; ROLLBACK;")
    return f"""
BEGIN;
SET LOCAL lock_timeout='5s';
SET LOCAL statement_timeout='120s';
DO $$ BEGIN
 IF NOT pg_try_advisory_xact_lock(hashtextextended('pucksdata:ingestion',0)) THEN
   RAISE EXCEPTION 'Another ingestion command is running; retry after it finishes';
 END IF;
END $$;
LOCK TABLE games, events, nhl_team_identities IN SHARE ROW EXCLUSIVE MODE;
CREATE TEMP TABLE expected ON COMMIT DROP AS {expected};
ALTER TABLE expected ADD PRIMARY KEY(game_id);
ANALYZE expected;
CREATE TEMP TABLE audit_before ON COMMIT DROP AS {REPORT_SQL};
DO $$ DECLARE r jsonb; BEGIN
 SELECT report INTO r FROM audit_before;
 IF r->>'identity_franchise' IS NULL OR (r->>'identity_franchise')::bigint NOT IN (28,35)
    OR (r->>'invalid_games')::bigint<>0 OR (r->>'invalid_event_owners')::bigint<>0
    OR r->'unexpected_game_ids'<>'[]'::jsonb THEN
   RAISE EXCEPTION 'Ambiguous attribution or unexpected population; run the audit and review';
 END IF;
 IF NOT EXISTS(SELECT 1 FROM teams WHERE team_id=35) THEN
   RAISE EXCEPTION 'Winnipeg franchise 35 is missing';
 END IF;
END $$;
UPDATE games g SET home_team_id=x.home, away_team_id=x.away FROM expected x
 WHERE g.game_id=x.game_id AND ROW(g.home_team_id,g.away_team_id) IS DISTINCT FROM ROW(x.home,x.away);
UPDATE events e SET event_owner_team_id=35 FROM expected x
 WHERE e.game_id=x.game_id AND e.event_owner_team_id=28;
UPDATE nhl_team_identities SET franchise_id=35, observed_at=clock_timestamp()
 WHERE nhl_team_id=33 AND franchise_id=28;
UPDATE analytics.coverage SET note=
 'Games and event ownership use current NHL franchise attribution. Original Winnipeg (1979-1996), Atlanta and current Winnipeg resolve to Winnipeg; Phoenix/Arizona (1996-2024) resolve to Arizona. Source team identities remain distinct in nhl_team_identities.'
 WHERE subject='historical_team_names' AND note IS DISTINCT FROM
 'Games and event ownership use current NHL franchise attribution. Original Winnipeg (1979-1996), Atlanta and current Winnipeg resolve to Winnipeg; Phoenix/Arizona (1996-2024) resolve to Arizona. Source team identities remain distinct in nhl_team_identities.';
CREATE TEMP TABLE audit_after ON COMMIT DROP AS {REPORT_SQL};
DO $$ DECLARE b jsonb; a jsonb; k text; BEGIN
 SELECT report INTO b FROM audit_before; SELECT report INTO a FROM audit_after;
 IF (a->>'games_to_update')::bigint<>0 OR (a->>'events_to_update')::bigint<>0
    OR (a->>'identity_franchise')::bigint<>35 THEN
   RAISE EXCEPTION 'Attribution reconciliation did not complete';
 END IF;
 FOREACH k IN ARRAY ARRAY['stored_games','events','missing_game_ids','game_facts_checksum',
                          'event_facts_checksum','control_games_checksum'] LOOP
   IF a->k IS DISTINCT FROM b->k THEN RAISE EXCEPTION 'Unexpected change in %',k; END IF;
 END LOOP;
END $$;
SELECT jsonb_build_object('before',b.report,'after',a.report) FROM audit_before b CROSS JOIN audit_after a;
COMMIT;
"""


def execute(url: str, sql: str) -> dict:
    result = subprocess.run(["psql", "--dbname", url, "-XAtq", "-v", "ON_ERROR_STOP=1"],
                            input=sql, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr.replace(url, "[database URL]"))
    return json.loads(result.stdout)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="Commit the verified repair; default is read-only")
    parser.add_argument("--database-env", default="DATABASE_URL", help="Environment/dotenv variable containing the connection URL")
    parser.add_argument("--output", type=Path, help="Write the audit/repair report as JSON")
    args = parser.parse_args()
    teams, games = source_catalog()
    expected = expected_games(teams, games)
    report = dict(observed_at=datetime.now(timezone.utc).isoformat(), applied=args.apply,
                  source_game_count=len(games), excluded_other_game_types=len(games)-len(expected),
                  result=execute(database_url(args.database_env), sql_for(expected, args.apply)))
    output = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output)
    print(output, end="")


if __name__ == "__main__":
    main()
