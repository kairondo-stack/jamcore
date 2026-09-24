# Rework-Plan: Voll funktionsfähig + E2E-Tests + imgsh-Merge

Datum: 2026-09-23
Branch: `opencode-rework-1-versuch`
Referenz: C++-Originale in `~/Documents/cde/` (rrn5.cpp, pattern_recorder_202601.cpp, imgsh.cpp, bundleudp.cpp)
Testmodul: `~/Documents/drumnbaesh.it`

## Ziel

1. Alle Crates (`it_live`, `rrn5`, `pattern_recorder`, `imgsh`, `bundle_udp`) funktionieren vollständig wie im Workflow vorgesehen:
   Rhythmus per CSV speichern/laden, Farbmischmengen in Bildern zu Rhythmen/Noten (rrn5), OSC-Live an DAW (Renoise Ports 7000/7001/7002/7005), Pattern-Clipboard nach Renoise/Schism.
2. End-to-End getestet: echte Prozesse unter Xvfb, SHM-Pipeline, OSC-Mitschnitt, Datei-Artefakte.
3. imgsh-Funktionalität (Bildverzeichnis → MyImageShm) in rrn5 integrieren.

## Regeln

- Jede Logikänderung bekommt zuerst einen fehlschlagenden Unit-Test (TDD), danach Fix.
- C++-Original ist Verhaltensreferenz. Wo C++ nachweislich UB/Bug ist (Brace-Init, OOB), entscheidet die dokumentierte Absicht — Entscheidungen sind unten mit ⚠ markiert und werden vor Umsetzung abgestimmt.
- Kein Schritt ohne Verify-Kommando.
- GUI-E2E nur unter `Xvfb` + `SDL_AUDIODRIVER=dummy`, damit reproduzierbar.

## Offene Entscheidungen (vor Start klären)

- **E1** imgsh-Crate nach Merge behalten (parallel nutzbar) oder löschen? Vorschlag: behalten, aber rrn5 kann alles allein.
- **E2** Hardcodierten Pfad `/home/apricot/Dokumente/66` durch CLI-Arg `--dir` / Env `FRAMES_DIR` ersetzen, Default: aktuelles Verzeichnis. Vorschlag: ja.
- **E3** C++-Bugs nachbilden oder funktional korrigieren? Vorschlag: korrigieren, Abweichungen dokumentieren (betrifft: `removeduplicates`-No-op, `record()`-Brace-Init, `scale()`-Pausenfilter, Alt+KP `fixedBpm+1`). Alt+KP wird exakt wie C++ übernommen (kein Bug, sondern BPM-Rechenweg).

---

## Phase 0 — Testinfrastruktur

### Schritt 0.1: Dev-Shell erweitern
- Ziel: Build-/Testwerkzeuge für alle Crates und E2E verfügbar.
- Dateien: `shell.nix`
- Test: —
- Verify: `nix-shell --run "cargo --version && which xdotool xclip xsel python3"` → alle gefunden
- Abhängig: —
- Status: libopenmpt/xclip/xsel/wl-clipboard/xdotool bereits ergänzt; `python3` fehlt noch.

### Schritt 0.2: E2E-Harness anlegen
- Ziel: Wiederverwendbare Skripte/Fixtures: `tests/e2e/lib.sh` (Xvfb starten/stoppen, App mit Timeout, auf Ausgabe warten), `osc_listen.py` (UDP-Mitschnitt nach JSONL), `wav_check.py` (RMS > 0), `shm_probe` (Rust-Example in `imgsh/examples/shm_probe.rs`: Header + Prüfsumme ausgeben), Testbilder `tests/e2e/fixtures/*.png`.
- Dateien: `tests/e2e/*`, `imgsh/examples/shm_probe.rs`
- Test: Harness selbst: `tests/e2e/smoke_xvfb.sh` startet Xvfb + `imgsh/examples/probe.rs`-Äquivalent
- Verify: `bash tests/e2e/smoke_xvfb.sh` → Exit 0
- Abhängig: 0.1

---

## Phase 1 — pattern_recorder (höchstes Risiko, zuerst)

