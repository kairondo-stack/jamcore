# Android-App: fehlende Funktionalität ergänzen (Liste `docs/fehlende-funktionen-android.md`)

Stand: 2026-10-09 · **A und B umgesetzt** · core 141 Tests grün · APK baut ohne Warnungen
· Gerätetest offen (Handy im aktuellen Netz nicht erreichbar, siehe unten).

## Ergebnis

| Step | Status | Beleg |
|---|---|---|
| A1 MIDI-Trigger (7002) | ✅ | `osc::trigger_midi` + Test (UDP-Empfänger, Port-Mapping); im Bild-Scan neben `note_on` |
| A2 BPM-Teiler (7001) | ✅ | `bpm_und_teiler` + Test; Regler im OSC-Tab. ⚠️ sendet 7001 nur bei Teiler ≠ 1 (bewusst, im Plan dokumentiert) |
| A3 KeyEventRender.txt | ✅ | Knöpfe „Datei sichern“/„Datei laden“; Rest hinter der Datei bleibt gesperrt (resync auf MAX_SCHRITTE) |
| A4 XML laden | ✅ | `rhythm::load_xml` + Knopf, spiegelt `load_csv` |
| A5 Player-Info | ✅ | `player::info_paare`/`sample_namen` + Anzeige; dabei **Leck behoben** (`openmpt_free_string` in `sample_name`) |
| A6 Notenbremse | ⚠️ | bewusst fest 8/s (Fader war unerwünscht) |
| B1 Schism-Export | ✅ | `Pattern::to_schism` + Desktop-Regressionstest (zeichengleich), Knopf + Clipboard |
| B2 BMP-Snapshot | ✅ | `core/src/bmp.rs` + 4 Tests (Header, Padding, BGR, unten→oben), Knopf „Snapshot“ → `frame4save.bmp` |
| B3 Scan-Bereich | ✅ | `livescan::spalte_im_bereich` + 2 Tests; Regler „Bereich von/bis %“; Maske und Sweep nur im Bereich |

## Review-Befunde (Subagent) — alle abgearbeitet

- **BLOCKER** Querformat: Scan und Leiste rechneten mit verschiedenen Schrittzahlen, und
  die Maske wurde pro Frame doppelt resized → Bildbreite wird jetzt **vor** `scan_schritt`
  festgelegt (Querformat: linke Spalte), ein Resync.
- **BLOCKER** Leck: `openmpt_module_get_sample_name`-String nicht freigegeben (wird je
  Frame im Player-Tab abgefragt) → `openmpt_free_string` ergänzt.
- **WICHTIG** `keyevent_laden` kürzte die Datei-Maske → Rest bis `MAX_SCHRITTE` gesperrt.
- **WICHTIG** Sperr-Zähler zählte Abschnitte statt Spalten → `gesperrte_spalten`-Helfer.
- **KLEIN** `abschnitt_step` rechnet in u32 (kein u16-Überlauf); `bereich()` prüft nach
  dem Klemmen; rechter Rand der Leiste gehört zur letzten Spalte; Scan-Toggle setzt
  `scan_naechste_spalte` zurück; Abschnitt wächst beim Drehen mit; Scan schläft nicht mehr
  im UI-Thread (`request_repaint_after` statt `sleep`); Doku-Tippfehler behoben.
- **Tests** Randfälle für `spalte_im_bereich` (NaN/∞/invertiert/kollabiert/Breite 0).

## Offen (bewusst später)

Harmonics/Chord-Filter, Graph-Editor, Recorder-Ketten, Standbilder als Quelle,
Bild-Follow (`o`/`k`), Einzel-Slot-Wiedergabe, WAV/Dump/Order-Variation.
Siehe `docs/fehlende-funktionen-android.md`, Abschnitt „Offene Punkte“.

## Gerätetest — am 2026-10-09 bestanden

Pixel 10 Pro, Handy-Hotspot (Laptop `10.175.119.209`), adb per `adb tcpip 5555` über USB
einmal aktiviert, dann `adb connect 10.175.119.115:5555`. Belege: KeyEventRender.txt
(448 Zeilen, laden sperrt 448), Snapshot (gültige 640×480×24-BMP), Bereich (Leiste ab
10 %), XML schreiben/laden, Schism kopiert, Player-Info + Samples, Teiler 0,25 → 32,0 BPM
auf Port 7006 mitgeschnitten, MIDI-Trigger auf Port 7007 mit `,i 127`.
Details in `docs/fehlende-funktionen-android.md`.

---

## A. Klein

