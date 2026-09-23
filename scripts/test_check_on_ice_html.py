import unittest
from check_on_ice_html import compare, report_rows


class SampleComparisonTests(unittest.TestCase):
    def test_elapsed_clock_nested_players_and_separate_goalie_role(self):
        row = ('<tr class="oddColor"><td>1</td><td>1</td><td>EV</td>'
               '<td>0:10<br>19:50</td><td>SHOT</td><td>Test</td>'
               '<td><table><tr><td><font title="Goalie - A">30</font></td></tr></table></td>'
               '<td><font title="Center - B">12</font></td></tr>')
        rows = report_rows(row)
        self.assertEqual(rows, [((1, 10, "shot-on-goal"), {
            "report_event_id": 1, "lineup": {"away": [(30, "goalies")], "home": [(12, "skaters")]}})])

    def test_unique_matches_and_bounds_do_not_resolve_ambiguous_identities(self):
        lineup = {"home": {"skaters": [12], "goalies": [], "unknown": []},
                  "away": {"skaters": [], "goalies": [30], "unknown": []}}
        e = dict(game_id=123, event_id_in_game=1, period=1, time_in_period="00:10",
                 event_type="shot-on-goal", status="resolved", definite=lineup, possible=lineup)
        row = ((1, 10, "shot-on-goal"), {"report_event_id": 5,
               "lineup": {"home": [(12, "skaters")], "away": [(30, "goalies")]}})
        pbp = dict(id=123, homeTeam={"id": 1}, awayTeam={"id": 2}, rosterSpots=[
            {"teamId": 1, "playerId": 12, "sweaterNumber": 12},
            {"teamId": 2, "playerId": 30, "sweaterNumber": 30}])
        self.assertEqual(compare([e], [row], pbp)["counts"]["exact_identity"], 1)
        e["status"] = "ambiguous"
        e["definite"] = {"home": {"skaters": [], "goalies": [], "unknown": []}, "away": lineup["away"]}
        self.assertEqual(compare([e], [row], pbp)["counts"]["identity_bounds_compatible"], 1)
        self.assertEqual(compare([e, e], [row], pbp)["counts"]["nonunique_key_events_skipped"], 2)
        self.assertEqual(compare([e], [row, row], pbp)["counts"]["nonunique_key_events_skipped"], 1)


if __name__ == "__main__":
    unittest.main()