### Schritt 1.1: `record()`-Tastendispatch reparieren
- Ziel: S/A/D/W, PageUp/Down, Y/Q/E/X/C/V/B/N/M, F1–F10 wirken wieder (aktuell schluckt `else if !scaletoa` alle Nicht-Ziffern, `scaletoa` kann nie true werden).
- Dateien: `pattern_recorder/src/main.rs`
- Test: Reine Funktion `record_key_action(kc, alleseins, scaletoa) -> RecordAction` extrahieren; Tests: `S` bei `scaletoa=false` → `ToggleKk`, `1` → `SetNote(1)`, `F5` → `SetNote(5)+ScaleToa`, `A` → ToggleAlleseins.
- Verify: `nix-shell --run "cargo test --manifest-path pattern_recorder/Cargo.toml record_key"` → 4+ passed
- Abhängig: 0.2
- Rollback: `git diff` verwerfen (kein Commit vorher)
- Referenz: C++ `record()` cpp:2335-2460 (Ziffernbereich in jeder Bedingung).

### Schritt 1.2: `play_pattern_as_sound` — Panic + Loop-Fortschritt
- Ziel: `Y`+1..8 stürzt nicht mehr ab; Startzeit wird pro Loop korrekt fortgeschrieben (aktuell No-op, alle Noten feuern sofort).
- Dateien: `pattern_recorder/src/main.rs`
- Test: Bounds-Guard vor Indexzugriff; reiner Helper `next_loop_start(...)`; Test: `pattern_len=0` und `note_index==pattern_len` → kein Panic.
- Verify: `cargo test ... play_pattern` → passed; manuell: `Y`,`1` läuft >5 s ohne Absturz.
- Abhängig: 1.1
- Referenz: cpp:226-300, Guardlogik.

### Schritt 1.3: `play_pattern_thread` — Timing
- Ziel: play4/playpm spielt Noten zeitlich korrekt (Startzeit-Advance statt No-op `let _ = st`).
- Dateien: `pattern_recorder/src/main.rs`
- Test: reiner Helper für `start += letzte Notendauer`; Test mit 3 Noten, prüft kumulierte Startzeit.
- Verify: `cargo test ... play_thread` → passed; hörbar per OSC-Mitschnitt: Notenabstände plausibel.
- Abhängig: 1.2

### Schritt 1.4: Renoise-XML-Import (`parse_line_notes`)
- Ziel: `<Lines>` nicht als `<Line` matchen, self-closing `<Line index="N"/>` korrekt behandeln, nur erste innere Spalte lesen, Noten nicht doppelt/verschoben importieren.
- Dateien: `pattern_recorder/src/main.rs` (+ Test-Fixtures inline)
- Test: 3 XML-Fixtures: leer bei Zeile 0, Noten auf Zeile 2+4, mehrere Spalten; erwartete `(index, note, instrument)`-Listen.
- Verify: `cargo test ... parse_line` → passed
- Abhängig: 1.1
- Referenz: cpp:1069-1166.

### Schritt 1.5: CSV speichern/laden
- Ziel: Bulk-Mapping `S+9`/`F+9` = Slot i → `pattern i.csv` (wie C++), Einzel-Mapping unverändert; Bool als `0/1` schreiben (C++-kompatibel), Reader akzeptiert `0/1/true/false`.
- Dateien: `pattern_recorder/src/main.rs`
- Test: Round-Trip (save→load→gleiche Noten) + Fixture `pattern 4.csv` aus cde laden und Feldwerte prüfen.
- Verify: `cargo test ... csv` → passed
- Abhängig: 1.1

### Schritt 1.6: Panic-Härtung
- Ziel: Keine Panics bei leerer `notes4`/`notes2`/`mod1`, ungültigen Indizes (F2/F3/F4/P/K/V/B), negativen Pitches, nicht schreibbarem cwd.
- Dateien: `pattern_recorder/src/main.rs`, `pattern_recorder/src/notes.rs`
- Test: Grenzfall-Tests je Funktion (leer, Index 0/9/negativ, `midi_to_renoise_note(-1)` → `"Off"`), `NaN`-Timestamps sortieren ohne Panic.
- Verify: `cargo test --manifest-path pattern_recorder/Cargo.toml` → alle passed
- Abhängig: 1.1

### Schritt 1.7: Semantik-Angleichungen
- Ziel: `removeAndRecord` F1→10 (nicht 11), `modul1` hängt an statt zu leeren, `removeduplicates` gemäß ⚠E3.
- Dateien: `pattern_recorder/src/main.rs`
- Test: F-Key-Mapping-Test; `modul1`-Append-Test.
- Verify: `cargo test ... fkey && cargo test ... modul` → passed
- Abhängig: 1.6

