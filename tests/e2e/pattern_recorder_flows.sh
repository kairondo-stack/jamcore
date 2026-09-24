#!/usr/bin/env bash
# E2E: weitere pattern_recorder-Flows — CSV-Bulk, Schism-Export, Renoise-Import.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

start_xvfb :93
(cd "$ROOT" && cargo build -q --manifest-path pattern_recorder/Cargo.toml)
PR="$ROOT/pattern_recorder/target/debug/pattern_recorder"

run_pr() { # run_pr <name> <keys> -> startet App im eigenen Arbeitsverzeichnis
  local name="$1" keys="$2"
  local work="$TMP/pr_$name"
  rm -rf "$work"
  mkdir -p "$work"
  cp "$ROOT/sagan.ttf" "$work/" 2>/dev/null || true
  E2E_KEYS="$keys" timeout 60 bash -c "cd '$work' && SDL_AUDIODRIVER=dummy '$PR' >'$TMP/pr_$name.log' 2>&1" &
  APP_PIDS+=("$!")
  echo "$work"
}

REC_KEYS="R,1,wait:150,1,wait:150,2,wait:150,3,wait:150,0,J,1"

log "Phase A: CSV-Bulk S+9 / F+9"
WORK_A="$(run_pr bulk "$REC_KEYS,S,9,F,9,Quit")"
wait_for 15 grep -q "loaded from pattern.csv" "$TMP/pr_bulk.log" || fail "Bulk-Load nicht gelaufen: $(tail -5 "$TMP/pr_bulk.log")"
for i in 0 1 2 3 4 5 6 7; do
  [ -f "$WORK_A/pattern $i.csv" ] || fail "pattern $i.csv fehlt"
done
[ -s "$WORK_A/pattern 0.csv" ] || fail "pattern 0.csv ist leer"
assert_contains "$TMP/pr_bulk.log" "Pattern 0 saved to pattern.csv"
assert_contains "$TMP/pr_bulk.log" "loaded from pattern.csv"

log "Phase B: Schism-Export"
WORK_B="$(run_pr schism "$REC_KEYS,S,Down,Right,6,4,Return,Quit")"
wait_for 15 test -s /tmp/schism_pattern.pit || fail "Schism-Datei fehlt: $(tail -5 "$TMP/pr_schism.log")"
assert_contains /tmp/schism_pattern.pit "Pasted Pattern - IT"
xclip_schism="$(xclip -selection clipboard -o 2>/dev/null || true)"
grep -q "Pasted Pattern - IT" <<<"$xclip_schism" || fail "Schism-Pattern nicht im Clipboard"

log "Phase C: Renoise-Import (Z) -> CSV"
cat >"$TMP/renoise_import.xml" <<'XML'
<?xml version="1.0"?>
<PatternClipboard.BlockBuffer doc_version="0">
<Columns><Column><Column><Lines>
<Line index="0"/>
<Line index="4"><NoteColumns><NoteColumn><Note>C-4</Note><Instrument>00</Instrument></NoteColumn></NoteColumns></Line>
<Line index="8"><NoteColumns><NoteColumn><Note>E-4</Note><Instrument>01</Instrument></NoteColumn></NoteColumns></Line>
</Lines><ColumnType>NoteColumn</ColumnType></Column></Column></Columns>
</PatternClipboard.BlockBuffer>
XML
xclip -selection clipboard -i "$TMP/renoise_import.xml"
sleep 0.3
WORK_C="$(run_pr import "Z,1,Return,J,1,S,1,Quit")"
wait_for 15 test -s "$WORK_C/pattern 0.csv" || fail "Import-CSV fehlt: $(tail -5 "$TMP/pr_import.log")"
assert_contains "$TMP/pr_import.log" "Successfully converted"
grep -q ",60," "$WORK_C/pattern 0.csv" || fail "C-4 (MIDI 60) fehlt in CSV: $(cat "$WORK_C/pattern 0.csv")"
grep -q ",64," "$WORK_C/pattern 0.csv" || fail "E-4 (MIDI 64) fehlt in CSV"

log "Phase D: Graph-Fenster (Klicks, F, V, 0 -> Noten)"
WORK_D="$(run_pr graph "$REC_KEYS,G,0,Return,5,Return,click:100:100,click:100:250,click:300:300,click:500:250,F,V,O,0,Quit")"
wait_for 20 grep -q "Graph-Noten eingefügt" "$TMP/pr_graph.log" || fail "Graph-Konvertierung fehlt: $(tail -5 "$TMP/pr_graph.log")"
assert_contains "$TMP/pr_graph.log" "DrawGraph"
assert_contains "$TMP/pr_graph.log" "stepGraph2: 501 Punkte"
assert_contains "$TMP/pr_graph.log" "pitchstepGraph2:"
assert_contains "$TMP/pr_graph.log" "pitchGraph:"

log "OK"
