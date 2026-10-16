#!/usr/bin/env bash
# Aktualisiert hashes.nix: baut jedes Crate, faengt den cargoHash-Fehler ab
# und schreibt den echten Hash in hashes.nix.
# Aufruf: ./update-hashes.sh [crate ...]
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"

CRATES=("$@")
if [ ${#CRATES[@]} -eq 0 ]; then
  CRATES=(it_live pattern_recorder rrn5 imgsh bundle_udp lissa)
fi

for c in "${CRATES[@]}"; do
  echo "=== $c"
  out="$(nix-build "$DIR" -A "$c" --no-out-link 2>&1 || true)"
  hash="$(printf '%s\n' "$out" | grep -oE 'got: +sha256-[A-Za-z0-9+/=]+' | head -1 | sed -E 's/got: +//')"
  if [ -z "$hash" ]; then
    echo "kein Hash-Fehler, letzte Zeilen:"
    printf '%s\n' "$out" | tail -5
    continue
  fi
  echo "hash: $hash"
  sed -i -E "s|($c = \")[^\"]*|\\1$hash|" "$DIR/hashes.nix"
done

echo "--- hashes.nix ---"
cat "$DIR/hashes.nix"
