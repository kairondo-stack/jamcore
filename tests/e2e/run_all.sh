#!/usr/bin/env bash
# Gesamtlauf: Unit-Tests + alle E2E-Skripte. Ausführung in nix-shell.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"

echo "== Unit-Tests"
(cd "$ROOT" && cargo test -q)
(cd "$ROOT" && cargo test -q --manifest-path pattern_recorder/Cargo.toml)
(cd "$ROOT" && cargo test -q --manifest-path rrn5/Cargo.toml)
(cd "$ROOT" && cargo test -q --manifest-path bundle_udp/Cargo.toml)

echo "== Builds"
(cd "$ROOT" && cargo build -q)
(cd "$ROOT" && cargo build -q --manifest-path bundle_udp/Cargo.toml)
(cd "$ROOT" && cargo build -q --manifest-path imgsh/Cargo.toml)

for script in it_live it_live_live bundle_udp smoke_xvfb shm_pipeline rrn5_e2e pattern_recorder_e2e pattern_recorder_flows; do
  echo "== E2E: $script"
  bash "$DIR/$script.sh"
done

echo "== ALLES OK"