### Schritt 1.8: Audio-Init nicht fatal + Clipboard-Robustheit
- Ziel: Audio-Gerät fehlt → Warnung und weiter (wie C++), nicht Abbruch; Schism-Patternlänge ≤0 fragt nach (oder Fallback dokumentiert); Renoise-Import „letzte Note pro Zeile gewinnt“.
- Dateien: `pattern_recorder/src/main.rs`
- Test: `AudioGen`-Pfad mit fehlgeschlagenem `open_playback` → `Ok`; Map-Last-wins-Test.
- Verify: `SDL_AUDIODRIVER=doesnotexist cargo run --manifest-path pattern_recorder/Cargo.toml` → startet trotzdem (Exit per Timeout)
- Abhängig: 1.6

---

## Phase 2 — rrn5

### Schritt 2.1: Left/Right-Vorschau + OSC
- Ziel: Beim Noteadjust-Zyklus Mittelpixel/Formel auswerten, Ecken-Skala und „Tonauswahl“ loggen, Note per OSC an 7000+7002 bzw. 7001 senden (aktuell fehlt der ganze Block).
- Dateien: `rrn5/src/main.rs`
- Test: reine `preview_note_vol(noteadjust, x, y, w, h, noteScale, noteScaleadd, rgb) -> (note, vol)`; Tests für alle 9 Modi + Clamping 0..127.
- Verify: `cargo test --manifest-path rrn5/Cargo.toml preview` → passed
- Abhängig: 0.2
- Referenz: cpp:2833-2923.

### Schritt 2.2: F3/F4-Formel exakt + Div0-Schutz
- Ziel: Bedingung `half % noteScaleaddPartbefore < noteScaleaddPartbefore/2`, Ergebnis `half - (half % (partbefore/2)) [+ partbefore]`; kein Panic bei `partbefore==0`.
- Dateien: `rrn5/src/main.rs`
- Test: reine `scale_add(...)`; Fälle part_start true/false, partbefore 0/1/4/10, positive/negative half.
- Verify: `cargo test ... scale_add` → passed
- Abhängig: 0.2
- Referenz: cpp:2786-2823.

### Schritt 2.3: Alt+KP-BPM (fixedBpm+1)
- Ziel: Alt+KP-Recalc nutzt `fixedBpm+1` wie C++ (alle 9 Pfade).
- Dateien: `rrn5/src/main.rs`
- Test: `calc_bpm` mit explizitem Offset-Parameter; Test: Basis vs. +1.
- Verify: `cargo test ... calc_bpm` → passed
- Abhängig: 2.2
- Referenz: cpp:2512-2629.

### Schritt 2.4: Harmonics-Prädikat C++-kompatibel
- Ziel: Nur Pixel verarbeiten, für die `r != 255 && g != 0 && b != 0` gilt (C++-Semantik; Rust verarbeitet aktuell zu viele).
- Dateien: `rrn5/src/main.rs`
- Test: `harmonic_pixel_eligible(r,g,b)`; Fälle (255,0,0) nein, (0,255,0) nein, (10,10,10) ja.
- Verify: `cargo test ... harmonic` → passed
- Abhängig: 0.2
- Referenz: cpp:1676.

### Schritt 2.5: Clipboard-XML korrekt verschachteln
- Ziel: Genau ein `<Line index="i">` pro Zeile; Noten werden in die vorhandene Zeile eingefügt (aktuell Duplikat-Indizes → Renoise-Import kaputt); `xclip`-Verfügbarkeit prüfen.
- Dateien: `rrn5/src/main.rs`
- Test: reiner `build_renoise_xml(lines, notes) -> String`; Test: 16 Zeilen, 2 Noten → 16 Line-Tags, jede index genau einmal, Noten in richtiger Zeile.
- Verify: `cargo test ... renoise_xml` → passed
- Abhängig: 2.1
- Referenz: cpp:851-1006.

### Schritt 2.6: stdin token-robust + Bildverzeichnis konfigurierbar
- Ziel: `"384 384"` in einer Zeile liefert 384/384; `--dir`/`FRAMES_DIR` ersetzt `/home/apricot/Dokumente/66` (⚠E2).
- Dateien: `rrn5/src/main.rs`
- Test: Parser-Test für `read_pair` (ein/mehrere Tokens, Müll → Default).
- Verify: `cargo test ... read_pair` → passed; `rrn5 --dir tests/e2e/fixtures` nutzt Fixtures.
- Abhängig: 2.1