### A1: MIDI-Trigger (Port 7002)
- Goal: `Renoise::trigger_midi` wie in rrn5 (`/renoise/trigger/midi` mit Volume), im
  Bild-Scan neben `note_on` senden.
- Files: `core/src/osc.rs`, `android/src/image.rs`
- Test: `trigger_midi` liefert den gemappten Port (7002) und die Adresse.
- Verify: `cargo test` core; Log `Scan-Note` + `Scan-MIDI`.

### A2: BPM-Teiler (Port 7001)
- Goal: Tempo auf 7000 und Tempo×Teiler auf 7001 wie `rrn5::osc::send_bpm`; Teiler im
  OSC-Tab einstellbar (Vorgabe 1,0).
- Files: `core/src/osc.rs` (Helfer `bpm_und_teiler`), `android/src/lib.rs`
- Test: `bpm_und_teiler(128.0, 0.5) == (128.0, 64.0)`; Teiler 1 lässt 7001 aus.
- Verify: Log `Tempo 128.0 BPM -> 172.22.99.181:7005` bzw. zusätzlich Port 7006.

### A3: KeyEventRender.txt laden/sichern (alter Step 7)
- Goal: Knöpfe im Bild-Tab: Maske als `KeyEventRender.txt` in den App-Ordner schreiben
  (`als_keyevent`) und von dort lesen (`aus_keyevent`, danach `resync`).
- Files: `android/src/image.rs` (Datei-E/A), Kern kann alles.
- Verify: Gerätetest + `adb shell cat` der Datei (eine Zeile je Schritt).

### A4: Renoise-XML laden (Rhythmus)
- Goal: `load_xml` spiegelt `load_csv` mit `Pattern::from_renoise_xml(…, 16)`.
- Files: `android/src/rhythm.rs`
- Verify: Gerätetest: `XML laden` liest `patterns/pattern N.xml`.

### A5: Player-Info
- Goal: Titel/Artist/Format und Sample-Namen im Player-Tab anzeigen.
- Files: `android/src/player.rs` (`info_paare()`, `sample_namen()`), `android/src/lib.rs` (UI)
- Verify: Gerätetest, Liste unter dem Player.

### A6: Notenbremse
- **Bewusst nicht**: der Nutzer hat den „Noten/s“-Fader ausdrücklich entfernt. Bleibt
  fest 8/s; die Zeile in `docs/fehlende-funktionen-android.md` wird als „bewusst“ markiert.

## B. Mittel

### B1: Schism-Export
- Goal: `Pattern::to_schism()` — Portierung von `notes_to_schism_pattern`
  (`Pasted Pattern - IT`, 12 Zeichen je Zeile, `|C-400......`).
- Files: `core/src/grid.rs` (TDD mit der Desktop-Regression), `android/src/rhythm.rs` (Knopf + Clipboard)
- Test: exakt die Zeilen aus `pattern_recorder` (`lines[1] == "|C-400......"`, Länge 12,
  `lines.len() == n + 1`).

### B2: Snapshot als BMP
- Goal: eigener kleiner BMP-Schreiber (24 Bit, bottom-up) im Kern — kein neues Crate.
- Files: `core/src/bmp.rs` (neu, TDD), `android/src/image.rs` (Knopf „Snapshot“)
- Test: Kopf `BM`, Größenfelder, Zeilen-Padding auf 4 Byte, Pixelreihenfolge unten→oben.
- Verify: Gerätetest, Datei per `adb pull` + `file`-Kommando.

### B3: Scan-Bereich links/rechts
- Goal: Sweep nur über einen Bereich (rrn5 Alt+D/Alt+C): zwei Regler „von %/bis %“.
- Files: `core/src/livescan.rs` (Helfer `spalte_im_bereich`, TDD), `android/src/image.rs`
- Test: Bereich 25–75 % bildet Schritt 0 auf x=160, den letzten auf 480 ab (Bild 640).
- Verify: Gerätetest: Maskenleiste und Scan liegen nur im Bereich.

## C. Groß (eigener Abschnitt, danach)

- Harmonics/Chord-Filter (rrn5: `chordTypes`, `is_note_in_chordpatt`, `useNotesSet`)
- Graph-Editor (pattern_recorder `G`)
- Recorder-Ketten (`scale_to_range_num`, Sinus-Interpolation, `vmidi`, Timestamp)
- Standbilder als Quelle (PNG/JPG/BMP — braucht einen Decoder)
- Bild-Follow (`o`/`k`), Recorder-Aufnahmemodell, Einzel-Slot-Wiedergabe
