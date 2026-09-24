#!/usr/bin/env bash
# E2E-Helfer. Ausführung innerhalb von nix-shell (SDL2, xdotool, xclip, python3).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES="$ROOT/tests/e2e/fixtures"
TMP="${TMPDIR:-/tmp}/opencode-e2e"
mkdir -p "$TMP"

XVFB_PID=""
APP_PIDS=()

cleanup() {
  for p in "${APP_PIDS[@]:-}"; do
    kill "$p" 2>/dev/null || true
  done
  if [ -n "$XVFB_PID" ]; then
    kill "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

log()  { printf '== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

start_xvfb() {
  local display="${1:-:97}"
  Xvfb "$display" -screen 0 3840x2160x24 >/dev/null 2>&1 &
  XVFB_PID=$!
  export DISPLAY="$display"
  local sock="/tmp/.X11-unix/X${display#:}"
  for _ in $(seq 1 100); do
    [ -e "$sock" ] && break
    sleep 0.1
  done
  [ -e "$sock" ] || fail "Xvfb startete nicht ($display)"
}

stop_xvfb() {
  if [ -n "$XVFB_PID" ]; then
    kill "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
    XVFB_PID=""
  fi
}

first_window() { # first_window <sekunden> -> Fenster-ID des ersten Fensters
  local t="${1:-10}"
  local wid=""
  local end=$((SECONDS + t))
  while [ "$SECONDS" -lt "$end" ]; do
    wid="$(xdotool search --name ".*" 2>/dev/null | sort -n | head -1 || true)"
    if [ -n "$wid" ]; then
      echo "$wid"
      return 0
    fi
    sleep 0.2
  done
  return 1
}

wait_for() { # wait_for <sekunden> <cmd...>
  local t="$1"; shift
  local end=$((SECONDS + t))
  while [ "$SECONDS" -lt "$end" ]; do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 0.1
  done
  return 1
}

wait_for_file() { # wait_for_file <pfad> <sekunden>
  local f="$1" t="${2:-10}"
  wait_for "$t" test -s "$f" || fail "Datei nicht erzeugt: $f"
}

assert_file() { [ -s "$1" ] || fail "Datei fehlt/leer: $1"; }

assert_contains() { # assert_contains <datei> <regex>
  grep -Eq "$2" "$1" || fail "Muster '$2' nicht in $1"
}

assert_not_contains() {
  ! grep -Eq "$2" "$1" || fail "Verbotenes Muster '$2' in $1"
}

run_app() { # run_app <logdatei> <cmd...>
  local logf="$1"; shift
  "$@" >"$logf" 2>&1 &
  APP_PIDS+=("$!")
  echo "${APP_PIDS[-1]}"
}

make_fixtures() {
  python3 "$ROOT/tests/e2e/make_fixtures.py" "$FIXTURES"
}
