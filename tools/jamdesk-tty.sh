#!/usr/bin/env bash
# jamdesk-tty — startet ein Desktop-Paket-Programm auf der Konsole (KMSDRM,
# ohne Wayland/X), so wie Schism Tracker läuft.
#
#   tools/jamdesk-tty.sh rrn5 [--size 1280x720]
#   tools/jamdesk-tty.sh pattern_recorder
#   tools/jamdesk-tty.sh imgsh bild.png
#   tools/jamdesk-tty.sh lissa
#   tools/jamdesk-tty.sh schism [datei.it]
#   tools/jamdesk-tty.sh jamcast datei.wav --genre techno
#
# Vorher auf eine freie VT wechseln (Strg+Alt+F3). Zugriff auf /dev/dri
# (Gruppe video) ist nötig; sonst bricht KMSDRM ab.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/result/bin"

if [ $# -lt 1 ]; then
  sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
  exit 1
fi

PROG="$1"; shift

if ! ls /dev/dri/card* >/dev/null 2>&1; then
  echo "Kein /dev/dri (KMSDRM nicht möglich) — läuft Mesa?" >&2
  exit 1
fi

# Konsole: KMSDRM + ALSA. Bildschirmgröße über SDL-Vorgaben, damit die
# Werkzeuge auf 2880x1800 nicht winzig sind.
export SDL_VIDEODRIVER=kmsdrm
export SDL_AUDIODRIVER=alsa
export SDL_VIDEO_KMSDRM_DEVICE="${SDL_VIDEO_KMSDRM_DEVICE:-/dev/dri/card0}"

case "$PROG" in
  rrn5)             exec "$BIN/rrn5" --size "${1:-1280x720}" ;;
  pattern_recorder) exec "$BIN/pattern_recorder" ;;
  imgsh)            exec "$BIN/imgsh" "$@" ;;
  lissa)            exec "$BIN/lissa" ;;
  jamcast)          exec "$BIN/jamcast" "$@" ;;
  schism|schismtracker)
                    exec "$(command -v schismtracker)" "$@" ;;
  *)
    echo "Unbekannt: $PROG (rrn5, pattern_recorder, imgsh, lissa, jamcast, schism)" >&2
    exit 2 ;;
esac
