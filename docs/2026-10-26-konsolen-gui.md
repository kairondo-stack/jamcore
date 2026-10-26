# Konsole/KMSDRM + Schism-Import — Überlegungen (2026-10-26)

## Ziel
Das Desktop-Paket (rrn5, pattern_recorder, imgsh, lissa, jamcast) soll **ohne
Wayland/X** auf einer Konsole laufen — so wie Schism Tracker mit SDL2-KMSDRM —
und seine Ergebnisse (**`.it`**) direkt in Schism Tracker importierbar sein.

## Was heute schon geht
- `pattern_recorder`, `rrn5`, `imgsh`, `lissa` nutzen **SDL2** → SDL2 hat den
  Video-Treiber `kmsdrm` (direkt über `/dev/dri`, kein X/Wayland).
- `jamcast` ist ein CLI und erzeugt `.it`-Dateien (mit Order + Song-Message).
- Schism Tracker liegt unter `~/.nix-profile/bin/schismtracker` und liest
  `.it`-Dateien direkt (`schismtracker datei.it`).

## Start auf der Konsole
`tools/jamdesk-tty.sh <programm>` setzt:
```
SDL_VIDEODRIVER=kmsdrm
SDL_AUDIODRIVER=alsa
```
und startet das Programm. Voraussetzungen:
- eine **freie VT** (Strg+Alt+F3 …), nicht die laufende Wayland-Sitzung;
- Zugriff auf `/dev/dri/card*` (Gruppe `video`) — sonst `kmsdrm` verweigert;
- Schriften/Auflösung: SDL nutzt die native Auflösung (hier 2880×1800);
  die Programme skalieren über ihre Fenstergröße (`--size`, Vorgaben).

## GUI auf der Konsole — was anders ist
1. **Kein Fenstermanager**: kein Vollbild-Umschalter, kein Alt+Tab. Jedes
   Programm belegt den ganzen Bildschirm; beenden mit `Esc`/`Q` (wie Schism).
2. **Maus**: KMSDRM liefert evdev-Eingaben; ein Cursor ist nicht garantiert.
   Deshalb müssen alle Funktionen **per Tastatur** erreichbar sein
   (Tab/Pfeile/Enter, Buchstaben-Shortcuts wie im Tracker).
3. **Schriftgröße**: 2880×1800 ist auf der Konsole sehr klein
   (Hint: `SDL_VIDEO_KMSDRM_DEVICE`/`--size`). Vorschlag: feste 1280×720
   oder 1920×1080 als Standard für Konsole, per `--size` übersteuerbar.
4. **Farbschema**: Schism-Tracker-Look (dunkler Hintergrund, Cyan/Gelb) —
   passt zu den vorhandenen Farben; kein Antialiasing nötig.
5. **egui-Tools** (`eframe`) sind auf KMSDRM fragil (winit/EGL/GBM).
   Für die Konsole die **SDL2-Tools** bevorzugen; egui bleibt Wayland/X.
   `jamcast` (CLI) läuft ohnehin.

## Import in Schism Tracker
- `jamcast` schreibt `.it` mit **Order-Liste** und **Song-Message**
  (Quelle/Datum/Dauer). Schism zeigt die Message im Song-Info-Dialog.
- Weg: `jamcast` (oder der Rec-Export der App) → Datei im Modulordner →
  `schismtracker datei.it` → Pattern/Order direkt sichtbar.
- Idee für später: ein **„Nach Schism senden“**-Knopf im Desktop-Paket, der
  die Datei in `~/Musik/jamcore/` ablegt und Schism per
  `schismtracker <datei>` startet (gleiche Datei, kein Kopieren).

## Nächste Schritte (offen)
1. `tools/jamdesk-tty.sh` auf einer VT testen (braucht Nutzer, kein Fernstart).
2. Prüfen, ob `SDL_VIDEODRIVER=kmsdrm` mit den Nix-Builds die GPU nutzt
   (`/dev/dri/renderD128`, Mesa) oder auf llvmpipe fällt (dann 1280×720).
3. Schism-Import mit einer `jamcast`-Datei am Gerät prüfen (Order/Message).
4. Keyboard-Only-Audit der SDL-Tools (Maus-unabhängig bedienbar?).
