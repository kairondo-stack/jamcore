# Funktionen des Rust-Pakets VOR der Android-Portierung

Stand: Commit `be627ca` (2026-09-29, letzter Commit vor `S1: jamcore-Crate extrahiert`).
Die Desktop-Werkzeuge sind seitdem funktional unverändert — seit dem Portierungsbeginn
hat sich an `rrn5/`, `pattern_recorder/`, `bundle_udp/`, `imgsh/` nur `Cargo.lock`
geändert. Aus `src/` wanderte `openmpt.rs` nach `core/` (S3), die Funktionalität blieb.

Das Paket bestand aus **fünf Programmen**:

| Programm | Zweck in einem Satz | Zeilen |
|---|---|---|
| `it_live` (`src/`) | IT-Module abspielen, ansehen, als WAV rendern | 382 |
| `rrn5` | Live-Scan von Bildern/Kamera → Renoise-Noten | 3387 + Module |
| `pattern_recorder` | Notenpattern per Tastendruck aufnehmen und austauschen | 5126 + Module |
| `bundle_udp` | OSC-Relay zwischen Geräten | 171 |
| `imgsh` | Bildbetrachter, der Bilder in den Shared Memory legt | 333 |

---

## it_live (Paket im Wurzelverzeichnis, `src/main.rs`)

Lädt IT-Module über die libopenmpt-Anbindung (`jamcore::openmpt`, damals `src/openmpt.rs`)
und gibt Info sowie Sample-Namen aus. Optional Struktur-Dump, WAV-Render oder
Live-Wiedergabe über cpal.

- `it_live <datei.it>` — Info + Sample-Liste des Moduls
- `--dump` — Patterns, Orders, Channels, Rows je Pattern, Order-Liste inkl. `<BREAK>` (0xFFFE)
- `--wav out.wav` — 16-Bit-Stereo-WAV, 44100 Hz; Länge über `RLENGTH` (Vorgabe 6:40);
  alle vier Beats wird die Order auf Pattern `creative % 8` bei gleicher Row umgeschaltet
- Live-Wiedergabe über cpal: Default-Gerät oder `--device NAME` (bei Fehler wird die
  Geräteliste ausgegeben); Puffer in 512er-Blöcken, Stereo auf die Kanäle verteilt
- Beat-Trigger im Audio-Callback (`BeatTracker`): beim 4. Beat einer Phrase
  `openmpt_module_set_position_order_row`, Log „BEAT-Trigger (live)“
- `--seconds N` stoppt nach N+0,3 s, sonst beendet Enter die Wiedergabe
- Unit-Tests für `BeatTracker` (Phrasengrenzen, wiederholte Rows, anderes rpb)

**Dateien:** `src/main.rs` (CLI, BeatTracker, cpal, WAV), `src/openmpt.rs` (FFI-Wrapper, später nach core).
**Nutzt:** `jamcore::openmpt` (`Mod`, `openmpt_module_*`).

---

## rrn5

Rust-Port von `rrn5.cpp`: **Live-Scan-Werkzeug**. Es scannt Bilder (Dateien, Kamera/Video
über OpenCV oder X11-Fensteraufnahme) auf einem Raster in Farb-Pixel und sendet die
Treffer als Noten/Lautstärken an Renoise (OSC). Drei SDL-Fenster: Hauptbild, zweites und
drittes Fenster als CPU-Pixelpuffer („Selected color“). Eingaben laufen über stdin/SDL.

**Bildquellen**
- Bildverzeichnis (`--dir`, `FRAMES_DIR`, sonst cwd), Einzelbild `frame.bmp`
- Kamera (`z` an/aus), Videodatei (`--video <pfad>`, Loop; FPS über `RRN5_VIDEO_FPS`),
  X11-Fensteraufnahme (`i` = angeklicktes Fenster nach `output.ppm`/`frame0.bmp`)

