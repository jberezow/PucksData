#!/usr/bin/env python3
"""Exercise migration selection and refusal paths using disposable databases."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
from urllib.parse import urlsplit, urlunsplit
import uuid

ROOT = Path(__file__).resolve().parents[1]


def run(*args: str) -> str:
    result = subprocess.run(args, capture_output=True, text=True, cwd=ROOT)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


def main() -> None:
    maintenance = os.environ["TEST_DATABASE_URL"]
    parsed = urlsplit(maintenance)
    if parsed.scheme not in ("postgres", "postgresql") or "test" not in parsed.path.lower():
        raise SystemExit("TEST_DATABASE_URL must identify a PostgreSQL test database")
    run("cargo", "build", "--quiet", "--bin", "pucksdata-migrate")
    binary = ROOT / os.environ.get("CARGO_TARGET_DIR", "target") / "debug/pucksdata-migrate"
    created: list[str] = []

    def create() -> str:
        name = "pucksdata_runner_test_" + uuid.uuid4().hex[:12]
        run("createdb", "--maintenance-db", maintenance, name)
        created.append(name)
        return urlunsplit(parsed._replace(path=f"/{name}"))

    def sql(url: str, statement: str) -> str:
        return run("psql", url, "-XAtq", "-v", "ON_ERROR_STOP=1", "-c", statement).strip()

    def migrate(url: str, *args: str, failure: str | None = None) -> str:
        result = subprocess.run([str(binary), *args], cwd=ROOT,
                                env={**os.environ, "MIGRATION_DATABASE_URL": url},
                                capture_output=True, text=True)
        if failure is None:
            if result.returncode:
                raise AssertionError(result.stderr)
        elif result.returncode == 0 or failure not in result.stderr:
            raise AssertionError(f"Expected {failure!r}, got {result.stderr!r}")
        return result.stdout

    try:
        fresh = create()
        assert "Pending 38" in migrate(fresh, "--dry-run")
        assert sql(fresh, "SELECT to_regclass('public._sqlx_migrations') IS NULL") == "t"
        assert "baseline" in migrate(fresh)
        assert sql(fresh, "SELECT min(version) FROM _sqlx_migrations") == "38"
        before = sql(fresh, "SELECT jsonb_agg(to_jsonb(m) ORDER BY version) FROM _sqlx_migrations m")
        migrate(fresh)
        migrate(fresh, "--dry-run")
        assert before == sql(fresh, "SELECT jsonb_agg(to_jsonb(m) ORDER BY version) FROM _sqlx_migrations m")
        legacy = create()
        run("sqlx", "migrate", "run", "--no-dotenv", "--database-url", legacy,
            "--source", str(ROOT / "migrations/legacy"), "--target-version", "36")
        assert "Pending 37" in migrate(legacy, "--dry-run")
        assert "legacy" in migrate(legacy)
        migrate(legacy)
        assert sql(legacy, "SELECT count(*) FROM _sqlx_migrations WHERE version<=38") == "38"
        assert sql(legacy, "SELECT franchise_id FROM nhl_team_identities WHERE nhl_team_id=33") == "28"
        sql(fresh, "UPDATE _sqlx_migrations SET success=false WHERE version=38")
        migrate(fresh, "--dry-run", failure="dirty")
        migrate(fresh, failure="dirty")
        sql(fresh, "UPDATE _sqlx_migrations SET success=true, checksum=decode('00','hex') WHERE version=38")
        migrate(fresh, failure="ledger mismatch")
        sql(legacy, "UPDATE _sqlx_migrations SET version=999999 WHERE version=1")
        migrate(legacy, failure="ledger mismatch")
        sql(legacy, "DELETE FROM _sqlx_migrations WHERE version=999999")
        migrate(legacy, failure="ledger mismatch")
        nonempty = create()
        sql(nonempty, "CREATE TABLE untracked(id integer)")
        migrate(nonempty, failure="no migration history")
        assert sql(nonempty, "SELECT to_regclass('public._sqlx_migrations') IS NULL") == "t"
        empty_ledger = create()
        run("sqlx", "migrate", "run", "--no-dotenv", "--database-url", empty_ledger,
            "--source", str(ROOT / "migrations/legacy"), "--target-version", "1")
        sql(empty_ledger, "DELETE FROM _sqlx_migrations")
        migrate(empty_ledger, failure="no migration history")
        print("Migration runner passed: fresh and legacy paths, repeat runs, read-only dry runs, and dirty/unknown/changed/incomplete/untracked refusal.")
    finally:
        for name in reversed(created):
            run("dropdb", "--maintenance-db", maintenance, "--force", name)


if __name__ == "__main__":
    main()
