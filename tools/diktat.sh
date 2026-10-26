#!/usr/bin/env bash
# Spracheingabe für die Zwischenablage (Diktat):
#   nix-shell -p pulseaudio whisper-cpp --run './tools/diktat.sh 8'
# Nimmt `Sekunden` (Default 8) vom Standard-Mikrofon auf, transkribiert mit
# whisper.cpp (Deutsch) und legt den Text in die Zwischenablage (wl-copy).
# Danach im Chat mit Strg+V einfügen.
set -euo pipefail

SEK="${1:-8}"
MODELL="${WHISPER_MODELL:-$HOME/.cache/whisper/ggml-small.bin}"
OUT="${AUS:-/tmp/opencode}"
mkdir -p "$OUT"
WAV="$OUT/diktat.wav"
TXT="$OUT/diktat"

echo "🎙  Aufnahme ${SEK}s … (sprechen)"
# Roh-PCM mit fester Dauer aufnehmen: `timeout` beendet parec zuverlässig;
# der WAV-Kopf wird danach mit ffmpeg geschrieben (ein SIGTERM mitten im
# Schreiben würde sonst einen kaputten Header hinterlassen).
QUELLE="$(pactl get-default-source 2>/dev/null || true)"
ARGS=()
[ -n "$QUELLE" ] && ARGS=(-d "$QUELLE")
RAW="$OUT/diktat.raw"
timeout "$SEK" parec "${ARGS[@]}" --format=s16le --rate=16000 --channels=1 --raw > "$RAW" 2>/dev/null || true
FF="${FFMPEG:-/nix/store/wfdhrw2jxj59gllsf1kh70g7a1n6zyvs-ffmpeg-8.1.2-bin/bin/ffmpeg}"
"$FF" -y -loglevel error -f s16le -ar 16000 -ac 1 -i "$RAW" "$WAV"
rm -f "$RAW"

echo "✍  Transkribiere …"
whisper-cli -m "$MODELL" -l de -otxt -of "$TXT" -np "$WAV" 2>/dev/null

TEXT="$(cat "$TXT.txt" 2>/dev/null | sed 's/^ *//')"
if [ -z "${TEXT// }" ]; then
  echo "Keine Sprache erkannt."
  exit 1
fi
printf '%s\n' "$TEXT"
printf '%s' "$TEXT" | wl-copy
echo "📋 in der Zwischenablage (${#TEXT} Zeichen)"