### Schritt 2.7: View-Präsentation angleichen
- Ziel: Right-Mouse präsentiert second **und** third; bpmk/bpmo leeren third sichtbar.
- Dateien: `rrn5/src/main.rs`
- Test: E2E-Sichtprüfung entfällt → Smoke: Prozess läuft; Log-Ausgabe.
- Verify: `tests/e2e/rrn5_smoke.sh` → Exit 0
- Abhängig: 2.6

### Schritt 2.8: `surfacesh` respektiert `SharedData2.format`
- Ziel: Pixelkonvertierung abhängig von `format`/Masken (RGBA vs BGRA), statt blind RGBA.
- Dateien: `rrn5/src/main.rs`, `rrn5/src/shared.rs`
- Test: reine `convert_to_rgba(format, pixels)`; Test BGRA→RGBA.
- Verify: `cargo test ... convert_to_rgba` → passed
- Abhängig: 2.6

---

## Phase 3 — imgsh-Merge (⚠E1)

### Schritt 3.1: `SharedMemoryEditor` schreibfähig + create-or-open
- Ziel: `write_image(w, h, rgba)` schreibt `SharedData2`+Pixel in `MyImageShm`; `open()` legt an, falls nicht vorhanden (rrn5 braucht imgsh nicht mehr als Vorläufer).
- Dateien: `rrn5/src/shared.rs`
- Test: Round-Trip: write_image → image_header_and_pixels liefert identische Pixel/Maße; Größenfehler → `Err`.
- Verify: `cargo test --manifest-path rrn5/Cargo.toml shm_roundtrip` → passed
- Abhängig: 2.8

### Schritt 3.2: rrn5 Bildnavigation
- Ziel: Tab (weiter) / Shift+Tab (zurück) lädt Bild aus dem Verzeichnis, schreibt in MyImageShm und zeigt es im Hauptfenster; Extension-Filter wie imgsh.
- Dateien: `rrn5/src/main.rs`
- Test: reine `filter_image_files(dir)`; Test mit Fixture-Verzeichnis.
- Verify: `cargo test ... filter_image && tests/e2e/rrn5_image_nav.sh` → Exit 0
- Abhängig: 3.1

### Schritt 3.3: imgsh-Entscheidung umsetzen
- Ziel: Je ⚠E1: Crate behalten (dann README-Hinweis) oder entfernen.
- Dateien: ggf. `imgsh/` löschen
- Verify: `cargo build --manifest-path rrn5/Cargo.toml && cargo build --manifest-path imgsh/Cargo.toml` (falls behalten)
- Abhängig: 3.2

---

## Phase 4 — End-to-End-Verifikation

### Schritt 4.1: it_live
- Ziel: Dump vollständig, WAV nicht still.
- Verify: `nix-shell --run "RLENGTH=3 ./target/debug/it_live ~/Documents/drumnbaesh.it --dump --wav /tmp/opencode/it.wav"` → enthält `patterns=`, `orders=`; `wav_check.py /tmp/opencode/it.wav` → RMS > 0
- Abhängig: 0.2

### Schritt 4.2: SHM-Pipeline
- Ziel: rrn5 schreibt Testbild → `shm_probe` liest identische Maße/Pixel; imgsh schreibt weiterhin kompatibel.
- Verify: `tests/e2e/shm_pipeline.sh` → Exit 0
- Abhängig: 3.1

### Schritt 4.3: pattern_recorder
- Ziel: Unter Xvfb starten, Rhythmus per Tasten aufnehmen (xdotool), CSV speichern, Renoise-Clipboard exportieren, OSC an 7005 mitschneiden.
- Verify: `tests/e2e/pattern_recorder_e2e.sh` → CSV-Datei existiert mit erwarteten Zeilen; `osc_listen.py`-JSONL enthält `/renoise/trigger/note_on`; `xclip -o` liefert valides XML mit eindeutigen Line-Indizes
- Abhängig: Phase 1, 0.2

### Schritt 4.4: bundle_udp
- Ziel: Datagramm auf 7000/7005 wird roh an Zielport 7003 weitergeleitet (inkl. Bundle/allen Typen).
- Verify: `tests/e2e/bundle_udp.sh` → Empfänger erhält identische Bytes; Nicht-OSC wird geloggt und geforwardet
- Abhängig: 0.2

### Schritt 4.5: rrn5 + OSC
- Ziel: rrn5 startet mit Fixtures, liest MyImageShm (X), sendet bei Left/Right Preview-`note_on` an 7000/7001.
- Verify: `tests/e2e/rrn5_e2e.sh` → OSC-JSONL enthält `note_on`; Prozess beendet sich auf Fenster-Close sauber (Exit 0)
- Abhängig: 2.1–3.2