**Geometrie**
- Größe `--size WxH` bzw. `RRN5_SIZE`, sonst stdin-Abfrage
- Split-Grenzen per Alt+D (links) / Alt+C (rechts); Cursor an Bildschirmgröße angepasst
  (`RRN5_SCREEN_WIDTH/HEIGHT`)
- Maus: links = Farbe im zweiten Puffer selektieren/filtern (Farbbereich + Harmonics),
  rechts = Fenster mit Hintergrundfarbe füllen, Mitte = Rasterscan mit iadd/jadd,
  `cc` = Farbbereichs-Modus, Zeichen-Ausdünnung

**Takte-Maske (`b` = biialpha)**
- liest `KeyEventRender.txt` (eine Zeile je x-Spalte `x;key;iialpha;xkeyk`)
- `space` liest die Datei neu und übernimmt zugleich BPM/Dauer aus dem Shared Memory
- `mask_alpha`: Spalten mit alpha 0 werden übersprungen, sonst Farbe und Volume skaliert

**Live-Scan**
- `6` an/aus, `1` freilaufend, `2` taktsynchron (ein Sweep = ein Beat), `3` Palette
  an/aus (gemalte Farben aus dem zweiten Fenster)
- Dichte über `RRN5_MAX_NOTES_PER_SEC` begrenzbar
- gehaltene linke Maustaste friert Bild und Scan ein

**Harmonie/Noten**
- Notenskala (`f3`/`f4` ±10, F1/F2 NoteScaleadd, `j`/`m`, `y`, `>`/Shift+`>` transponieren)
- noteadjust-Modi 0–8 per Pfeiltasten mit Live-Vorschau (Note/Volume an Renoise)
- `4` Harmonics an/aus, `3` Harmonien-Editor, `5` Frequenzspanne/Teiler oder
  interaktiver **Quintenzirkel** (eigenes SDL-Fenster), `7` useNotesSet

**Renoise (OSC)**
- BPM senden (7000; 7001 mit Teiler) über `r`, `space`, F9/F10 ±0,0625
- Transport Start/Stop (F5), Pattern-Follow/-Loop (F7/F6)
- `note_on` (7000/7001), MIDI-Trigger (7002); Portbasis über `RRN5_OSC_BASE_PORT`
- `g` = Patternbereich abfragen und als Renoise-PatternClipboard-XML
  (`/tmp/renoise_pattern.xml`) in Clipboard/Primary kopieren (xclip)

**Wiedergabe/Export**
- `o`/`k` rendern das zweite Fenster taktsynchron als KeyEvents und triggern Renoise
- `l` = Snapshot `frame4save.bmp` + Aufbau von `keyEventsxyrgbt`, `p` = bpmk

**MyImageShm**
- veröffentlicht Kamera-/Videoframes (`8` Modus `every`/`throttle`/`off`, `--shm`)
- `x` zeigt das Shm-Bild an (Enter schließt)

**E2E-Hook** `E2E_KEYS` (Tasten/Maus/Quit per SDL-Queue), viele Unit-Tests.

**Dateien:** `main.rs` (Zustand, Event-Loop, Scan, Harmonie, XML, Tests), `osc.rs`
(Sender: `send_bpm`, Transport, `note_on`, `trigger_midi`), `shared.rs` (Boost-kompatibler
Shared Memory: `ZeitfaktoSharedMemory`, `MyImageShm`), `video.rs` (OpenCV, BGR→RGBA,
SHM-Modi), `x11cap.rs` (Fensteraufnahme), `livescan.rs` (Spaltenscan, später nach core),
`quint.rs` (Zirkel-Geometrie, später nach core), `examples/make_testvideo.rs`.
**Nutzt (heute):** `jamcore::livescan`, `jamcore::quint`.

---

## pattern_recorder

**Recorder für Notenpattern.** Tastenanschläge werden mit Zeitstempeln aufgezeichnet, zu
Notenlisten skaliert und transformiert. Wiedergabe über internen Sinus-Oszillator oder
Renoise (OSC Port 7005); Austausch über Renoise-/Schism-Clipboard, CSV und Shared Memory.
Acht Pattern-Slots (`pnotes`), kleines SDL-Fenster, Schrift `sagan.ttf`.

