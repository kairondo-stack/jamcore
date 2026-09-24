#!/usr/bin/env bash
# E2E: rrn5 startet unter Xvfb, sendet Preview-OSC und beendet sich auf Fenster-Close.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

rm -f /dev/shm/MyImageShm /dev/shm/ZeitfaktoSharedMemory
python3 - <<'PY'
with open("/dev/shm/ZeitfaktoSharedMemory", "wb") as f:
    f.write(b"\x00" * 1024)
PY

make_fixtures
start_xvfb :95
(cd "$ROOT" && cargo build -q --manifest-path rrn5/Cargo.toml)

log "OSC-Mitschnitt 7000/7002"
python3 "$DIR/osc_listen.py" 7000 "$TMP/rrn5_7000.jsonl" 10 &
APP_PIDS+=("$!")
python3 "$DIR/osc_listen.py" 7002 "$TMP/rrn5_7002.jsonl" 10 &
APP_PIDS+=("$!")
sleep 0.5

log "Starte rrn5 (E2E_KEYS: Tab, Right)"
rrn5_log="$TMP/rrn5.log"
printf '384 384\n' | E2E_KEYS=Tab,Right "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" >"$rrn5_log" 2>&1 &
rrn5_pid=$!
APP_PIDS+=("$rrn5_pid")

wid="$(first_window 10)" || fail "rrn5-Fenster nicht gefunden: $(cat "$rrn5_log")"
sleep 1

wait_for 3 test -s "$TMP/rrn5_7000.jsonl" || fail "kein OSC auf 7000"
assert_contains "$TMP/rrn5_7000.jsonl" "trigger/note_on"
assert_contains "$TMP/rrn5.log" "Tonauswahl:"

log "Surface View (X) öffnen und mit Enter schließen"
printf '384 384\n' | E2E_KEYS=Tab,X,Return,Quit timeout 20 "$ROOT/rrn5/target/debug/rrn5" \
  --dir "$FIXTURES" >"$TMP/rrn5_surface.log" 2>&1 ||
  fail "Surface-View-Lauf Exit != 0: $(tail -3 "$TMP/rrn5_surface.log")"
assert_contains "$TMP/rrn5_surface.log" "surfacesh: true"

log "Sauberer Exit über SDL_QUIT"
printf '384 384\n' | E2E_KEYS=Quit "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" >"$TMP/rrn5_quit.log" 2>&1 &
quit_pid=$!
for _ in $(seq 1 50); do
  if ! kill -0 "$quit_pid" 2>/dev/null; then break; fi
  sleep 0.2
done
if kill -0 "$quit_pid" 2>/dev/null; then
  kill "$quit_pid" 2>/dev/null || true
  fail "rrn5 beendet sich nicht auf SDL_QUIT"
fi
wait "$quit_pid" || fail "rrn5 Exit != 0"

log "OK"
