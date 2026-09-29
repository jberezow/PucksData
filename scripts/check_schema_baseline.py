#!/usr/bin/env python3
"""Compare the version-38 baseline with its archived SQLx migration chain.

Creates and removes two disposable databases using TEST_DATABASE_URL only.
Requires PostgreSQL client tools, sqlx, and CREATEDB permission.
"""
from __future__ import annotations

import difflib
import hashlib
import json
import os
from pathlib import Path
import subprocess
from urllib.parse import urlsplit, urlunsplit
import uuid

ROOT = Path(__file__).resolve().parents[1]


def run(*args: str) -> str:
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


def normalized_schema(url: str) -> str:
    dump = run("pg_dump", url, "--schema-only", "--no-owner", "--no-privileges",
               "--exclude-table=public._sqlx_migrations")
    return "\n".join(line for line in dump.splitlines() if not line.startswith("\\"))


def main() -> None:
    checksums = json.loads((ROOT / "migrations/legacy/checksums.json").read_text())
    for name, checksum in checksums.items():
        if hashlib.sha384((ROOT / "migrations/legacy" / name).read_bytes()).hexdigest() != checksum:
            raise AssertionError(f"Archived migration changed: {name}")
    maintenance = os.environ["TEST_DATABASE_URL"]
    parsed = urlsplit(maintenance)
    if parsed.scheme not in ("postgres", "postgresql") or "test" not in parsed.path.lower():
        raise SystemExit("TEST_DATABASE_URL must identify a PostgreSQL test database")
    created: list[str] = []
    urls: list[str] = []

    def sql(url: str, statement: str) -> str:
        return run("psql", url, "-XAtq", "-v", "ON_ERROR_STOP=1", "-c", statement).strip()

    try:
        for label, source in (("legacy", "migrations/legacy"), ("baseline", "schema/baseline")):
            name = f"pucksdata_{label}_test_{uuid.uuid4().hex[:12]}"
            run("createdb", "--maintenance-db", maintenance, name)
            created.append(name)
            url = urlunsplit(parsed._replace(path=f"/{name}"))
            urls.append(url)
            run("sqlx", "migrate", "run", "--no-dotenv", "--database-url", url,
                "--source", str(ROOT / source))
            for relation, columns in json.loads((ROOT / "tests/contracts/consumer.json").read_text())["relations"].items():
                names = ", ".join(f'"{name}"' for name, _ in columns)
                sql(url, f"SELECT {names} FROM {relation} LIMIT 0")
            if sql(url, "SELECT count(*) FROM history.snapshots") != "0":
                raise AssertionError("Static install seeds unexpectedly created historical facts")
            if sql(url, "SELECT count(*) FROM pg_matviews WHERE schemaname IN ('analytics','observability') AND ispopulated") != "3":
                raise AssertionError("Fresh materialized views are not ready for reads")
        schemas = [normalized_schema(url) for url in urls]
        if schemas[0] != schemas[1]:
            difference = "\n".join(difflib.unified_diff(schemas[0].splitlines(), schemas[1].splitlines(),
                                                       fromfile="legacy", tofile="baseline"))
            raise AssertionError("Baseline schema differs from archived migrations:\n" + difference)
        for relation in ("analytics.coverage", "public.nhl_team_identities"):
            statements = []
            for url in urls:
                # Install times vary; Winnipeg's source namespace correction is intentional.
                query = f"SELECT COALESCE(jsonb_agg(row ORDER BY row::text),'[]') FROM (SELECT to_jsonb(t)-'observed_at' AS row FROM {relation} t"
                if relation.endswith("nhl_team_identities"):
                    query += " WHERE nhl_team_id<>33"
                statements.append(sql(url, query + ") q"))
            if statements[0] != statements[1]:
                raise AssertionError(f"Static seeds differ: {relation}")
        if sql(urls[1], "SELECT franchise_id FROM nhl_team_identities WHERE nhl_team_id=33") != "35":
            raise AssertionError("Fresh installs retain the obsolete Winnipeg source mapping")
        print("Baseline passed: identical schema, consumer columns, coverage and identity seeds, initialized views, no fabricated history.")
    finally:
        for name in reversed(created):
            run("dropdb", "--maintenance-db", maintenance, "--force", name)


if __name__ == "__main__":
    main()
