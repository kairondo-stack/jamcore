#!/usr/bin/env bash
# E2E: imgsh und rrn5 schreiben MyImageShm; Header/Pixel werden geprüft.
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
start_xvfb :96
(cd "$ROOT" && cargo build -q --manifest-path imgsh/Cargo.toml && cargo build -q --manifest-path rrn5/Cargo.toml)

log "imgsh schreibt Bild 2 (Gradient) nach MyImageShm"
imgsh_log="$TMP/imgsh.log"
imgsh_pid="$(E2E_KEYS=Right run_app "$imgsh_log" "$ROOT/imgsh/target/debug/imgsh" "$FIXTURES")"
sleep 1
dump="$(python3 "$DIR/shm_dump.py")"
echo "$dump"
grep -q "width=64 height=64 pitch=256" <<<"$dump" || fail "imgsh-Header falsch"
grep -q "pixel0=0,0,128,255" <<<"$dump" || fail "imgsh-Pixel falsch"
kill "$imgsh_pid" 2>/dev/null || true
wait "$imgsh_pid" 2>/dev/null || true

log "rrn5 liest bestehendes MyImageShm und schreibt per Tab"
rrn5_log="$TMP/rrn5_shm.log"
printf '384 384\n' | E2E_KEYS=Tab "$ROOT/rrn5/target/debug/rrn5" --dir "$FIXTURES" >"$rrn5_log" 2>&1 &
rrn5_pid=$!
APP_PIDS+=("$rrn5_pid")
sleep 2
dump="$(python3 "$DIR/shm_dump.py")"
echo "$dump"
grep -q "width=64 height=64 pitch=256" <<<"$dump" || fail "rrn5-Header falsch"
# currentFrame startet bei 1, ein Tab springt auf Index 2 (img_03, schwarz)
grep -q "pixel0=0,0,0,255" <<<"$dump" || fail "rrn5-Pixel falsch"

log "OK"
