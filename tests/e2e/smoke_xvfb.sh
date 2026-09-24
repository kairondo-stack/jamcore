#!/usr/bin/env bash
# Smoke: SDL-Apps starten unter Xvfb und laufen stabil.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

rm -f /dev/shm/MyImageShm /dev/shm/ZeitfaktoSharedMemory
make_fixtures
start_xvfb :97

log "SDL-Probe (pattern_recorder/examples/probe)"
(cd "$ROOT/pattern_recorder" && cargo run -q --example probe) || fail "probe fehlgeschlagen"

log "imgsh startet und bleibt am Leben"
imgsh_log="$TMP/imgsh_smoke.log"
pid="$(run_app "$imgsh_log" "$ROOT/imgsh/target/debug/imgsh" "$FIXTURES")"
sleep 2
kill -0 "$pid" 2>/dev/null || fail "imgsh ist früh beendet: $(cat "$imgsh_log")"
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
rm -f /dev/shm/MyImageShm

log "OK"
