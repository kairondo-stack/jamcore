#!/usr/bin/env bash
# E2E: Interaktiver Quintenzirkel (5,2) — x per Klick, Note im Kreis wählen,
# OSC-Vorschau, Enter speichert die Harmonie.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

rm -f /dev/shm/MyImageShm /dev/shm/ZeitfaktoSharedMemory
python3 - <<'PY'
with open("/dev/shm/ZeitfaktoSharedMemory", "wb") as f:
    f.write(b"\x00" * 1024)
PY

start_xvfb :88
(cd "$ROOT" && cargo build -q --manifest-path rrn5/Cargo.toml)

log "OSC-Mitschnitt 7300 (Vorschau)"
python3 "$DIR/osc_listen.py" 7300 "$TMP/rrn5_quint_osc.jsonl" 10 &
APP_PIDS+=("$!")
sleep 0.5

log "Quintenzirkel: Anzahl=1, x-Klick, Note C wählen, Enter"
printf '200 150\n' | DISPLAY="$DISPLAY" RRN5_OSC_BASE_PORT=7300 \
  E2E_KEYS="wait:400,3,1,Return,wait:200,5,2,wait:300,mousemove:100:100,mousedown:100:100,mouseup,wait:300,mousedown:410:240,mouseup,wait:300,Return,wait:300,Quit" \
  "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" >"$TMP/rrn5_quint.log" 2>&1 &
p=$!
APP_PIDS+=("$p")

wait_for 8 grep -q "Harmonie 0: x=" "$TMP/rrn5_quint.log" || fail "Harmonie nicht gespeichert: $(tail -4 "$TMP/rrn5_quint.log")"
assert_contains "$TMP/rrn5_quint.log" "notesset=\[0\]"
wait_for 5 test -s "$TMP/rrn5_quint_osc.jsonl" || fail "keine OSC-Vorschau"
assert_contains "$TMP/rrn5_quint_osc.jsonl" ", 60, 127\]"
wait "$p" 2>/dev/null || fail "Quintenzirkel-Lauf Exit != 0"

log "OK"
