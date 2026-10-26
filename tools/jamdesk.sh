#!/usr/bin/env bash
# jamdesk — startet das JamCore-Desktop-Paket übersichtlich auf einem
# Hyprland-Workspace: links Renoise, rechts drei Shell-Fenster
# (pattern_recorder, rrn5, OSC-Relay/Logs).
#
#   tools/jamdesk.sh                 # starten und anordnen
#   tools/jamdesk.sh --ohne-renoise  # nur die JamCore-Tools
#   tools/jamdesk.sh --stop          # alles wieder beenden
#
# Voraussetzung: `nix-build -A jam-tools` (result/bin) und Renoise unter
# ~/Documents/rns_352_linux_x86_64/renoise.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/result/bin"
RENOISE_DIR="$HOME/Documents/rns_352_linux_x86_64"

MIT_RENOISE=1
for a in "$@"; do
  case "$a" in
    --stop) MIT_RENOISE=2 ;;
    --ohne-renoise) MIT_RENOISE=0 ;;
  esac
done

# Adresse eines Fensters anhand eines Titel-Teils (JSON kompakt, ein Objekt je Zeile).
adresse_fuer() {
  local teil="$1"
  hyprctl clients -j 2>/dev/null | tr -d '\n' | sed 's/},{/}\n{/g' \
    | grep -F "\"title\": \"$teil" \
    | sed -n 's/.*"address": *"\(0x[0-9a-f]*\)".*/\1/p' | head -1
}

schliesse() { # Titel-Teil
  local teil="$1" a
  a=$(adresse_fuer "$teil")
  [ -n "$a" ] && hyprctl dispatch closewindow "address:$a" >/dev/null 2>&1
}

if [ "$MIT_RENOISE" = 2 ]; then
  echo "Beende JamCore-Desktop …"
  pkill -x pattern_recorder 2>/dev/null
  pkill -x rrn5 2>/dev/null
  pkill -x bundle_udp 2>/dev/null
  pkill -x renoise 2>/dev/null
  for t in "JamCore:" "Renoise"; do schliesse "$t"; done
  exit 0
fi

warte_adresse() {
  local teil="$1" limit="${2:-100}" i a
  for ((i = 0; i < limit; i++)); do
    a=$(adresse_fuer "$teil")
    [ -n "$a" ] && { echo "$a"; return 0; }
    sleep 0.2
  done
  return 1
}

setze() { # titel-teil x y w h
  local teil="$1" x="$2" y="$3" w="$4" h="$5" a
  a=$(warte_adresse "$teil") || { echo "  ! '$teil' nicht gefunden"; return 1; }
  hyprctl dispatch setfloating "address:$a" >/dev/null 2>&1
  hyprctl dispatch movewindowpixel "exact $x $y,address:$a" >/dev/null 2>&1
  hyprctl dispatch resizewindowpixel "exact $w $h,address:$a" >/dev/null 2>&1
  # Auf die eigene Arbeitsfläche legen (stört den aktuellen Arbeitsplatz nicht).
  hyprctl dispatch movetoworkspacesilent "$WS,address:$a" >/dev/null 2>&1
  echo "  ✓ $teil ($x,$y ${w}x${h})"
}

shell() { # titel befehl
  local titel="$1"; shift
  kitty --title "$titel" --class jamdesk \
    -o remember_window_size=no \
    bash -c "$*; echo; echo '--- fertig (Enter schließt) ---'; read -r" \
    >/dev/null 2>&1 &
}

# Eigene Arbeitsfläche: die Tools bekommen einen freien Workspace, damit der
# laufende Arbeitsplatz (Editor/Browser) unberührt bleibt.
WS="${JAMDESK_WS:-9}"
MW=; MH=
# Monitor: der erste (eDP-1, 2880x1800) — dort landet der neue Workspace.
while IFS= read -r zeile; do
  read -r MW MH <<<"$(printf '%s' "$zeile" | sed -n 's/.*"width": *\([0-9]*\), *"height": *\([0-9]*\).*/\1 \2/p')"
  [ -n "$MW" ] && break
done < <(hyprctl monitors -j | tr -d '\n' | sed 's/},{/}\n{/g')
MW=${MW:-2880}; MH=${MH:-1800}
LINKS=$((MW * 58 / 100))
RECHTS=$((MW - LINKS))
H3=$((MH / 3))

echo "JamCore-Desktop auf Workspace ${WS:-?} (${MW}x${MH})"

if [ "$MIT_RENOISE" = 1 ]; then
  echo "  … Renoise (Vollbild-Hintergrund, 27 % Deckkraft)"
  # Deckkraft als Hyprland-Regel (Lua-Config): 27 % aktiv/inaktiv, schwebend,
  # damit die Werkzeug-Fenster darüber liegen können.
  hyprctl eval 'hl.window_rule({name="jamdesk-renoise", match={class="Renoise"}, opacity="0.27 0.27", float=true})' >/dev/null 2>&1
  (cd "$RENOISE_DIR" && NIXPKGS_ALLOW_UNFREE=1 setsid nix-shell -p steam-run \
     --run "steam-run ./renoise" >/tmp/opencode/jamdesk-renoise.log 2>&1 &)
fi
shell "JamCore: pattern_recorder" "cd '$ROOT' && '$BIN/pattern_recorder'" 
shell "JamCore: rrn5" "cd '$ROOT' && '$BIN/rrn5' --size 800x600"
shell "JamCore: OSC & Logs" "cd '$ROOT' && echo 'bundle_udp -> 127.0.0.1:7003' && '$BIN/bundle_udp' 127.0.0.1"

sleep 1
[ "$MIT_RENOISE" = 1 ] && setze "Renoise" 0 0 "$MW" "$MH"
setze "JamCore: pattern_recorder" "$LINKS" 0 "$RECHTS" "$H3"
setze "JamCore: rrn5" "$LINKS" "$H3" "$RECHTS" "$H3"
setze "JamCore: OSC & Logs" "$LINKS" $((H3 * 2)) "$RECHTS" "$((MH - H3 * 2))"

# Zur Arbeitsfläche wechseln, damit das Paket direkt sichtbar ist.
hyprctl dispatch workspace "$WS" >/dev/null 2>&1

echo "Fertig. Beenden: tools/jamdesk.sh --stop"