**Aufnahme**
- `R` Aufnahmebereitschaft, `1` Start, `0` Stop
- im Aufnahmemodus: `A` alleseins, Ziffern 1–9 als Notenwerte, F1–F10 als 10–19,
  `Q`/`E` Faktor 2/4, `X`/`C`/`V`/`B`/`N`/`M` RStep-Werte, `S`/`D`/`W` Auswahl,
  PageUp/PageDown, `Y` Schrittweite 3; danach automatische Skalierung
- `2` Notenrampe mit abgefragter Länge, `E` = `remove_and_record` (Bereich löschen und
  neu einspielen), `V` = `fdx`, `M` = `modul1`

**Skalieren/Transformieren**
- `K`+`1`/`2` Indexverschiebung, `K`+`5` pnotes übernehmen, `K`+`6` Duplikate entfernen,
  `K`+`7` Gesamtdauer, `K`+`8` `totalDi`, `K`+Pfeile `scale_to_range_num`
- `D`+`1` Notenlisten zeigen, `D`+`3` Sinus-Pitch-Interpolation, `D`+`4` Rücknahme
  (Snapshot `notes4s`), `D`+`5` `vmidi`, `D`+`6` Mehrpattern-Interpolation (bis 8)
- `B`-Kette: PageUp/PageDown Timestamp-Transformation/-Rücknahme, Pfeile Notenlänge/Glide
- `F3` umkehren, `F4` rechnen (`*`, `/`, `+`, `-` mit Faktor), `C`/`X` Clear

**Graph-Editor**
- `G` öffnet ein Graph-Fenster; Klicks bilden eine stückweise lineare Kurve
  (`interpolate_step_graph`) und werden in Noten umgerechnet (y/10 Halbtöne ab MIDI 60)

**Austausch**
- `Z` liest Renoise-PatternClipboard-XML (self-closing Lines = leer) und konvertiert
- `S`+`Links`/`Rechts` erzeugt Renoise-XML bzw. Schism-Clipboard („Pasted Pattern - IT“)
  und kopiert per xclip (Fallback xsel)
- CSV `timestamp,instrument,pitch,glide`: `S`+`1..8` speichern (9 = alle), `F`+`1..8`/`9` laden
- **`S`+`0` schreibt `KeyEventRender.txt`** (fragt „Width?“ und „Start?“) als Steuermaske
  für rrn5, Format `x;key;alpha;xkeyk`

**Slots/Wiedergabe**
- `J`+`1..8` kopiert `notes4` in pnotes, `H`+`1..8` spielt einen Slot (auf die nächste
  16tel quantisiert), `H`+`0` alle, `H`+`M` umschalten `play4`/`play34`
- `Y` Pattern-Select-Modus (1–8 mit Klang, 9 beendet), `O` = `play2`
- `F5` Renoise-Transport, `F8` Wiedergabe über Renoise, `F9` Renoise-Record, `U` Loops

**Tempo/Zeit**
- `T`+`↑` BPM setzen, `T`+`↓` BPM-Tap, `T`+`L` Mikrofon-BPM live, `T`+`M` 8-s-Messung,
  `T`+`Return` übernimmt (intern + OSC + Shared Memory)
- `Q`+`3`/`4` quantisiert `notes3`/`notes4`
- `L`+`P` Dauer in Beats, `L`+Pfeile ÷2/×2/÷3/×3, `P` Pattern-Dauer je Slot, `W` = `plsl`

**Shared Memory**
- erzeugt/überschreibt `ZeitfaktoSharedMemory` (`zeitfakto`, `variable2/3`, `dauer`,
  `bpm`, `p1..p3`); `p1`/`p2`/`p3` lösen Playback aus; Update-Delay über `S`+`U`

**Eigener Ton:** SDL-Audio 44100 Hz mono, Sinus (Amplitude 28000) mit Glide;
Mikrofon-Callback puffert Samples.

