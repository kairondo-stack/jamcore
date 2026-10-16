#!/usr/bin/env bash
# Setzt die cargoHashes für alle Crates neu — parallel, alle Kerne.
# Vorgehen je Crate: Hash leeren, nix-build laufen lassen, "got: sha256-…"
# aus der Fehlermeldung ziehen und in hashes.nix schreiben.
set -uo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DIR"

CRATES=("$@")
if [ ${#CRATES[@]} -eq 0 ]; then
  CRATES=(it_live pattern_recorder rrn5 imgsh bundle_udp lissa)
fi

echo "Kerntzahl: $(nproc) · nix-build mit --cores 0 --max-jobs auto"
for c in "${CRATES[@]}"; do
  sed -i -E "s|($c = \")[^\"]*|\\1|" hashes.nix
done

hole() {
  local c="$1"
  local log="/tmp/opencode/hash-$c.log"
  nix-build "$DIR" -A "$c" --no-out-link --cores 0 --max-jobs auto >"$log" 2>&1
  local hash
  hash="$(grep -oE 'got: +sha256-[A-Za-z0-9+/=]+' "$log" | head -1 | sed -E 's/got: +//')"
  if [ -z "$hash" ]; then
    echo "$c: KEIN Hash gefunden (siehe $log)"
    tail -3 "$log"
    return 1
  fi
  echo "$c: $hash"
  (
    flock 9
    sed -i -E "s|($c = \")[^\"]*|\\1$hash|" "$DIR/hashes.nix"
  ) 9>"$DIR/.hash.lock"
}

pids=()
for c in "${CRATES[@]}"; do
  hole "$c" &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p" || true; done

rm -f "$DIR/.hash.lock"
echo "--- hashes.nix ---"
cat "$DIR/hashes.nix"
