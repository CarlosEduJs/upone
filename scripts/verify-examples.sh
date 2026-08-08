#!/usr/bin/env bash
# Validates the upone flow against the example fixtures under examples/.
#
# Usage:
#   scripts/verify-examples.sh          # dry-run only (no execution)
#   scripts/verify-examples.sh --exec    # also runs `upone up --yes` in each example

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPONE="$ROOT/target/debug/upone"
EXEC=0
[[ "${1:-}" == "--exec" ]] && EXEC=1

echo "== building upone =="
cargo build --locked -q --manifest-path "$ROOT/Cargo.toml"

fail=0
skipped=0

check_dry_run() {
  local dir="$1"
  local name
  name="$(basename "$dir")"
  local -a need=("${@:2}")

  local out
  out="$(cd "$dir" && "$UPONE" up --dry-run 2>&1)"

  echo "== [$name] dry-run =="
  echo "$out"

  local ok=1
  if ! grep -q "Plan created" <<<"$out"; then
    echo "FAIL [$name]: expected a plan, got none"
    ok=0
  fi
  local tok
  for tok in "${need[@]}"; do
    if ! grep -qF "$tok" <<<"$out"; then
      echo "FAIL [$name]: expected plan to contain: $tok"
      ok=0
    fi
  done

  if [[ $ok -eq 1 ]]; then
    echo "PASS [$name]"
  else
    fail=1
  fi
  echo
}

check_exec() {
  local dir="$1" name rc out
  name="$(basename "$dir")"

  # upone now exits non-zero when a task fails; capture the code without
  # toggling the global `set -e`.
  if out="$(cd "$dir" && "$UPONE" up --yes 2>&1)"; then
    rc=0
  else
    rc=$?
  fi

  echo "== [$name] exec (exit $rc) =="
  echo "$out"

  if [[ $rc -ne 0 ]] || grep -q "tasks failed" <<<"$out"; then
    echo "FAIL [$name]: one or more tasks failed"
    fail=1
  else
    echo "PASS [$name]"
  fi
  echo
}

docker_reachable() { docker info >/dev/null 2>&1; }

for dir in "$ROOT"/examples/*/; do
  name="$(basename "$dir")"
  [[ "$name" == _* ]] && continue # skip shared helpers

  case "$name" in
rust-hello)     check_dry_run "$dir" "check cargo installed" "cargo build" ;;
    go-hello)       check_dry_run "$dir" "check go installed" "go mod tidy" "go build ./..." ;;
    js-pnpm)        check_dry_run "$dir" "check pnpm installed" "pnpm install" ;;
    js-npm)         check_dry_run "$dir" "check npm installed" "npm install" ;;
    js-bun)         check_dry_run "$dir" "check bun installed" "bun install" ;;
    js-yarn)        check_dry_run "$dir" "check yarn installed" "yarn install" ;;
    py-uv)          check_dry_run "$dir" "check uv installed" "uv sync" ;;
    py-poetry)      check_dry_run "$dir" "check poetry installed" "poetry install" ;;
    py-pip)         check_dry_run "$dir" "check python installed" "create project venv" "pip install" ;;
    ruby-hello)     check_dry_run "$dir" "check ruby/bundler installed" "bundle install" ;;
    php-hello)      check_dry_run "$dir" "check php/composer installed" "composer install" ;;
    stack-docker)   check_dry_run "$dir" "check docker installed" "docker compose up" "verify postgres is running" "verify redis is running" ;;
    stack-mysql)    check_dry_run "$dir" "check docker installed" "docker compose up" "verify mysql is running" ;;
    stack-mongo)    check_dry_run "$dir" "check docker installed" "docker compose up" "verify mongodb is running" ;;
    sqlite-hello)   check_dry_run "$dir" "ensure sqlite database file" ;;
    mongoose)       check_dry_run "$dir" "check docker installed" "docker compose up" "check npm installed" "npm install" "verify mongodb is running" ;;
    orm-prisma)     check_dry_run "$dir" "check npm installed" "npm install" "prisma generate" ;;
    orm-drizzle)    check_dry_run "$dir" "check pnpm installed" "pnpm install" "drizzle-kit generate" "verify postgres is running" ;;
    monorepo-pnpm)  check_dry_run "$dir" "check pnpm installed" "pnpm install" ;;
    monorepo-bun)   check_dry_run "$dir" "check bun installed" "bun install" "check drizzle-kit available" "drizzle-kit generate" "check postgres is running" ;;
    *) echo "WARN [$name]: unknown example, skipping"; echo ;;
  esac

  if [[ $EXEC -eq 1 ]]; then
    case "$name" in
      rust-hello|js-pnpm|js-npm|js-bun|js-yarn|orm-prisma|monorepo-pnpm|monorepo-bun|go-hello|py-uv|py-poetry|py-pip|ruby-hello|php-hello|sqlite-hello)
        check_exec "$dir"
        case "$name" in
          monorepo-bun)
            # Remove what `bun install` and `drizzle-kit generate` wrote so the
            # fixture stays clean for the next run.
            rm -rf "$dir/node_modules" "$dir/packages/db/node_modules" "$dir/packages/db/src/migrations" ;;
          sqlite-hello)
            # Remove the database file sqlite-ensure created.
            rm -f "$dir/app.db" ;;
          py-uv|py-pip|py-poetry)
            # Remove the venv each provider may have created so the fixture
            # stays clean for the next run.
            rm -rf "$dir/.venv" ;;
          js-yarn)
            rm -rf "$dir/node_modules" "$dir/.yarn" ;;
          ruby-hello)
            rm -f "$dir/Gemfile.lock" ;;
          php-hello)
            rm -rf "$dir/vendor" && rm -f "$dir/composer.lock" ;;
        esac
        ;;
      stack-docker|stack-mysql|stack-mongo|orm-drizzle|mongoose)
        if docker_reachable; then
          check_exec "$dir"
          case "$name" in
            mongoose)
              # Remove what `npm install` wrote so the fixture stays clean.
              rm -rf "$dir/node_modules" ;;
          esac
          # Tear the example down so a later docker example can bind the same
          # host ports without colliding (both fixtures publish localhost:5432).
          (cd "$dir" && docker compose down -v >/dev/null 2>&1) || true
        else
          skipped=1
          echo "SKIP [$name] exec: docker not available"; echo
        fi ;;
      *) : ;;
    esac
  fi
done

if [[ $fail -ne 0 ]]; then
  echo "VERIFY FAILED"
  exit 1
fi

if [[ $skipped -ne 0 ]]; then
  echo "ALL EXAMPLES VERIFIED (with skipped exec)"
  exit 0
fi

echo "ALL EXAMPLES VERIFIED"