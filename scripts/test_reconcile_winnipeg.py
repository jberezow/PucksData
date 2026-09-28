"""Independent source checks and real PostgreSQL repair/rollback tests."""
import copy
import os
import subprocess
import unittest
from urllib.parse import urlsplit, urlunsplit
import uuid

import reconcile_winnipeg as repair


class SourceValidation(unittest.TestCase):
    def setUp(self):
        self.teams = [dict(id=t, franchiseId=f) for t, f in
                      [(33, 35), (11, 35), (52, 35), (27, 28), (53, 28), (3, 10)]]
        self.games = [dict(id=i, season=19951996, gameDate="1995-10-07",
                           gameType=2 if i < 1338 else 3, homeTeamId=33, visitingTeamId=3)
                      for i in range(1400)]

    def test_complete_source_and_wrong_mapping(self):
        self.assertEqual(len(repair.expected_games(self.teams, self.games)), 1400)
        self.teams[0]["franchiseId"] = 28
        with self.assertRaises(RuntimeError):
            repair.expected_games(self.teams, self.games)

    def test_missing_duplicate_unrelated_and_ambiguous_source(self):
        for kind in ("missing", "duplicate", "unrelated", "ambiguous", "season"):
            games = copy.deepcopy(self.games)
            if kind == "missing":
                games.pop()
            elif kind == "duplicate":
                games[-1]["id"] = games[0]["id"]
            elif kind == "unrelated":
                games[0]["homeTeamId"] = 52
            elif kind == "ambiguous":
                games[0]["visitingTeamId"] = 27
            else:
                games[0]["season"] = 19961997
            with self.subTest(kind=kind), self.assertRaises(RuntimeError):
                repair.expected_games(self.teams, games)


