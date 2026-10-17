# Archiv `/66` (externe Festplatte): brauchbare Funktionen aus den C++-Erstversuchen

Stand: 2026-10-17 · Quelle: `/run/media/tuxedo/WD>3.5>2TB>HDD/66` (11 849 Dateien,
134 C++/Header). Drei Subagenten haben die Quellen gelesen (read-only) und nach
übernehmbaren Funktionen durchsucht. Ziel: was fehlt im heutigen Rust-Stand
(App + Desktop) wirklich?

## Gruppe: kleine Werkzeuge (fertig)

### Top-5

1. **`modconv.cpp`** — Groove-/Tempo-Analysator: BPM-Verhältnis → Näherungsbruch
   n/d, Klassifikation `CLOCK` (2^x) / `FLOW` (d%3==0) / `DRIFT`, sortiert nach
   Fehler (`modconv.cpp:17–32,80–82`). Abhängigkeitsfrei → **Kern (`jamcore`)**.
2. **`midiinscanosc3.cpp`** (+2/1) — robuste **MIDI-Clock → BPM**-Rechnung:
   Intervall-Puffer je Viertelnote, Mittelung, Gate 20–300 BPM
   (`midiinscanosc3.cpp:110–140`). **Desktop** (Clock-Slave) und **App**
   (JamCore schätzt BPM nur aus Audio).
3. **`cvwebsufcam.cpp`** — **Kamera + Farbmaske + Range-Tasten + OSC-Blob**:
   `isColorInRange` (`:35`), Maske in zweites Fenster, alle 100 ms `memcpy` in
   Shared Memory (`:422`), OSC-Blob der ganzen Surface (`send_sdl_surface`,
   `:119`). **App** (Bild-Scan live statt Standbild) / Desktop.
4. **`fktsclbpm.cpp` / `fktsclbpmsave.cpp`** — **Noten-Zeitdehnung/Tempo-Rampe**:
   `scaleNotePoints()` verteilt Noten über ein Intervall (`fktsclbpm.cpp:55–89`),
   sanfter Übergang `xv → xvs` über `scaleDuration` (`fktsclbpmsave.cpp:35–40`).
   → **pattern_recorder** (Timestretch/Tempo-Map).
5. **`numkeyrecord.cpp` + `pattern.cpp`** — Tasten-Timing in **µs** mit
   x-Position (`numkeyrecord.cpp:13–85`, erklärt `KeyEvent.txt` /
   `KeyEventRender.txt`) und **Zeilentakt** `lineTime = (60/bpm)/bpmmal`
   (`pattern.cpp:55`). → **Kern** (Zeilen-/Timing-Modell).

### Nachrücker

- `vectorfar.cpp` — Wert-Streckung (Stützstellen → gleichmäßig verteilte Werte,
  Soll-Ergebnisse in `xValuefar.txt`) → Kern.
- `geompatterndraw.cpp` (65 KB) — Figuren/Playline: Winkel → Zeit → Note,
  Kollision Playline↔Figur als Trigger (`:1247,1299,1360`) → App (Verwandter von
  Stroboskop/Spiral-Scan).
- `loadfile.cpp` — SDL2-TTF-**Dateibrowser** mit Sortierung/Kopieren → Desktop.
- `sequenzerSDL.cpp` — Mehrspur mit **eigener Tracklänge/BPM je Spur** → Desktop.
- `nkrp.cpp` — Vorgänger des `pattern_recorder`: Aufnahme, CSV-Export
  (`timestamp,instrument,pitch`), „Note in N Teile teilen" (`:50–60`).
- `midiosc.cpp` / `netmidi*.cpp` — MIDI↔OSC in beide Richtungen (virtuelle Ports).
- `midioutclockmetro.cpp` — MIDI-Clock **senden** inkl. Start/Stop + Metronom.
- `main16102023.cpp` (2006 Z.) — komplette Renoise-OSC-Kommandoliste + Video-Export
  (libavcodec) als Referenz.
- `TODO.txt` / `Shortcuts_Designrecorder.txt` — Anforderungs- und Feature-Quellen.

### Fallen (nicht übernehmen)

`netmidiconv.cpp` ist ein **ELF-Binary** mit `.cpp`-Endung; `oscbpm.cpp:26`,
`lissajous2.cpp`, `lissa3.cpp` nutzen erfundene APIs; `cvwebsurfcam.cpp:136`
Syntaxfehler; `sdlmicro.cpp:84` / `micro2.cpp:64` lesen über den Puffer;
`rtpmidiin.cpp:21` sendet NULL-Pfad; `zeitfakto.cpp:60` enthält `}4`.

