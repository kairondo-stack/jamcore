#!/usr/bin/env bash
# E2E: bundle_udp leitet OSC von 7000 und Rohdaten von 7005 an 7003 weiter.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$DIR/lib.sh"

log "Baue bundle_udp"
(cd "$ROOT" && cargo build -q --manifest-path bundle_udp/Cargo.toml)

log "Starte OSC-Mitschnitt auf 7003"
python3 "$DIR/osc_listen.py" 7003 "$TMP/relay.jsonl" 8 &
APP_PIDS+=("$!")
sleep 0.5

log "Starte Relay"
run_app "$TMP/bundle.log" "$ROOT/bundle_udp/target/debug/bundle_udp" 127.0.0.1 >/dev/null
sleep 0.5

log "OSC-Nachricht an 7000"
SENT_HEX="$(python3 "$DIR/udp_send.py" 7000 "/test/one" i 42)"
log "Rohdaten (Nicht-OSC) an 7005"
RAW_HEX="$(python3 "$DIR/udp_send.py" 7005 --raw 68656c6c6f207261772064617461)"

wait_for 5 test -s "$TMP/relay.jsonl" || fail "Relay hat nichts weitergeleitet"
assert_contains "$TMP/relay.jsonl" '"/test/one"'
assert_contains "$TMP/relay.jsonl" "\"raw\": \"$SENT_HEX\""
assert_contains "$TMP/relay.jsonl" "\"raw\": \"$RAW_HEX\""
assert_contains "$TMP/bundle.log" "Nicht-OSC-Datagramm"

log "OK"
