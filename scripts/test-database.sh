#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

readonly compose_file="compose.test.yml"
export TEST_DATABASE_URL="postgresql://postgres:postgres@localhost:55432/pucksdata_test"

cleanup() {
  docker compose --file "$compose_file" down --volumes
}
trap cleanup EXIT

docker compose --file "$compose_file" up --detach --wait
MIGRATION_DATABASE_URL="$TEST_DATABASE_URL" ./scripts/run-migrations.sh
python3 scripts/check_schema_baseline.py
python3 scripts/check_migration_runner.py
python3 scripts/check_schema_upgrade.py
cargo test --all-targets
