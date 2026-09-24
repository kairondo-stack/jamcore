#!/usr/bin/env bash
# E2E: pattern_recorder nimmt Rhythmus auf, speichert CSV, sendet OSC und
# exportiert ein Renoise-Pattern ins Clipboard.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

WORK="$TMP/pattern_recorder_work"
rm -rf "$WORK"
mkdir -p "$WORK"
cp "$ROOT/sagan.ttf" "$WORK/" 2>/dev/null || true

start_xvfb :94
(cd "$ROOT" && cargo build -q --manifest-path pattern_recorder/Cargo.toml)

log "OSC-Mitschnitt 7005"
python3 "$DIR/osc_listen.py" 7005 "$TMP/pr_osc.jsonl" 15 &
APP_PIDS+=("$!")
sleep 0.5

log "Starte pattern_recorder (injizierte Tastenfolge)"
pr_log="$TMP/pr.log"
E2E_KEYS="R,1,wait:150,1,wait:150,2,wait:150,3,wait:150,0,J,1,S,1,H,1,S,Down,Left,6,4,Return" \
  bash -c "cd '$WORK' && SDL_AUDIODRIVER=dummy '$ROOT/pattern_recorder/target/debug/pattern_recorder' >'$pr_log' 2>&1" &
pr_pid=$!
APP_PIDS+=("$pr_pid")

wait_for 10 grep -q "Recording... Press '0' to stop." "$pr_log" || fail "Aufnahme nicht gestartet: $(cat "$pr_log")"

wait_for 10 test -s "$WORK/pattern 0.csv" || fail "CSV nicht erzeugt: $(cat "$pr_log")"
assert_file "$WORK/pattern 0.csv"
lines=$(wc -l <"$WORK/pattern 0.csv")
[ "$lines" -ge 2 ] || fail "CSV hat zu wenige Zeilen: $lines"
awk -F, 'NF != 4 { exit 1 }' "$WORK/pattern 0.csv" || fail "CSV-Spalten != 4"
grep -Eq ',[01]$' "$WORK/pattern 0.csv" || fail "CSV glide nicht 0/1"

log "OSC-Playback"
wait_for 10 test -s "$TMP/pr_osc.jsonl" || fail "kein OSC auf 7005: $(cat "$pr_log")"
assert_contains "$TMP/pr_osc.jsonl" "trigger/note_on"

log "Renoise-Pattern ins Clipboard (Länge 64)"
wait_for 10 test -s /tmp/renoise_pattern.xml || fail "XML nicht erzeugt: $(cat "$pr_log")"
assert_contains /tmp/renoise_pattern.xml "PatternClipboard.BlockBuffer"
assert_contains /tmp/renoise_pattern.xml "ColumnType>NoteColumn"
python3 - <<'PY' || fail "Line-Indizes nicht eindeutig"
import re
xml = open("/tmp/renoise_pattern.xml").read()
idxs = re.findall(r'<Line index="(\d+)"', xml)
assert len(idxs) == 64, len(idxs)
assert len(set(idxs)) == 64, "doppelte Line-Indizes"
PY
xclip_out="$(xclip -selection clipboard -o 2>/dev/null || true)"
grep -q "PatternClipboard.BlockBuffer" <<<"$xclip_out" || fail "Clipboard enthält kein Pattern-XML"

log "OK"
