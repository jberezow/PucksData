#!/usr/bin/env python3
"""Verify the 0036 -> latest upgrade and Consumer reads in a disposable database.

Uses TEST_DATABASE_URL only as the maintenance connection. Creates its own
random database/roles, and removes only those resources. Requires createdb,
dropdb, psql, CREATEDB and CREATEROLE (the CI PostgreSQL user has these).
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
from urllib.parse import urlsplit, urlunsplit
import uuid

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = json.loads((ROOT / "tests/contracts/consumer.json").read_text())
OPERATIONS = ["backfill_progress", "sync_state", "shift_fetch_status"]


def run(*args: str) -> str:
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode:
        # Do not print connection arguments or SQL containing role credentials.
        raise RuntimeError(result.stderr)
    return result.stdout


def main() -> None:
    maintenance = os.environ["TEST_DATABASE_URL"]
    parsed = urlsplit(maintenance)
    if parsed.scheme not in ("postgres", "postgresql") or "test" not in parsed.path.lower():
        raise SystemExit("TEST_DATABASE_URL must be a PostgreSQL URL with 'test' in the database name")
    suffix = uuid.uuid4().hex[:12]
    database = f"pucksdata_schema_upgrade_test_{suffix}"
    database_url = urlunsplit(parsed._replace(path=f"/{database}"))
    roles = {f"{kind}_role": f"schema_test_{kind}_{suffix}"
             for kind in ("reader", "column", "default", "writer")}
    created_roles: list[str] = []
    created_database = False
    variables = [arg for key, value in roles.items() for arg in ("-v", f"{key}={value}")]

    def sql(statement: str) -> str:
        return run("psql", database_url, "-XAtq", "-v", "ON_ERROR_STOP=1", "-c", statement).strip()

    def apply(path: Path, *, atomic: bool = False) -> None:
        run("psql", database_url, "-Xq", "-v", "ON_ERROR_STOP=1",
            *(["--single-transaction"] if atomic else []), *variables, "-f", str(path))

    def reader_snapshot() -> dict:
        snapshot = {}
        for relation in [*CONTRACT["relations"], *(f"public.{name}" for name in OPERATIONS)]:
            # Relation names come exclusively from the checked-in contract above.
            snapshot[relation] = json.loads(sql(
                f'SET ROLE "{roles["reader_role"]}"; '
                f"SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM {relation} t"
            ))
        parameters = {
            "scoring": ["ARRAY[20252026]::integer[]", "'2026-01-01'::date", "'2026-01-02'::date"],
            "draft": ["20252026", "20252026", "20252026"],
            "schedule": ["20252026", "'2026-01-01 00:00Z'::timestamptz", "20252026"],
        }
        for name, values in parameters.items():
            query = (ROOT / f"tests/contracts/consumer_{name}.sql").read_text()
            for value in values:
                query = query.replace("%s", value, 1)
            if "%s" in query:
                raise AssertionError(f"Unbound Consumer query parameter: {name}")
            rows = json.loads(sql(
                f'SET ROLE "{roles["reader_role"]}"; '
                "SELECT COALESCE(jsonb_agg(to_jsonb(q) ORDER BY to_jsonb(q)::text),'[]'::jsonb) FROM (\n"
                + query + "\n) q"
            ))
            if not rows:
                raise AssertionError(f"Consumer query fixture returned no rows: {name}")
            snapshot[f"query:{name}"] = rows
        return snapshot

    try:
        run("createdb", "--maintenance-db", maintenance, database)
        created_database = True
        for role in roles.values():
            sql(f'CREATE ROLE "{role}" NOLOGIN')
            created_roles.append(role)
        migrations = sorted((ROOT / "migrations").glob("*.sql"))
        for migration in migrations:
            if int(migration.name.split("_", 1)[0]) <= 36:
                apply(migration)  # Includes CREATE INDEX CONCURRENTLY migrations.
        apply(ROOT / "tests/schema_upgrade/before.sql")
        before = reader_snapshot()
        oids = {name: sql(f"SELECT 'public.{name}'::regclass::oid") for name in OPERATIONS}
        for migration in migrations:
            if int(migration.name.split("_", 1)[0]) > 36:
                apply(migration, atomic=True)
        if before != reader_snapshot():
            raise AssertionError("Existing Consumer or legacy operational read results changed")
        for name, oid in oids.items():
            if oid != sql(f"SELECT 'ingestion.{name}'::regclass::oid"):
                raise AssertionError(f"Operational table identity changed: {name}")
        apply(ROOT / "tests/schema_upgrade/after.sql")
        print("Schema upgrade passed: Consumer rows, legacy reader grants, canonical upserts, history continuity, triggers and FK cascades.")
    finally:
        if created_database:
            run("dropdb", "--maintenance-db", maintenance, "--force", database)
        for role in reversed(created_roles):
            run("psql", maintenance, "-Xq", "-v", "ON_ERROR_STOP=1", "-c", f'DROP ROLE "{role}"')


if __name__ == "__main__":
    main()