### Schritt 4.6: Gesamtlauf
- Ziel: Alle Unit-Tests + alle E2E-Skripte in einem Lauf; Abschlussbericht mit Evidenz.
- Verify: `nix-shell --run "cargo test --manifest-path pattern_recorder/Cargo.toml && cargo test --manifest-path rrn5/Cargo.toml && cargo build --manifest-path bundle_udp/Cargo.toml" && bash tests/e2e/run_all.sh`
- Abhängig: alle

---

## Umsetzungsstand (2026-09-23)

Alle Schritte umgesetzt; Gesamtlauf grün (`tests/e2e/run_all.sh`, Exit 0):
19 Unit-Tests pattern_recorder, 16 Unit-Tests rrn5, 3 Unit-Tests it_live,
Builds aller Crates, E2E: it_live, it_live_live, bundle_udp, smoke_xvfb,
shm_pipeline, rrn5_e2e, pattern_recorder_e2e, pattern_recorder_flows.

### Schism-Integration (Handtest + Regression)

- Handtest auf dem echten System: Export-Pattern in Schism (Alt+P) eingefügt und
  von Schism zurückkopiert — die Rückkopie ist **zellgenau identisch** mit
  `/tmp/schism_pattern.pit` (6 Noten, gleiche Reihen/Kanäle/Pitches inkl. C#4;
  Schism schreibt Instrument `00` als leer, Zellbreite 11 + `|`).
- Export als reine Funktion `notes_to_schism_pattern` extrahiert, Unit-Test mit
  exakten Zeilen/Pitches; E2E prüft Format + Reihenfolge (Zeilen zeitbedingt
  tolerant, Unit-Test exakt).

### Nachgezogen (6. Runde): Harmonics-Bereiche

- **`useNotesSet`-X-Bereiche:** neuer reiner Helfer `in_notesset_range`
  (Set n gilt für `x ∈ [setx[n-1][0], setx[n][0])`), Unit-Test; `is_note_in_chord`
  und `is_note_in_chord_ext` nutzen ihn jetzt, `_ext` bekommt dafür das
  Pixel-x übergeben (vorher wurden alle Sets unabhängig vom Bereich geprüft).
- **C++-Bugs bewusst nicht übernommen:** `isNoteInChordpatt` übergibt den
  Chord-Index statt Pixel-x und vergleicht `i < xValueschords.size()-1`;
  Rust behält die funktionale Variante (⚠E3).

### Nachgezogen (5. Runde): letzte Interaktionen

- **`eingabe()` Dezimaltaste:** `KpDecimal` hängt jetzt `.` an (vorher der
  rohe SDL-Keycode), Helper `append_input_token` verhindert doppelte Punkte.
- **Sinus-Interpolation:** `interpolate_notes_with_sinus_pitch` ruft wie C++
  interaktiv `anfend_index()` auf; Kernlogik als reine Funktion
  `sinus_interpolate` ausgelagert inkl. Schutz bei leerem/ungültigem Bereich.
- **E2E Phase E:** kompletter `D`+`3`-Flow mit Parametern und interaktiven
  Indizes → CSV mit interpolierten Noten.

### Nachgezogen (4. Runde): Kleinkram & Doku

- **bundle_udp:** Ziele werden per `ToSocketAddrs` aufgelöst (Hostnamen wie
  `localhost`), Sendefehler pro Ziel werden geloggt statt den Relay-Thread zu
  beenden. Unit-Test.
- **rrn5 LCtrl+9:** C++-Fall-through in den Home-Handler portiert (Zeitaufnahme);
  gemeinsamer Helfer `lctrl_time_record()`.
- **rrn5 `frame0.bmp`:** speichert jetzt den rohen Capture (wie C++), nicht das
  skalierte Hauptbild.
- **Alt+G-Fall-through bewusst nicht portiert:** das C++-Fall-through (Toggle +
  `leftset` + Farbwechsel gleichzeitig) ist offensichtlich versehentlich; gemäß
  ⚠E3 ausgelassen.
- **README.md** mit Crates, Build-/Testkommandos, E2E-Hook und Workflow.
- `run_all.sh` führt jetzt auch it_live- und bundle_udp-Unit-Tests aus.

### Nachgezogen (3. Runde): restliche Fenster