@unittest.skipUnless(os.environ.get("TEST_DATABASE_URL"), "requires disposable PostgreSQL")
class DatabaseRepair(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.maintenance = os.environ["TEST_DATABASE_URL"]
        parsed = urlsplit(cls.maintenance)
        if "test" not in parsed.path.lower():
            raise RuntimeError("Refusing non-test database")
        cls.name = "attribution_test_" + uuid.uuid4().hex[:12]
        cls.url = urlunsplit(parsed._replace(path="/" + cls.name))
        cls.command("createdb", "--maintenance-db", cls.maintenance, cls.name)
        try:
            for migration in sorted((repair.ROOT / "migrations").glob("*.sql")):
                cls.command("psql", "--dbname", cls.url, "-Xq", "-v", "ON_ERROR_STOP=1", "-f", str(migration))
        except Exception:
            cls.tearDownClass()
            raise

    @classmethod
    def tearDownClass(cls):
        cls.command("dropdb", "--maintenance-db", cls.maintenance, "--force", cls.name)

    @staticmethod
    def command(*args):
        result = subprocess.run(args, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr)

    def setUp(self):
        self.expected = [dict(game_id=1995020010, season=19951996, game_date="1995-10-07", game_type=2, home=35, away=10),
                         dict(game_id=1995030010, season=19951996, game_date="1996-04-20", game_type=3, home=10, away=35)]
        self.command("psql", "--dbname", self.url, "-Xq", "-v", "ON_ERROR_STOP=1", "-c", """
            TRUNCATE events, games CASCADE;
            INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev)
            VALUES(28,'Arizona','Coyotes','Arizona','ARI'),(35,'Winnipeg','Jets','Winnipeg','WPG'),
                  (10,'New York','Rangers','New York','NYR') ON CONFLICT DO NOTHING;
            UPDATE nhl_team_identities SET franchise_id=28 WHERE nhl_team_id=33;
            INSERT INTO games(game_id,season,game_date,game_type,home_team_id,away_team_id,home_score,away_score) VALUES
             (1995020010,19951996,'1995-10-07',2,28,10,7,5),
             (1995030010,19951996,'1996-04-20',3,10,35,2,1),
             (1996020010,19961997,'1996-10-07',2,28,10,1,0),
             (2000020010,20002001,'2000-10-07',2,35,10,4,2);
            INSERT INTO events(game_id,event_id_in_game,period,period_type,time_in_period,event_type,
                               event_owner_team_id,season,game_type,game_date)
            SELECT g.game_id,n,1,'REG','01:00','goal',
                   CASE WHEN n=1 THEN 28 WHEN n=2 THEN 10 ELSE NULL END,
                   g.season,g.game_type,g.game_date FROM games g CROSS JOIN generate_series(1,3) n;
            DELETE FROM ingestion.derived_invalidations;
        """)

    def audit(self):
        return repair.execute(self.url, repair.sql_for(self.expected, False))

    def test_atomic_repair_controls_and_repeat_run(self):
        before = self.audit()
        self.assertEqual((before["games_to_update"], before["events_to_update"]), (1, 2))
        result = repair.execute(self.url, repair.sql_for(self.expected, True))
        after = result["after"]
        self.assertEqual((after["games_to_update"], after["events_to_update"], after["identity_franchise"]), (0, 0, 35))
        for key in ("game_facts_checksum", "event_facts_checksum", "control_games_checksum", "events"):
            self.assertEqual(before[key], after[key])
        control = repair.execute(self.url, "SELECT jsonb_build_object('owners',jsonb_agg(event_owner_team_id ORDER BY game_id,event_id_in_game)) FROM events WHERE game_id IN (1996020010,2000020010)")
        self.assertEqual(control["owners"], [28, 10, None, 28, 10, None])
        dirty = repair.execute(self.url, "SELECT jsonb_build_object('count',count(*)) FROM ingestion.derived_invalidations")
        self.assertGreater(dirty["count"], 0)
        again = repair.execute(self.url, repair.sql_for(self.expected, True))
        self.assertEqual(again["before"], again["after"])

    def test_rejects_bad_participant_and_rolls_back(self):
        bad = copy.deepcopy(self.expected)
        bad[0]["away"] = 99
        before = self.audit()
        with self.assertRaisesRegex(RuntimeError, "Ambiguous attribution"):
            repair.execute(self.url, repair.sql_for(bad, True))
        self.assertEqual(before, self.audit())

    def test_rejects_unknown_owner_and_unlisted_game(self):
        self.command("psql", "--dbname", self.url, "-Xq", "-c",
                     "UPDATE events SET event_owner_team_id=35 WHERE game_id=1995020010 AND event_id_in_game=3")
        # Already-current owners are valid even while game attribution is old.
        self.assertEqual(self.audit()["invalid_event_owners"], 0)
        incomplete = self.expected[:1]
        with self.assertRaisesRegex(RuntimeError, "Ambiguous attribution"):
            repair.execute(self.url, repair.sql_for(incomplete, True))
        self.assertEqual(self.audit()["identity_franchise"], 28)
        self.command("psql", "--dbname", self.url, "-Xq", "-v", "ON_ERROR_STOP=1", "-c", """
            INSERT INTO teams(team_id,full_name,common_name,place_name,abbrev)
            VALUES(1,'Montreal','Canadiens','Montreal','MTL') ON CONFLICT DO NOTHING;
            UPDATE events SET event_owner_team_id=1 WHERE game_id=1995020010 AND event_id_in_game=3;
        """)
        with self.assertRaisesRegex(RuntimeError, "Ambiguous attribution"):
            repair.execute(self.url, repair.sql_for(self.expected, True))
        self.assertEqual(self.audit()["identity_franchise"], 28)

    def test_refuses_concurrent_writer(self):
        holder = subprocess.Popen(["psql", "--dbname", self.url, "-XAtq"],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            holder.stdin.write("BEGIN; SELECT pg_try_advisory_xact_lock(hashtextextended('pucksdata:ingestion',0));\n")
            holder.stdin.flush()
            self.assertEqual(holder.stdout.readline().strip(), "t")
            with self.assertRaisesRegex(RuntimeError, "Another ingestion command"):
                repair.execute(self.url, repair.sql_for(self.expected, True))
        finally:
            holder.communicate("ROLLBACK;\n\\q\n", timeout=10)
        self.assertEqual(self.audit()["identity_franchise"], 28)

    def test_late_error_rolls_back_game_event_and_identity_writes(self):
        before = self.audit()
        sql = repair.sql_for(self.expected, True).replace(
            "CREATE TEMP TABLE audit_after", "SELECT 1/0; CREATE TEMP TABLE audit_after")
        with self.assertRaises(RuntimeError):
            repair.execute(self.url, sql)
        self.assertEqual(before, self.audit())


if __name__ == "__main__":
    unittest.main()
