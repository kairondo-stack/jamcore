#!/usr/bin/env bash
# E2E: rrn5 Videoeingabe (Testvideo) — laufendes Bild, Einfrieren bei Pixelauswahl,
# Shared-Memory-Veröffentlichung, Fehlerpfad bei fehlender Datei.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

rm -f /dev/shm/MyImageShm /dev/shm/ZeitfaktoSharedMemory
python3 - <<'PY'
with open("/dev/shm/ZeitfaktoSharedMemory", "wb") as f:
    f.write(b"\x00" * 1024)
PY

start_xvfb :89
(cd "$ROOT" && cargo build -q --manifest-path rrn5/Cargo.toml)
(cd "$ROOT" && cargo run -q --manifest-path rrn5/Cargo.toml --example make_testvideo -- "$TMP/testvideo.avi")
assert_file "$TMP/testvideo.avi"

pixel0() { python3 "$DIR/shm_dump.py" | grep "^pixel0="; }

LAST_PID=0
run_video() { # run_video <log> <keys...>; PID steht danach in LAST_PID
  local log="$1"; shift
  printf '200 150\n' | DISPLAY="$DISPLAY" E2E_KEYS="$*" \
    "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" \
    --video "$TMP/testvideo.avi" --shm every >"$log" 2>&1 &
  LAST_PID=$!
  APP_PIDS+=("$LAST_PID")
}

log "Laufendes Video wird veröffentlicht und bewegt sich"
rm -f /dev/shm/MyImageShm
run_video "$TMP/rrn5_video_moving.log" "wait:5000,Quit"
p1="$LAST_PID"
wait_for 5 test -e /dev/shm/MyImageShm || fail "MyImageShm nicht angelegt"
sleep 1.0
samples=""
for _ in 1 2 3 4 5; do
  samples="$samples $(pixel0)"
  sleep 0.25
done
unique="$(tr ' ' '\n' <<<"$samples" | sort -u | grep -c 'pixel0=' || true)"
[ "$unique" -ge 2 ] || fail "Video steht, obwohl es laufen sollte:$samples"
assert_contains "$TMP/rrn5_video_moving.log" "Video geladen"
wait "$p1" 2>/dev/null || true

log "Bei gehaltener linker Maustaste steht das Bild"
rm -f /dev/shm/MyImageShm
run_video "$TMP/rrn5_video_frozen.log" "wait:500,mousedown:100:100,wait:5000,Quit"
p2="$LAST_PID"
wait_for 5 test -e /dev/shm/MyImageShm || fail "MyImageShm nicht angelegt"
sleep 1.5
f1="$(pixel0)"
sleep 0.5
f2="$(pixel0)"
sleep 0.5
f3="$(pixel0)"
[ "$f1" = "$f2" ] && [ "$f2" = "$f3" ] || fail "Bild bewegt sich trotz Maus-Down: $f1 | $f2 | $f3"
wait "$p2" 2>/dev/null || true

log "Live-Scan (freilaufend + taktsynchron) sendet OSC und rescannt Frames"
rm -f /dev/shm/MyImageShm
python3 "$DIR/osc_listen.py" 7200 "$TMP/rrn5_live_osc.jsonl" 10 &
APP_PIDS+=("$!")
sleep 0.5
printf '32 24\n' | DISPLAY="$DISPLAY" RRN5_OSC_BASE_PORT=7200 \
  E2E_KEYS="wait:400,mousedown:16:12,wait:300,mouseup,wait:200,6,wait:2000,2,wait:1500,Quit" \
  "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" \
  --video "$TMP/testvideo.avi" --shm off >"$TMP/rrn5_live.log" 2>&1 &
p3=$!
APP_PIDS+=("$p3")
wait_for 8 grep -q "Live-Scan an" "$TMP/rrn5_live.log" || fail "Live-Scan nicht gestartet: $(tail -3 "$TMP/rrn5_live.log")"
wait_for 8 test -s "$TMP/rrn5_live_osc.jsonl" || fail "keine OSC-Noten beim Live-Scan"
assert_contains "$TMP/rrn5_live_osc.jsonl" "trigger/note_on"
wait_for 8 grep -q "Sweep beendet" "$TMP/rrn5_live.log" || fail "kein Sweep-Abschluss im Log"
wait_for 8 grep -q "taktsynchron" "$TMP/rrn5_live.log" || fail "Taktmodus nicht aktiv"
wait "$p3" 2>/dev/null || fail "Live-Scan-Lauf Exit != 0"

log "Fehlende Videodatei -> klare Meldung, App läuft weiter"
printf '200 150\n' | E2E_KEYS=Quit \
  "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" --video "$TMP/fehlt.avi" \
  >"$TMP/rrn5_video_missing.log" 2>&1 || fail "rrn5 Exit != 0 bei fehlender Datei"
assert_contains "$TMP/rrn5_video_missing.log" "konnte nicht geöffnet werden"

log "OK"
