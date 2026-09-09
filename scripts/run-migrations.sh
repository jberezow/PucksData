#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

migration_database_url="${MIGRATION_DATABASE_URL:-}"

if [[ -z "$migration_database_url" ]]; then
  if [[ ! -f .env ]]; then
    echo "MIGRATION_DATABASE_URL is unset and .env does not exist" >&2
    exit 1
  fi

  while IFS= read -r env_line; do
    case "$env_line" in
      MIGRATION_DATABASE_URL=*)
        migration_database_url="${env_line#MIGRATION_DATABASE_URL=}"
        break
        ;;
    esac
  done < .env
fi

# Accept the common dotenv forms MIGRATION_DATABASE_URL=value, "value", or 'value'.
if [[ "$migration_database_url" == \"*\" && "$migration_database_url" == *\" ]]; then
  migration_database_url="${migration_database_url:1:${#migration_database_url}-2}"
elif [[ "$migration_database_url" == \'*\' && "$migration_database_url" == *\' ]]; then
  migration_database_url="${migration_database_url:1:${#migration_database_url}-2}"
fi

if [[ -z "$migration_database_url" ]]; then
  echo "MIGRATION_DATABASE_URL is empty" >&2
  exit 1
fi

exec sqlx migrate run --database-url "$migration_database_url" "$@"
