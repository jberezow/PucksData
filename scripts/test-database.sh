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
DATABASE_URL="$TEST_DATABASE_URL" sqlx migrate run
cargo test --all-targets
