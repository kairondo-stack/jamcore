#!/usr/bin/env bash
# E2E: it_live lädt ein IT-Modul, dumpt die Struktur und rendert hörbares WAV.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

MODULE="${1:-$HOME/Documents/drumnbaesh.it}"
[ -f "$MODULE" ] || fail "Testmodul fehlt: $MODULE"

log "Baue it_live"
(cd "$ROOT" && cargo build -q)

log "Dump + WAV"
RLENGTH=3 "$ROOT/target/debug/it_live" "$MODULE" --dump --wav "$TMP/it.wav" >"$TMP/it_dump.log" 2>&1 || fail "it_live Exit != 0"

assert_contains "$TMP/it_dump.log" "patterns="
assert_contains "$TMP/it_dump.log" "orders="
assert_contains "$TMP/it_dump.log" "sample "
assert_contains "$TMP/it_dump.log" "WAV geschrieben"

log "WAV-Signal prüfen"
python3 "$DIR/wav_check.py" "$TMP/it.wav" || fail "WAV ist still"

log "OK"
