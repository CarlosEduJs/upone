#!/usr/bin/env bash
# Validates the upone flow against the example fixtures under examples/.
#
# Usage:
#   scripts/verify-examples.sh          # dry-run only (no execution)
#   scripts/verify-examples.sh --exec   # also runs `upone up --yes` in each example

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
  local dir="$1"
  local name
  name="$(basename "$dir")"

  local out
  out="$(cd "$dir" && "$UPONE" up --yes 2>&1)"

  echo "== [$name] exec =="
  echo "$out"

  if grep -q "tasks failed" <<<"$out"; then
    echo "FAIL [$name]: one or more tasks failed"
    fail=1
  else
    echo "PASS [$name]"
  fi
  echo
}

for dir in "$ROOT"/examples/*/; do
  name="$(basename "$dir")"
  [[ "$name" == _* ]] && continue # skip shared helpers

  case "$name" in
    rust-hello) check_dry_run "$dir" "check cargo installed" "cargo build" ;;
    js-pnpm)    check_dry_run "$dir" "check pnpm installed" "pnpm install" ;;
    js-npm)     check_dry_run "$dir" "check npm installed" "npm install" ;;
    js-bun)     check_dry_run "$dir" "check bun installed" "bun install" ;;
    *) echo "WARN [$name]: unknown example, skipping"; echo ;;
  esac

  if [[ $EXEC -eq 1 ]]; then
    case "$name" in
      rust-hello|js-pnpm|js-npm|js-bun) check_exec "$dir" ;;
      *) : ;;
    esac
  fi
done

if [[ $fail -ne 0 ]]; then
  echo "VERIFY FAILED"
  exit 1
fi

echo "ALL EXAMPLES VERIFIED"
