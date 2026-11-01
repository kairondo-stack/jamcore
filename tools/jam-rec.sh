#!/usr/bin/env bash
# jam-rec — Desktop-Recorder + Video-Player für das JamCore-Paket.
#
#   tools/jam-rec.sh rec [--size 1280x720]   # Bildschirm aufnehmen (Wayland)
#   tools/jam-rec.sh play <datei>            # Video mit variabler Fenstergröße
#   tools/jam-rec.sh stop                    # Aufnahme beenden
#   tools/jam-rec.sh                         # kurze Bedienung
set -uo pipefail

REC_DIR="${JAM_REC_DIR:-$HOME/Videos/JamCore}"
mkdir -p "$REC_DIR"

start_rec() {
  local groesse="${1:-1280x720}"
  local ziel="$REC_DIR/rec-$(date +%Y%m%d-%H%M%S).mp4"
  echo "Aufnahme: $ziel"
  echo "Stop mit: tools/jam-rec.sh stop   (oder Strg+C)"
  # wf-recorder nimmt den Wayland-Output; --audio schreibt das Mikrofon.
  # Fenstergröße: ganzer Output (Aufnahmebereich mit --geometry wählbar).
  exec nix-shell -p wf-recorder --run "wf-recorder --audio -f '$ziel'"
}

play() {
  local datei="$1"
  [ -f "$datei" ] || { echo "Keine Datei: $datei" >&2; exit 1; }
  # mpv: Fenstergröße frei per Ziehen änderbar; Startgröße per --geometry.
  exec mpv --geometry 960x540+60+60 --force-window=yes --keep-open=yes "$datei"
}

case "${1:-}" in
  rec)   start_rec "${2:-}" ;;
  play)  shift; play "$@" ;;
  stop)
    pkill -INT -x wf-recorder && echo "Aufnahme gestoppt (Datei in $REC_DIR)" ;;
  *)
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    echo "Vorhandene Videos: $REC_DIR" ;;
esac