**Dateien:** `main.rs` (Audio, Shm, Aufnahme, Transformationen, Clipboard/CSV, Graph,
Event-Loop, Tests), `notes.rs` (Notentypen 1:1 aus dem Original + Notennamen + Helfer),
`osc.rs` (Port 7005, Env `RENOISE_OSC_PORT`), `shared.rs` (`ZeitfaktoSharedMemory`,
1024 Byte), `micbpm.rs` (Onset-Erkennung + Intervall-Histogramm, später nach core),
`examples/probe.rs`.
**Nutzt (heute):** `jamcore::micbpm::BpmEstimator`.

---

## bundle_udp

**OSC-Relay.** Lauscht auf mehreren UDP-Ports und leitet jedes Datagramm roh an alle
Ziele weiter; OSC-Nachrichten werden zusätzlich dekodiert und mit Adresse/Argumenten
geloggt. Gedacht für Handy → Laptop/Renoise.

- CLI: erster Parameter = Ziel-IP, sonst Eingabeaufforderung
- Ziele: `OSC_TARGETS` (Vorgabe `<ip>:7003`), mehrere per Komma, Hostnamen und IPs
- Ports: `OSC_LISTEN_PORTS` (Vorgabe `7000,7005`), je Port ein Thread
- Rohweiterleitung an alle Ziele; Logs für Messages, Bundles und Nicht-OSC-Datagramme
- transiente Empfangsfehler (EINTR/WouldBlock/TimedOut) beenden den Relay nicht;
  200-ms-Read-Timeout
- Unit-Tests für Fehlerklassifikation, Port- und Ziel-Parsing

**Dateien:** `src/main.rs` (171 Zeilen). **Nutzt:** nur `rosc`, kein `jamcore`.

---

## imgsh

**Bildbetrachter**, der Bilder in den Boost-kompatiblen `MyImageShm` legt, damit rrn5 sie
als Eingabe nutzen kann.

- Aufruf `imgsh <verzeichnis>`; sammelt `png/jpg/jpeg/bmp` sortiert
- `←`/`→` voriges/nächstes Bild, `Return` neu laden; jedes Bild wird in den Shared Memory
  geschrieben
- `S` Snapshot des Framebuffers nach `snapshot.tga`
- Besitzer des Mappings räumt beim Beenden per `shm_unlink` auf; E2E-Hook `E2E_KEYS`

**Dateien:** `src/main.rs` (333 Zeilen). **Nutzt:** kein `jamcore`.

---

## Was bei der Portierung nach `core` wanderte

Bis `S1` steckten diese Bausteine **in den Werkzeugen**; danach wurden sie hardwarefrei
in `core/` (Crate `jamcore`) extrahiert, die Werkzeuge importieren sie von dort:

| Baustein | Ursprung | Heimat ab S1 |
|---|---|---|
| Spaltenscan, Filter, Palette, Maske (`mask_alpha`, `scale_rgb`, `next_column`, `note_allowed`, `column_interval_ns`, `note_volume`) | `rrn5/src/livescan.rs` | `core/src/livescan.rs` |
| Quintenzirkel-Geometrie (`segment_angles`, `segment_index_at`, `CIRCLE`, `note_classes`) | `rrn5/src/quint.rs` | `core/src/quint.rs` |
| BPM aus Onsets + PCM-Umrechnung (`BpmEstimator`, `detect_onsets`, `bpm_from_onsets`, `pcm16_le_to_f32`) | `pattern_recorder/src/micbpm.rs` | `core/src/micbpm.rs` |
| libopenmpt-FFI (`Mod`, `openmpt_module_*`) | `src/openmpt.rs` | `core/src/openmpt.rs` (S3) |

Damit ist der Funktionsumfang der fünf Programme zugleich die Vorlage für die
Android-Portierung (S1–S12); die Zuordnung „Werkzeug → Tab“ steht im Plan
`docs/plans/2026-09-30-android-port.md`.
