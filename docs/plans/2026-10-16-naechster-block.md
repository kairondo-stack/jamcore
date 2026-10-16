# Plan 2026-10-16 (nächster Block): Bereiche, Kreis-Akkord, Ordner-Zugriff

Stand: 2026-10-16 · Nutzerwünsche nach dem Slicer-Block. **Reihenfolge
festgelegt: A → B → C → D → danach der Max-Patch-Block** (Euklid-Muster,
Akkordname, Spiral-Scan in App **und** Desktop; die übrigen Punkte der
Patch-Übersicht danach).

## Step A: Instr/Samples/Slices – x/y-Bereiche und Mehrfach-Auswahl

- Goal: Jeder Eintrag im Instrument-/Sample-Tab (und später Slices) bekommt
  **Bereichsfader für die x- und y-Achse** des Bildes: nur innerhalb beider
  Bereiche (und im Notenbereich) klingt er, sonst still. Ein **Auswählhaken**
  erlaubt, den Bereich für **mehrere** Einträge gleichzeitig zu setzen
  (Mehrfachauswahl: markierte Einträge bekommen denselben x/y/Noten-Bereich).
- Files: `core/src/instrument.rs` (Bereiche x/y + Trefferlogik + Tests),
  `android/src/instr.rs` (Fader, Haken, Mehrfachauswahl),
  `android/src/image.rs` (x/y der Treffer an die Zuordnung reichen).
- Test: Kern: `x_bereich_klemmt`, `y_bereich_klemmt`, `ausserhalb_still`,
  `mehrfach_bereich_setzen`, `notenbereich_wie_bisher`.
- Verify: Kern-Tests grün; Handy: zwei Einträge markieren, Bereich setzen,
  Scan außerhalb → still, innerhalb → beide klingen.

## Step B: Stroboskop in B2 prüfen/zeigen

- Goal: In B2 sind Stroboskop-Schalter, Richtung und Raten erreichbar; die
  Ansicht ist dieselbe wie im Bild-Tab (gemeinsamer Zustand).
- Files: `android/src/image.rs` (B2-Leiste; ggf. Reihenfolge/Sichtbarkeit).
- Verify: Handy: B2 öffnen → Zeile „Stroboskop" in der linken Leiste; an →
  Bild läuft wie im Bild-Tab.

## Step C: Akkord im Kreis festlegen + x-Bereichsfader teilen

- Goal: Im Kreis-Tab einen **Akkord festlegen** (mehrere Segmente als Akkord
  speichern, Name anzeigen) und **x-Bereichsfader** für die Bildspalten, die in
  Kreis, Bild und B2 **denselben** Wert zeigen (ein Zustand).
- Files: `core/src/quint.rs` (Akkord-Modell + Tests), `android/src/quint.rs`
  (UI: Akkord setzen/löschen), `android/src/image.rs` (x-Bereich liest/schreibt
  denselben Zustand), `android/src/lib.rs` (gemeinsamer Zustand).
- Test: Kern: `akkord_aus_segmenten`, `x_bereich_teilen`.
- Verify: Handy: Akkord im Kreis setzen → Bild/B2 zeigen denselben x-Bereich;
  Scan nutzt ihn.

## Step D: Handy-Zugriff auf Bild-/Audio-/Pattern-Ordner

- Goal: Von der App aus auf die Ordner für **Bild, Audio, Patterns** zugreifen
  (lesen/schreiben) — heute sind nur der App-Ordner und (ohne Berechtigung
  gesperrt) Download/Documents erreichbar.
- Optionen (zu entscheiden): (1) **App-eigene Ordner** unter
  `/sdcard/Android/data/org.jamcore.app/files/{bild,audio,module,videos}` —
  ohne Berechtigung, per USB vom Rechner erreichbar; (2) zusätzlich
  `MANAGE_EXTERNAL_STORAGE` („Alle Dateien") im Manifest + Einstellungs-Intent,
  damit auch `/sdcard/DCIM`, `Music`, `Download` gehen (Berechtigung muss der
  Nutzer in den Systemeinstellungen erteilen).
- Files: `android/src/paths.rs` (Ordnerliste), `android/src/browser.rs`
  (Startordner), `android/app/src/main/AndroidManifest.xml` (nur bei (2)),
  `docs/bedienungsanleitung.md`.
- Verify: Handy: Datei per USB in den Ordner legen → im Browser sichtbar;
  Slicer/Player finden sie.

## Offen aus früheren Blöcken

- Slicer-App-Teil (läuft als Subagent): WAV/Audio wählen, aufnehmen, slicen,
  .it sichern, Slices im Raster.
- Desktop: Sync/Hashes (läuft), `nix-build`, `Start.txt` (erledigt).
- Handy: Stroboskop-Zeitmessung (Scan-Intervalle aus dem Log, mit/ohne Stroboskop).

## Danach: Max-Patch-Block (aus der Übersicht gewählt)

1. **Euklidisches Muster** (App): 0/1-Folge → Schritte im Rhythmus-Raster,
   Verteilung auf Kanäle/OSC (Quelle `eucl-midi-in out`).
2. **Akkordname** (App): 31 Namen aus den klingenden Noten/Notenbereichen
   (Quelle `showchord` + `mollchords`) — passt zum Kreis-Akkord aus Step C.
3. **Spiral-Scan** (App **und** Desktop): r = a·φ bzw. r = ln(a·φ),
   x = r·cos φ, y = r·sin φ, Radius → Tonhöhe (Quelle `spiralobpm` /
   `Matrizenleser_Spiral_archimedisch`).
4. Danach optional aus der Patch-Übersicht: „Note → Bild" (8 Slots),
   bereichsgesteuerte Notenwert-Clocks (`2nd_3rd`), Kurve → Schritte
   (`fktscalerender`/`funktionaudio`), BPM aus Notenabständen
   (`bpmoptisch-midinotein`), 3-Spur-Grid-Recorder (`pattern.maxpat`).