## Gruppe: `rhythmrender*` + `pattern_recorder*` (fertig)

**Kernbefund:** Der Rust-Stand deckt die C++-Logik fast vollständig ab (rrn5:
`is_note_in_chord`, `find_and_draw_harmonics`, `color_to_note_value_and_volume`,
`calculate_x_coordinates`, `read_key_event2`; pattern_recorder: `sinus_interpolate`,
`transform_timestamps`, `float_to_fraction`, KeyEventRender-Schreiber; Kern:
`grid::to_renoise_xml/from_renoise_xml/to_schism`, `harmonie::note_erlaubt`).
`rhythmrendernixosaufgeräumt18072024.cpp` ist byte-identisch mit `rrn4.cpp.save`
(md5 `cdc2507f…`); `pattern_recorder_3_ws_save2.cpp` == `_3_ws.cpp` (md5 `9a4b98da…`).
Kein C++-Stand nutzt FFT (kiss_fft nur Include).

### Echte Lücken (Top-5)

1. **`myFunction` (selfunct 1–8)** — Funktionskurve als Notenquelle: global
   sin / 1/x·sin / tan, je Bildabschnitt sin/tan/Rampe auf/ab und Stufen aus
   gemerkter Kurve, Parameter je Abschnitt mit der Maus editierbar
   (`rhythmrender…13072024.cpp:706`, Aufruf `:1358`, Per-Part-Werte `:1160–1165`)
   → **rrn5** als „Kurven-Modus" (Pitch = Kurvenhöhe statt Pixelfarbe).
2. **`bpmHandler` — OSC-Eingang von Renoise** — `/renoise/song/bpm`,
   `/bpmstart`, `/bpmstop` mit Mutex
   (`rhythmrender…aufgeräumt18072024.cpp:915`; MIDI-Clock-Zweig buggy,
   `…13072024.cpp:968`). **Im Rust empfängt nichts OSC** → Kern
   (`core/src/osc.rs` um Empfänger) + rrn5 (Tempo/Transport folgen dem DAW).
3. **Feld-Preset-Export** — Snapshot aller Parameter (BPM, noteScale,
   anzpi/ampli/pim/selfunct) + Vektordaten + BMP-Vorschau je 200×200-Zelle
   (`…13072024.cpp:412/523`, Aufrufe `:3061/3097`) → Kern als Preset-Format.
4. **`getNextSixChords` + `parallelMinor`** — nächste sechs Akkorde des
   Quintenzirkels inkl. Parallelen (`…13072024.cpp:816`, Füllung `:1330`; im C++
   toter Code) → Kern/App als „Nachbar-Harmonien"-Vorschlag.
5. **MP4-Recorder (ffmpeg)** — RenderReadPixels → sws_scale → H.264
   (`rhythmrender11112023.cpp:3036–3236`, Kern `:3042/3069`; unfertig, Codec-Kontext
   mehrfach benutzt) → **rrn5** als Video-Export (sauber neu, z. B. ffmpeg-CLI).

Nachrücker: `sendMidiControlFromNotes` (`pattern_recorder_202507.cpp:625`,
auskommentiert) — Pitch als MIDI-CC; im Rust gibt es **keinen MIDI-Ausgang**.
Bereits im Rust korrigiert: C++ `removeduplicates` löschte nichts
(leerer `erase`-Bereich, `202507:660`).

Nicht übernehmen: `setpixel8/16/24/32_nolock` (Format-Blitter),
`askForChordTypes`/ChordTypes-Zweig (toter Code), `getNextSixChords`-Aufruf
(fehlt), die Zwischenstände (`*_3.cpp.save*`, `rhythmrendernixos3.cpp.sav`,
`…11112023nix.cpp`).

## Gruppe: `main666*`, `rrn*`, `pr3_*`, `arm*` (fertig)