- **Graph-Fenster (pattern_recorder):** `run_graph_window()` mit Maus-Zeichnen
  (obere Hälfte = Tonhöhe, Mitte = Schrittweite), `F` interpoliert die
  Step-Kurve (`interpolate_step_graph`), `V` tastet sie ab (`ae_points`),
  `O` druckt die Punkte, `0`/Close beendet und wandelt die Punkte in Noten
  (`graph_points_to_notes`, y/10 Halbtöne ab 60) und fügt sie an
  `start_idx` in `notes4` ein. Alle drei Helfer mit Unit-Tests.
- **Surface-View-Fenster (rrn5):** `surfacesh` öffnet ein eigenes Fenster
  („Surface View“, Bildgröße), zeigt das konvertierte SHM-Bild und schließt
  mit Enter (wie C++).
- **Fensterpositionen:** zweites/drittes Fenster auf 480/960 (wie C++).
- **Test-Hook:** `click:x:y`-Tokens für Maus-Events.
- **E2E:** `pattern_recorder_flows` Phase D zeichnet per Klicks, füllt/tastet
  ab und prüft die Noten-Konvertierung; `rrn5_e2e` prüft Surface View inkl.
  Enter-Schließen.

### Nachgezogen (2. Runde)

- **it_live Live-Audio (cpal 0.15):** `live_playback()` mit taktkompatiblen
  Order-Wechseln (gemeinsamer `BeatTracker` mit dem Offline-Renderer),
  `--device NAME` (Liste bei Fehler) und `--seconds N`.
- **E2E damit:** `it_live_live.sh` nutzt ein definiertes ALSA-Null-Device
  (`ALSA_CONFIG_PATH`), prüft Beat-Trigger, sauberen Exit und Fehlerpfad.
- **Weitere Flows:** `pattern_recorder_flows.sh` für CSV-Bulk (`S+9`/`F+9`),
  Schism-Export und Renoise-Import (`Z`) → CSV.
- **Bugfix (vom Import-Flow gefunden):** `renoise_note_to_midi` war nicht
  invers zu `midi_to_renoise_note` (C++: `(octave+2)`, Import landete eine
  Oktave zu hoch) und verlor Kreuz-Noten; jetzt `(octave+1)` + `#`-Parsing
  (⚠E3). Roundtrip-Test.
- **Bugfix:** cpal-Callback schrieb über das Ende des Ausgabepuffers
  (Index-Panic bei Puffergrößen, die kein Vielfaches von `channels` sind).
- **Test-Hook:** `E2E_KEYS` unterstützt jetzt alle Buchstaben, `wait:<ms>`
  und `Quit`; Prompt-Flows brauchen `Return` (vgl. `eingabe()`).

Abweichungen vom Plan (dokumentiert):

- **E2E-Tastatureingabe:** XTEST-Tasten erreichen die SDL3-compat-Apps unter Xvfb
  nicht (xev empfängt sie, SDL nicht). Daher Test-Hook `E2E_KEYS` in
  `rrn5`, `imgsh`, `pattern_recorder`: injiziert Tasten/`Quit`/`wait:<ms>` per
  `SDL_PushEvent` (nur aktiv, wenn die Env-Variable gesetzt ist).
- **Kein Window-Manager nötig:** `jwm`/`twm` verworfen; Fenster werden über die
  erste X11-Window-ID gefunden (`first_window` in `tests/e2e/lib.sh`).
- **NaN-Schutz:** Bei Aufnahmen ohne Zeitdifferenz (alle Tasten in derselben ms)
  entstanden NaN-Timestamps; `safe_divisor()` verhindert das.
- **Bildnavigation:** `currentFrame` startet bei 1 (wie im Port vorhanden);
  ein Tab springt auf Index 2.
- **modul1:** bewusst geleert statt angehängt (⚠E3), da C++-Append stale Daten
  akkumuliert; Sentinel `{1,0,0}` ist im Rust-Playback nicht nötig.
- **shell.nix:** `libopenmpt`, `xclip`, `xsel`, `wl-clipboard`, `xdotool`,
  `python3` ergänzt (Build + E2E).

## Nicht im Scope (dokumentiert)

- Graph-Fenster (`createGraphWindow`) — in C++ vorhandene, aber kaum nutzbare Funktion; nur falls explizit gewünscht.
- OSC-Empfang Port 7010/7770 — in C++ deaktiviert/ungenutzt.
- `midi_rtmidi.cpp`, `rtpmidiin.cpp` — nicht Teil der Rust-Ports.
- Kosmetik: Fensterpositionen 480/960 vs. yh/2yh, TGA-vs-BMP-Snapshots.
