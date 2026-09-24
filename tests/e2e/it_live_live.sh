#!/usr/bin/env bash
# E2E: it_live Live-Wiedergabe über cpal (ALSA-Null-Device) inkl. Beat-Triggern
# und sauberem Fehlerpfad bei unbekanntem Gerät.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

MODULE="${1:-$HOME/Documents/drumnbaesh.it}"
[ -f "$MODULE" ] || fail "Testmodul fehlt: $MODULE"

log "Baue it_live"
(cd "$ROOT" && cargo build -q)

log "ALSA-Null-Device für Tests definieren"
ALSA_PREFIX="$(pkg-config --variable=prefix alsa)"
ALSA_CONF="$TMP/alsa_null.conf"
mkdir -p "$TMP"
cat >"$ALSA_CONF" <<EOF
<$ALSA_PREFIX/share/alsa/alsa.conf>
pcm.e2e_null {
  type null
  hint {
    show on
    description "E2E Null Device"
  }
}
EOF

log "Live-Wiedergabe 2 s auf e2e_null"
ALSA_CONFIG_PATH="$ALSA_CONF" "$ROOT/target/debug/it_live" "$MODULE" \
  --device e2e_null --seconds 2 >"$TMP/it_live_live.log" 2>&1 ||
  fail "Live-Wiedergabe Exit != 0: $(tail -3 "$TMP/it_live_live.log")"

assert_contains "$TMP/it_live_live.log" "Live-Wiedergabe auf: e2e_null"
assert_contains "$TMP/it_live_live.log" "BEAT-Trigger \(live\)"
assert_contains "$TMP/it_live_live.log" "Live-Wiedergabe beendet."

log "Unbekanntes Gerät -> klarer Fehler"
set +e
ALSA_CONFIG_PATH="$ALSA_CONF" "$ROOT/target/debug/it_live" "$MODULE" \
  --device gibtsnicht --seconds 1 >"$TMP/it_live_bad.log" 2>&1
rc=$?
set -e
[ "$rc" -ne 0 ] || fail "Unbekanntes Gerät muss fehlschlagen"
assert_contains "$TMP/it_live_bad.log" "nicht gefunden"

log "OK"
