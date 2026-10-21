import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from check_shift_toi_sample import compare, html_rows, seconds

PBP = {"id": 2024021235, "gameDate": "2025-04-08", "homeTeam": {"id": 7},
       "awayTeam": {"id": 12}, "rosterSpots": [
           {"teamId": 7, "sweaterNumber": 4, "playerId": 101},
           {"teamId": 12, "sweaterNumber": 5, "playerId": 102}]}


def report(side, duplicate=False):
    jersey = 4 if side == "Home" else 5
    row = '<tr class="oddColor">' + ''.join(f'<td>{v}</td>' for v in
        [1, 1, '0:36 / 19:24', '1:16 / 18:44', '00:40', '']) + '</tr>'
    return (f'<title>Time On Ice Report {side} Team</title>'
            '<table id="GameInfo"><tr><td>Tuesday, April 8, 2025 Game 1235 Final</td></tr></table>'
            f'<table><tr><td class="playerHeading + border">{jersey} NAME</td></tr>'
            + row * (2 if duplicate else 1) + '</table>')


def source():
    return {"total": 2, "data": [
        {"gameId": PBP['id'], "typeCode": 517, "teamId": team, "playerId": player,
         "period": 1, "shiftNumber": 1, "startTime": '00:36', "endTime": '01:16',
         "duration": '00:40'} for team, player in [(7, 101), (12, 102)]]}


class SampleAudit(unittest.TestCase):
    def test_exact_match_and_non_shift_rows(self):
        data = source()
        data['data'].append({'typeCode': 505})
        data['total'] += 1
        result = compare(data, PBP, report('Home'), report('Away'))
        self.assertEqual(result['source_result'], 'exact_multiset_match')
        self.assertEqual(result['exact_matching_rows'], 2)
        self.assertEqual(result['json_response_rows'], 3)
        self.assertEqual(result['database_coverage'], 'not_observed')

    def test_missing_json_is_not_database_missing(self):
        result = compare({'data': [], 'total': 0}, PBP, report('Home'), report('Away'))
        self.assertEqual(result['html_only_rows'], 2)
        self.assertEqual(result['source_result'], 'json_unavailable_html_present')
        self.assertEqual(result['database_coverage'], 'not_observed')

    def test_duplicates_are_not_deduplicated(self):
        result = compare(source(), PBP, report('Home', duplicate=True), report('Away'))
        self.assertEqual(result['html_only_rows'], 1)
        self.assertEqual(result['source_result'], 'source_difference')

    def test_rejects_foreign_or_incomplete_json(self):
        data = source()
        data['total'] = 3
        with self.assertRaises(ValueError):
            compare(data, PBP, report('Home'), report('Away'))
        data = source()
        data['data'][0]['gameId'] += 1
        with self.assertRaises(ValueError):
            compare(data, PBP, report('Home'), report('Away'))

    def test_rejects_wrong_game_date_side_and_empty_html(self):
        for bad in [report('Home').replace('1235', '1236'),
                    report('Home').replace('2025', '2024'), report('Away'),
                    report('Home').replace('oddColor', 'unknown')]:
            with self.assertRaises(ValueError):
                html_rows(bad, PBP, 'home')

    def test_rejects_unknown_or_nonunique_roster(self):
        pbp = copy.deepcopy(PBP)
        pbp['rosterSpots'][0]['sweaterNumber'] = 99
        with self.assertRaises(ValueError):
            html_rows(report('Home'), pbp, 'home')
        pbp = copy.deepcopy(PBP)
        pbp['rosterSpots'].append(pbp['rosterSpots'][0])
        with self.assertRaises(ValueError):
            html_rows(report('Home'), pbp, 'home')

    def test_clock_and_interval_validation(self):
        for value in ['1:60', '-1:00', '1:2', 'bad']:
            with self.assertRaises(ValueError):
                seconds(value)
        for before, after in [('00:40', '00:39'), ('1:16 /', '0:30 /'),
                              ('<td>1</td><td>1</td>', '<td>1</td><td>OT</td>')]:
            with self.assertRaises(ValueError):
                html_rows(report('Home').replace(before, after), PBP, 'home')

    def test_cli_hashes_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            inputs = {'json': json.dumps(source()), 'pbp': json.dumps(PBP),
                      'home': report('Home'), 'away': report('Away')}
            cmd = [sys.executable, str(Path(__file__).with_name('check_shift_toi_sample.py'))]
            for name, content in inputs.items():
                (path / name).write_text(content)
                cmd.extend([f'--{name}', str(path / name)])
            cmd.extend(['--output', str(path / 'result.json')])
            subprocess.run(cmd, check=True, capture_output=True)
            original = (path / 'result.json').read_bytes()
            result = json.loads(original)
            self.assertEqual(len(result['input_sha256']), 4)
            self.assertNotEqual(subprocess.run(cmd, capture_output=True).returncode, 0)
            self.assertEqual((path / 'result.json').read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
