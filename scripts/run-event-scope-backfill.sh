#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

scope_database_url="${DATABASE_URL:-}"

if [[ -z "$scope_database_url" ]]; then
  if [[ ! -f .env ]]; then
    echo "DATABASE_URL is unset and .env does not exist" >&2
    exit 1
  fi

  while IFS= read -r env_line; do
    case "$env_line" in
      DATABASE_URL=*)
        scope_database_url="${env_line#DATABASE_URL=}"
        break
        ;;
    esac
  done < .env
fi

# Accept the common dotenv forms DATABASE_URL=value, "value", or 'value'.
if [[ "$scope_database_url" == \"*\" && "$scope_database_url" == *\" ]]; then
  scope_database_url="${scope_database_url:1:${#scope_database_url}-2}"
elif [[ "$scope_database_url" == \'*\' && "$scope_database_url" == *\' ]]; then
  scope_database_url="${scope_database_url:1:${#scope_database_url}-2}"
fi

if [[ -z "$scope_database_url" ]]; then
  echo "DATABASE_URL is empty" >&2
  exit 1
fi

psql "$scope_database_url" \
  --set ON_ERROR_STOP=1 \
  --file scripts/backfill-event-scope.sql
