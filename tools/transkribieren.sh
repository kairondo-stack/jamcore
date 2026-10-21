#!/usr/bin/env bash
# Handy-Aufnahmen transkribieren: alle Audio-Dateien im Ordner (oder die
# genannten) nach 16-kHz-WAV wandeln und mit whisper.cpp (Deutsch) in .txt
# schreiben. Aufruf: ./transkribieren.sh <ordner-oder-dateien...>
set -euo pipefail

FF="/nix/store/wfdhrw2jxj59gllsf1kh70g7a1n6zyvs-ffmpeg-8.1.2-bin/bin/ffmpeg"
MODELL="${WHISPER_MODELL:-$HOME/.cache/whisper/ggml-small.bin}"
AUS="${AUS:-/tmp/opencode/transkripte}"
mkdir -p "$AUS"

if [ ! -f "$MODELL" ]; then
  echo "Modell fehlt: $MODELL" >&2
  exit 1
fi

dateien=()
for arg in "$@"; do
  if [ -d "$arg" ]; then
    while IFS= read -r f; do dateien+=("$f"); done < <(
      find "$arg" -maxdepth 2 -type f \
        \( -iname '*.m4a' -o -iname '*.mp3' -o -iname '*.ogg' -o -iname '*.opus' \
           -o -iname '*.wav' -o -iname '*.3gp' -o -iname '*.amr' -o -iname '*.flac' \) \
        | sort
    )
  else
    dateien+=("$arg")
  fi
done

if [ ${#dateien[@]} -eq 0 ]; then
  echo "Keine Audio-Dateien gefunden." >&2
  exit 1
fi

echo "== ${#dateien[@]} Datei(en), Modell: $(basename "$MODELL")"
for f in "${dateien[@]}"; do
  name="$(basename "${f%.*}")"
  wav="$AUS/$name.16k.wav"
  txt="$AUS/$name.txt"
  if [ -f "$txt" ]; then
    echo "-- $name: schon transkribiert ($txt)"
    continue
  fi
  echo "== $name"
  "$FF" -y -loglevel error -i "$f" -ar 16000 -ac 1 -c:a pcm_s16le "$wav"
  whisper-cli -m "$MODELL" -l de -otxt -of "$AUS/$name" -np "$wav" 2>/dev/null
  rm -f "$wav"
  echo "-- fertig: $txt"
done

echo "== Transkripte in $AUS"
ls -1 "$AUS"/*.txt 2>/dev/null
