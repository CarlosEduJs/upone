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
cargo build -q --manifest-path "$ROOT/Cargo.toml"

fail=0

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

  # upone now exits non-zero when a task fails; capture the code instead of
  # letting `set -e` abort the whole run.
  set +e
  out="$(cd "$dir" && "$UPONE" up --yes 2>&1)"
  rc=$?
  set -e

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
    js-pnpm)        check_dry_run "$dir" "check pnpm installed" "pnpm install" ;;
    js-npm)         check_dry_run "$dir" "check npm installed" "npm install" ;;
    js-bun)         check_dry_run "$dir" "check bun installed" "bun install" ;;
    stack-docker)   check_dry_run "$dir" "check docker installed" "docker compose up" "verify postgres is running" "verify redis is running" ;;
    orm-prisma)     check_dry_run "$dir" "check npm installed" "npm install" "prisma generate" ;;
    orm-drizzle)    check_dry_run "$dir" "check pnpm installed" "pnpm install" "drizzle-kit generate" "verify postgres is running" ;;
    monorepo-pnpm)  check_dry_run "$dir" "check pnpm installed" "pnpm install" ;;
    *) echo "WARN [$name]: unknown example, skipping"; echo ;;
  esac

  if [[ $EXEC -eq 1 ]]; then
    case "$name" in
      rust-hello|js-pnpm|js-npm|js-bun|orm-prisma|monorepo-pnpm)
        check_exec "$dir" ;;
      stack-docker|orm-drizzle)
        if docker_reachable; then
          check_exec "$dir"
        else
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

echo "ALL EXAMPLES VERIFIED"