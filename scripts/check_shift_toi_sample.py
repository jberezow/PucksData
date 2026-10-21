#!/usr/bin/env python3
"""Offline, two-source shift sample audit, not an ingestion fallback.

Use cached NHL shift JSON, home/visitor TOI HTML and a matching PBP roster.
Preserve row multiplicity; never infer a production database's coverage.
"""
import argparse
from collections import Counter
from datetime import date
import hashlib
import json
from pathlib import Path
import re

from check_on_ice_html import Document, Node


def seconds(value):
    if not re.fullmatch(r"\d+:[0-5]\d", value):
        raise ValueError(f"invalid clock: {value!r}")
    minutes, secs = map(int, value.split(":"))
    return minutes * 60 + secs


def html_rows(html, pbp, side):
    root = Document(html).root
    title = [n.text().strip() for n in root.descendants("title")]
    expected = "Home" if side == "home" else "Away"
    if title != [f"Time On Ice Report {expected} Team"]:
        raise ValueError("wrong report kind or side")
    info = [n.text() for n in root.descendants("table") if n.attrs.get("id") == "GameInfo"]
    day = date.fromisoformat(pbp["gameDate"])
    report_date = f"{day.strftime('%A, %B')} {day.day}, {day.year}"
    if len(info) != 1 or report_date not in info[0] or not re.search(
        rf"\bGame\s+{pbp['id'] % 10000:04d}\b", info[0]
    ) or "Final" not in info[0]:
        raise ValueError("report game/date/final status mismatch")
    team = pbp[f"{side}Team"]["id"]
    roster = [p for p in pbp["rosterSpots"] if p["teamId"] == team]
    jerseys = {p["sweaterNumber"]: p["playerId"] for p in roster}
    if len(jerseys) != len(roster):
        raise ValueError("nonunique roster jerseys")
    player = None
    rows = []
    for row in root.descendants("tr"):
        cells = [c for c in row.children if isinstance(c, Node) and c.tag == "td"]
        heading = [c for c in cells if "playerHeading" in c.attrs.get("class", "").split()]
        if heading:
            jersey = int(heading[0].text().strip().split()[0])
            if jersey not in jerseys:
                raise ValueError(f"unknown {side} jersey {jersey}")
            player = jerseys[jersey]
        if not {"oddColor", "evenColor"}.intersection(row.attrs.get("class", "").split()):
            continue
        # Seven-column per-period summaries are not shift rows.
        if len(cells) == 7 and all(" / " not in c.text() for c in cells):
            continue
        if len(cells) != 6 or player is None:
            raise ValueError("unrecognized TOI shift row")
        values = [c.text().strip() for c in cells]
        period = 4 if values[1] == "OT" else int(values[1])
        if not 1 <= period <= 3:
            raise ValueError("sample helper currently supports regulation-only games")
        start, end = [seconds(v.split(" / ")[0]) for v in values[2:4]]
        duration = seconds(values[4])
        if not 0 <= start <= end <= 1200 or end - start != duration:
            raise ValueError("invalid or incoherent HTML interval")
        rows.append((team, player, period, int(values[0]), start, end, duration))
    if not rows:
        raise ValueError("empty HTML shift sample")
    return rows


def compare(source, pbp, home, away):
    if source.get("total") != len(source.get("data", [])) or "data" not in source:
        raise ValueError("incomplete JSON shift response")
    raw = source["data"]
    shifts = []
    for row in raw:
        if not isinstance(row.get("typeCode"), int):
            raise ValueError("missing JSON typeCode")
        if row["typeCode"] != 517:
            continue
        if row["gameId"] != pbp["id"]:
            raise ValueError("foreign game in JSON")
        shifts.append((row["teamId"], row["playerId"], row["period"], row["shiftNumber"],
                       seconds(row["startTime"]), seconds(row["endTime"]), seconds(row["duration"])))
    html = html_rows(home, pbp, "home") + html_rows(away, pbp, "away")
    a, b = Counter(shifts), Counter(html)
    return {"game_id": pbp["id"], "game_date": pbp["gameDate"],
            "json_response_rows": len(raw), "json_shift_rows": len(shifts),
            "html_shift_rows": len(html), "html_players": len({r[1] for r in html}),
            "exact_matching_rows": sum((a & b).values()),
            "json_only_rows": sum((a - b).values()), "html_only_rows": sum((b - a).values()),
            "json_only_examples": list((a - b).elements())[:5],
            "html_only_examples": list((b - a).elements())[:5],
            "source_result": "json_unavailable_html_present" if not shifts else
                "exact_multiset_match" if a == b else "source_difference",
            "database_coverage": "not_observed", "scope": "regulation-only cached source sample"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("json", "pbp", "home", "away", "output"):
        parser.add_argument(f"--{arg}", type=Path, required=True)
    args = parser.parse_args()
    paths = {k: getattr(args, k) for k in ("json", "pbp", "home", "away")}
    content = {k: p.read_bytes() for k, p in paths.items()}
    result = compare(json.loads(content["json"]), json.loads(content["pbp"]),
                     content["home"].decode(), content["away"].decode())
    result["input_sha256"] = {k: hashlib.sha256(v).hexdigest() for k, v in content.items()}
    with args.output.open("x") as out:
        json.dump(result, out, indent=2)
        out.write("\n")


if __name__ == "__main__":
    main()