**Kernbefund:** Auch diese Gruppe ist im Rust-Stand fast vollständig abgedeckt
(Stichproben mit Beleg: Farbe→Note → `core/livescan.rs`; Quintenzirkel/Harmonik →
`core/quint.rs`/`harmonie.rs`; KeyEventRender → `core/scanmask.rs`; X11-Capture →
`rrn5/src/x11cap.rs`; Shared Memory → `shared.rs`/`imgsh`; Renoise-Clipboard-XML →
`rrn5/src/main.rs:2072`; Schism/Import → `pattern_recorder/src/main.rs:2823,2949`;
`floatToFraction`, `modul1`, `transform_timestamps`, `scale_to_range_num`,
`sinus_interpolate`, `bpmtap`, `bpmmultipli`, Notendauer `t` — alle vorhanden).
`rrn5.cpp` **kompiliert nicht** (`array<float> myControlValue[15]` :264,
`i   f (…)` :486); `rrn2508_1` ist die letzte bauende Fassung. „arm" in
`arm09112023.cpp` ist unbelegt (ELF ist x86-64).

### Echte Lücken (Top-5)

| # | Funktion | Beleg | Wohin |
|---|---|---|---|
| 1 | **Kurven-Scan** `myFunction` mit 8 Formen (sin, 1/x·sin, tan, stückweise auf/ab), `anzpi`/`ampli`/`pim`, x-Abschnitte aus Tastendruck-Zeiten | `main666save2_31102023.cpp:473–520,450–471,1685–1846` | **rrn5** (dritter Abtastpfad neben Linie und Spirale) |
| 2 | **Scan als Video** (8 Beats rendern → H.264) | `main666save2_091023_1906.cpp:1451–1650` | **rrn5** (Export-Knopf) |
| 3 | **MIDI-Note → Farbe** (OSC-Empfang 7010, gelernte Zuordnung `ColorNote`) | `…091023_1906.cpp:44–70,1059–1066,1515–1540` | **rrn5/App** (braucht erst einen OSC-Empfänger) |
| 4 | **Feld-Slots mit Miniaturbildern** (Parameter + BMP je Rasterzelle an der Mausposition) | `main666save2_31102023.cpp:2305–2355,2357–2520` | **rrn5** (Slot-Leiste) |
| 5 | **vBPM-Tempofolge** beim Abspielen (pro Note Zwischen-Tempo an Renoise 7005) | `pr3_20251015.cpp:3187–3229`, Aufruf `:4334` | **pattern_recorder** |

Knapp verpasst: **Sweep-Richtung/Ecke** (4 Zustände, Taste w;
`main666save2_24102023.cpp:1205–1245`) — im Rust läuft der Sweep nur vorwärts
(`core/livescan.rs:136–146`); C++-Umsetzung fehlerhaft.

Bewusst nicht übernehmen: `askForChordTypes`/`getNextSixChords` (nie gefüllt/gerufen),
`bpmHandler`/`generic_handler` (Server ohne Methoden), `midiCallback` (nie
registriert), `notelengtharray` (nur Debug-Ausgabe).

## Gesamt-Shortlist (alle drei Gruppen)

**Kern (`jamcore`)** — klein und abhängigkeitsfrei:
1. `modconv` Groove-Analyse (Bruch + CLOCK/FLOW/DRIFT)
2. **OSC-Empfänger** (fehlt komplett: `/renoise/song/bpm`, `/bpmstart|stop`)
3. MIDI-Clock → BPM (`midiinscanosc3`: Intervall-Puffer, Gate 20–300)
4. Feld-Preset-Format (Parameter + Vektor + Miniaturbild)
5. `getNextSixChords`/`parallelMinor` als „Nachbar-Harmonien"-Vorschlag
6. Zeilen-/Timing-Modell `lineTime = (60/bpm)/bpmmal`, Wert-Streckung (`vectorfar`)

**rrn5 (Desktop)** — Bild-Pfad:
1. **Kurven-Scan** (8 Formen, `anzpi/ampli/pim`) — dritter Abtastpfad
2. **Video-Export** (Scan → H.264, ffmpeg-CLI statt Bibliothek)
3. **Feld-Slots** mit Miniaturbildern
4. **MIDI-Note → Farbe** (OSC-Empfang + gelernte Zuordnung)
5. Sweep-Richtung/Ecke (4 Zustände)

**pattern_recorder (Desktop)**:
1. **vBPM-Tempofolge** beim Abspielen
2. Noten-Zeitdehnung/Tempo-Rampe (`fktsclbpm`/`fktsclbpmsave`)
3. MIDI-Ausgang (Pitch → CC; gibt es im Rust gar nicht)

**App (JamCore)**:
1. Live-**Kamera + Farbmaske + OSC-Blob** (Konzept aus `cvwebsufcam`)
2. **MIDI-Clock → BPM** (heute nur Audio-Schätzung)
3. Sweep-Richtung/Ecke und Wert-Streckung, falls gewünscht
