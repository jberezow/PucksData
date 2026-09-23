#!/usr/bin/env python3
"""Sample-only independent report comparison. Standard library; no DB writes or network.
Use cached modern NHL HTML play-by-play and JSON PBP roster, plus shifts audit --events-out.
Only unique (period, elapsed second, event type) matches are compared; no guessed ordering.
"""
import argparse
import collections
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path

EVENT_TYPES = {
    "FAC": "faceoff", "SHOT": "shot-on-goal", "GOAL": "goal", "BLOCK": "blocked-shot",
    "MISS": "missed-shot", "HIT": "hit", "GIVE": "giveaway", "TAKE": "takeaway",
    "PENL": "penalty", "STOP": "stoppage", "PSTR": "period-start", "PEND": "period-end",
    "GEND": "game-end", "DELPEN": "delayed-penalty",
}


class Node:
    def __init__(self, tag="", attrs=()):
        self.tag, self.attrs, self.children = tag, dict(attrs), []

    def text(self):
        return "".join(c if isinstance(c, str) else c.text() for c in self.children)

    def descendants(self, tag):
        for child in self.children:
            if isinstance(child, Node):
                if child.tag == tag:
                    yield child
                yield from child.descendants(tag)


class Document(HTMLParser):
    def __init__(self, text):
        super().__init__(convert_charrefs=True)
        self.root = Node()
        self.stack = [self.root]
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        if tag == "br":
            self.stack[-1].children.append(" ")
        node = Node(tag, attrs)
        self.stack[-1].children.append(node)
        if tag not in {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "wbr"}:
            self.stack.append(node)

    def handle_endtag(self, tag):
        for i in range(len(self.stack) - 1, 0, -1):
            if self.stack[i].tag == tag:
                del self.stack[i:]
                break

    def handle_data(self, data):
        self.stack[-1].children.append(data)


def seconds(clock):
    m, s = clock.split(":")
    return int(m) * 60 + int(s)


def report_rows(html):
    rows = []
    for row in Document(html).root.descendants("tr"):
        if not {"evenColor", "oddColor"}.intersection(row.attrs.get("class", "").split()):
            continue
        cells = [c for c in row.children if isinstance(c, Node) and c.tag == "td"]
        if len(cells) != 8 or cells[4].text().strip() not in EVENT_TYPES:
            continue
        key = (int(cells[1].text()), seconds(cells[3].text().split()[0]),
               EVENT_TYPES[cells[4].text().strip()])
        sides = {}
        for side, cell in zip(("away", "home"), cells[6:8]):
            sides[side] = [
                (int(font.text().strip()), "goalies" if font.attrs["title"].startswith("Goalie -") else "skaters")
                for font in cell.descendants("font") if "title" in font.attrs
            ]
        rows.append((key, {"report_event_id": int(cells[0].text()), "lineup": sides}))
    return rows


def compare(events, rows, pbp):
    game_id = pbp["id"]
    rosters = {}
    for side in ("home", "away"):
        team_id = pbp[f"{side}Team"]["id"]
        players = [p for p in pbp["rosterSpots"] if p["teamId"] == team_id]
        rosters[side] = {p["sweaterNumber"]: p["playerId"] for p in players}
        if len(rosters[side]) != len(players):
            raise ValueError("nonunique roster jersey numbers")
    report_by_key, events_by_key = collections.defaultdict(list), collections.defaultdict(list)
    for key, row in rows:
        report_by_key[key].append(row)
    for e in events:
        if e["game_id"] == game_id:
            events_by_key[(e["period"], seconds(e["time_in_period"]), e["event_type"])].append(e)
    counts, breakdown, examples = collections.Counter(), collections.defaultdict(collections.Counter), []
    for key, candidates in events_by_key.items():
        matches = report_by_key[key]
        if len(candidates) != 1 or len(matches) > 1:
            counts["nonunique_key_events_skipped"] += len(candidates)
            continue
        if not matches:
            counts["unmatched_events"] += 1
            continue
        e, row = candidates[0], matches[0]
        if any(not row["lineup"][side] for side in ("home", "away")):
            counts["report_lineup_missing"] += 1
            continue
        if any(jersey not in rosters[side] for side in ("home", "away") for jersey, _ in row["lineup"][side]):
            counts["unknown_report_jersey"] += 1
            continue
        observed = {
            side: {role: sorted(rosters[side][j] for j, r in row["lineup"][side] if r == role)
                   for role in ("skaters", "goalies")}
            for side in ("home", "away")
        }
        if e["status"] not in ("resolved", "ambiguous", "incomplete", "ambiguous_incomplete"):
            counts["not_comparable"] += 1
            continue
        exact, compatible = True, True
        for side in ("home", "away"):
            for role in ("skaters", "goalies"):
                low, high, seen = map(set, (e["definite"][side][role], e["possible"][side][role], observed[side][role]))
                exact &= low == high == seen
                compatible &= low <= seen <= high
            compatible &= not e["possible"][side]["unknown"]
            exact &= not e["possible"][side]["unknown"]
        outcome = "exact_identity" if exact else "identity_bounds_compatible" if compatible else "identity_mismatch"
        counts[outcome] += 1
        breakdown[e["status"]][outcome] += 1
        if outcome == "identity_mismatch" and len(examples) < 10:
            examples.append({"event_id_in_game": e["event_id_in_game"], "report_event_id": row["report_event_id"],
                             "key": key, "observed": observed, "definite": e["definite"], "possible": e["possible"],
                             "situation_agreement": e["situation_agreement"], "contexts": e["contexts"]})
    return {"game_id": game_id, "derived_events": sum(map(len, events_by_key.values())),
            "report_rows": len(rows), "counts": counts, "by_reconstruction_status": breakdown, "examples": examples}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("events", "report", "pbp", "output"):
        p.add_argument("--" + name, required=True, type=Path)
    args = p.parse_args()
    pbp = json.loads(args.pbp.read_bytes())
    # The event export is season-sized: stream it rather than reading it all into RAM.
    event_digest = hashlib.sha256()
    events = []
    with args.events.open("rb") as stream:
        for line in stream:
            e = json.loads(line)
            if e["game_id"] == pbp["id"]:
                events.append(e)
                event_digest.update(line)
    rows = report_rows(args.report.read_text(encoding="utf-8-sig"))
    if not rows or not events:
        raise ValueError("no supported report rows or derived events; refusing an empty validation")
    result = compare(events, rows, pbp)
    game, year = pbp["id"], pbp["id"] // 1000000
    result["sources"] = {
        "html_url": f"https://www.nhl.com/scores/htmlreports/{year}{year+1}/PL{game%1000000:06}.HTM",
        "pbp_roster_url": f"https://api-web.nhle.com/v1/gamecenter/{game}/play-by-play",
        "html_sha256": hashlib.sha256(args.report.read_bytes()).hexdigest(),
        "pbp_sha256": hashlib.sha256(args.pbp.read_bytes()).hexdigest(),
        "selected_event_lines_sha256": event_digest.hexdigest(),
    }
    result["limitation"] = "Purposive sample; shared NHL provenance is not video ground truth. Unique clock/type matches only. No source repairs."
    with args.output.open("x") as out:
        json.dump(result, out, indent=2, sort_keys=True)
        out.write("\n")


if __name__ == "__main__":
    main()
