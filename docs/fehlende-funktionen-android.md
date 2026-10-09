| 2 | ✅ | **WAV-Rendern** (vor dem Rendern auf Order 0/Row 0 und ohne Wiederholung wie ein frisch geladenes Modul; zweimal rendern ✓)# Was der Android-App fehlt (gegenüber dem Laptop-Rust-Paket)

Vergleich von `docs/funktionen-vor-der-portierung.md` (Stand `be627ca`, vor der
Portierung) mit `docs/funktionen-android-app.md`. Jede Lücke ist am Code geprüft;
„nicht übertragbar“ heißt: die Funktion hängt an einem anderen Programm oder an
Prozessgrenzen, die es auf Android nicht gibt.

**Stand 2026-10-09: Die Abschnitte A und B sind umgesetzt** (✅), Kern-Tests 141 grün,
APK baut ohne Warnungen. Der **Gerätetest** der Datei-Funktionen (KeyEventRender.txt,
XML laden, Snapshot, Player-Info, Schism-Clipboard, Scan-Bereich) steht noch aus, weil das
Handy im aktuellen Netz nicht per adb erreichbar war.

✅ umgesetzt am 2026-10-09 · ✅ (nur am Gerät zu prüfen) · ⚠️ bewusst anders · ❌ offen

## Aus `it_live` (Modul-Player)

| # | Status | Fehlt/Details |
|---|---|---|
| 1 | ✅ | **Modul-Info und Sample-Namen** — `player.rs::info_paare()` (Titel, Künstler, Format, Länge, Aufbau) und `sample_namen()`; Anzeige im Player-Tab. Beim Abarbeiten fiel ein **libopenmpt-Leck** auf (`sample_name` gab den String nicht frei) — behoben. |
| 2 | ✅ | **WAV-Rendern** — Knopf im Player (Sekundenfeld, Vorgabe 60 s) schreibt nach `wav/<name>.wav`; Kern `wav::wav_stereo` (Tests). Gerätetest: `file` erkennt „RIFF, WAVE PCM 16 bit stereo 44100 Hz", 2.646.000 Frames = 60,0 s |
| 3 | ✅ | **Struktur-Dump** — aufklappbar „Struktur" im Player: Order, Pattern, Zeilen je Pattern, `<BREAK>`; Gerätetest: 26 Orders mit Patterns und 128 Zeilen |
| 4 | ✅ | **Beat-genaue Order-Variation** — im WAV-Render wie `render_variation`: alle vier Beats Order auf `creative % 8`, nur wenn das Pattern später vorkommt, Zeile bleibt; Kern `BeatTracker` + `order_mit_pattern` (Tests) |

*Nicht übertragbar:* `--device`, `--seconds`/Enter, cpal (ersetzt durch AudioTrack).

## Aus `rrn5` (Bild-Tab)

| # | Status | Fehlt/Details |
|---|---|---|
| 1 | ✅ | **Scan-Bereich links/rechts** — Regler „Bereich von/bis %“ + „ganz“ im Zoom-Block; Kern `livescan::spalte_im_bereich` (Tests inkl. NaN/Invertiert/Klemmen). Sweep und Maskenleiste liegen nur im Bereich. |
| 2 | ⚠️ | **Scan-Schrittweite `iadd`** — bewusst weg (der Sweep läuft über die Schrittzahl); die Schrittzahl ersetzt sie |
| 3 | ⚠️ | **Zeilen-Ausdünnung `jadd`** — laut Desktop-Beleg nur bei der Maus-Rasterabtastung (mittlere Taste) wirksam, nicht im Live-Scan; auf Touch nicht übertragbar |
| 4 | ✅ | **Alle neun Zuordnungen** — 0 = Farbe (`note_volume_farbe`), 1–8 nach den Desktop-Formeln (6/7 waren vertauscht und sind korrigiert), UI bietet 0…8 |
| 5 | ✅ | **Harmonics-Filter** — Kern `harmonie.rs` (8 Tests): Dur/Plagal/Moll, Quinte, Transposition und Notenmengen mit X-Bändern (`in_notesset_range` mit Desktop-Test). UI im Bild-Tab (Akkord-Wahl, Bänder mit Endspalte), Bänder kommen über „Notenmenge (Bild-Tab)" aus dem Kreis. Der Desktop-Chord-Teil (`chordTypes`/`is_note_in_chordpatt`) ist dort toter Code (Liste wird nie gefüllt) und bleibt bewusst aus. Gerätetest: Dur → 35 Noten, alle Klasse 0; Band {8} → 0 Noten |
| 6 | ❌ | **Frequenzspanne/Teiler** (`y_ashape`) fehlt; der Quintenzirkel (Kreis-Tab) ist da |
| 7 | ✅ | **KeyEventRender.txt** — Knöpfe „Datei sichern“/„Datei laden“ im Bild-Tab (`als_keyevent`/`aus_keyevent`); Rest hinter der Datei bleibt gesperrt (rrn5-Semantik). Die BPM/Dauer-Übernahme aus der Datei gibt es nicht mehr (die App hat **ein** Tempo). |
| 8 | ✅ | **Snapshot als BMP** — Knopf „Snapshot“ schreibt `frame4save.bmp`; eigener BMP-Schreiber im Kern (`core/src/bmp.rs`, Tests) ohne neue Abhängigkeit |
| 9 | ✅ | **Bilddateien als Quelle** — Ordner `bilder/` (png/jpg/jpeg/bmp), laden über die `image`-Kiste, verkleinert auf 640er-Kante; Auswahl im selben Dropdown wie Videos |
| 10 | ✅ | **MIDI-Trigger (Port 7002)** — `Renoise::trigger_midi` (Test) läuft im Bild-Scan neben `note_on` mit, wie im Desktop |
| 11 | ✅ | **BPM-Teiler (Port 7001)** — Regler „Teiler“ im OSC-Tab + `bpm_und_teiler` (Test). ⚠️ Anders als rrn5 sendet die App Port 7001 nur, wenn der Teiler ≠ 1 ist. |
| 12 | ❌ | **Pattern-XML aus den Scan-Treffern** (`g`) — der Tagebuch-Export zeigt den Masken-Ausschnitt (noch ohne Noten) |
| 13 | ⚠️ | **Freilaufender Scan** — der Frei/Takt-Schalter ist weg; es gilt immer die Taktdauer (Minimum 1/36 Takt ist schnell, aber nicht frei) |
| 14 | ⚠️ | **Notenbremse einstellbar** — bewusst fest 8/s (der Fader war ausdrücklich unerwünscht) |
| 15 | ❌ | **Bild-Follow als KeyEvents** (`o`/`k`) fehlt |

*Nicht übertragbar:* X11-Aufnahme, `MyImageShm`, drittes SDL-Fenster als Palette.

## Aus `pattern_recorder`

| # | Status | Fehlt/Details |
|---|---|---|
| 1 | ⚠️ | **Tastatur-Aufnahmemodell** — bewusst ersetzt durch das Tastenfeld der Maske (jeder Druck = Schritt); die Ziffern-/Faktor-/RStep-Varianten fehlen |
| 2 | ❌ | **Notenrampe** (`2`) fehlt |
| 3 | ❌ | **remove_and_record** (`E`) fehlt |
| 4 | ❌ | **fdx (`V`), modul1 (`M`)** fehlen |
| 5 | ❌ | **Graph-Editor** (`G`) fehlt |
| 6 | ❌ | **Transformationsketten K/D/B** — vorhanden: Transponieren, Umkehren, Q2/Q4, Glide, Instrument, Skalenfilter; offen: `scale_to_range_num`, `totalDi`, Indexverschiebung, Duplikate entfernen, Timestamp-Transformation, Notenlängen, Sinus-Pitch-Interpolation, `vmidi`, Mehrpattern-Interpolation |
| 7 | ✅ | **Schism-Clipboard-Export** — `Pattern::to_schism()` mit der Desktop-Regression als Test (zeichengleich), Knopf „Schism kopieren“ im Rhythmus-Tab |
| 8 | ✅ | **Renoise-XML einlesen** — Knopf „XML laden“ liest `patterns/pattern N.xml` (`Pattern::from_renoise_xml`) |
| 9 | ✅ | **KeyEventRender.txt schreiben** — siehe rrn5 Nr. 7 (Step 7 erledigt) |
| 10 | ❌ | **Pattern-Dauer je Slot** (`P`) — es gibt eine globale Taktdauer |
| 11 | ❌ | **Slot-Wiedergabe** (`H`, `Y`, `O`, `U`) — 8 Slots und „Rendern“ gibt es, keine Einzel-Slot-Wiedergabe |

*Nicht übertragbar:* `ZeitfaktoSharedMemory`, xclip/xsel (Android hat eigenes Clipboard),
SDL-Audio (ersetzt durch `klang.rs`).

## Aus `bundle_udp` und `imgsh`

- **OSC-Relay** (`bundle_udp`) bewusst nicht auf Android — die App sendet direkt; der
  Relay läuft bei Bedarf auf dem Laptop.
- **Bildbetrachter `imgsh`** entfällt (kein Prozess-Shared-Memory); die Rolle
  „Bilder zuführen“ hat der Ordner `videos/` — **Standbilder fehlen** (rrn5 Nr. 9).

## Runde 2 (2026-10-09): Zuordnungen und Standbilder erledigt

- **Alle neun Notenzuordnungen** — 0 = Farbe (`note_volume_farbe`), 1–8 nach
  `event_note_volume`; dabei war 6/7 im Kern vertauscht (korrigiert). Kern-Tests 143 grün;
  Gerätetest: der Modus-Knopf zeigt „Note: Farbe".
- **Standbilder als Quelle** — `bilder/` (png/jpg/jpeg/bmp) wird mitgescannt, Laden über
  die `image`-Kiste, auf 640er-Kante verkleinert. Gerätetest mit einem farbigen 8×8-BMP:
  blau/weiß oben, rot/grün unten, korrekt dargestellt; Log „Standbild geladen: … (8x8)".
- **`jadd`** ist laut Desktop-Beleg (Subagent, `rrn5/src/main.rs` 2619–2659) Teil der
  Maus-Rasterabtastung (mittlere Taste), nicht des Live-Scans → als nicht übertragbar
  festgehalten.

## Offene Punkte, nach Nutzen sortiert

1. **Recorder-Ketten** + Graph-Editor + Notenrampe/remove_and_record
   (pattern_recorder Nr. 2–6)
2. **Bild-Follow** (rrn5 Nr. 15) und **Einzel-Slot-Wiedergabe** + Pattern-Dauer
   (pattern_recorder Nr. 10–11)

## Gerätetest — am 2026-10-09 auf dem Pixel 10 Pro durchgeführt

Alle Punkte bestanden (Netz: Handy-Hotspot, Laptop `10.175.119.209`, adb über
`adb tcpip 5555` + `adb connect 10.175.119.115:5555`):

- **KeyEventRender.txt**: „alle aus" → sichern → Datei hatte 448 Zeilen `x;0;0;0`
  (Format wie am Desktop); „alle an" → laden → Status „448 Schritte, 448 gesperrt"
  und „448 von 448 gesperrt" (Spaltenzähler)
- **Snapshot**: `frame4save.bmp`, `file` meldet „PC bitmap, Windows 3.x, 640 x 480 x 24,
  image size 921600, bits offset 54"
- **Bereich**: „von 10 %" → grüne/rote Maskenleiste beginnt exakt bei 10 % der Bildbreite
- **XML schreiben/laden**: `patterns/pattern 0.xml`, Log „XML geschrieben/geladen"
- **Schism kopieren**: Log „Schism-Pattern kopiert" (Format deckt der Desktop-Regressionstest ab)
- **Player-Info**: „Künstler: Veertig Nix / Format: FastTracker 2 v1.04 / Länge 124,8 s /
  Aufbau: 26 Orders, 26 Patterns, 18 Kanäle, 7 Samples" + aufgeklappte Sample-Namen
- **BPM-Teiler**: Teiler 0,25 → Status „Tempo 128.0 BPM -> 10.175.119.209:7005,
  32.0 BPM -> Port+1"; ein UDP-Mitschnitt auf Port **7006** fing `/renoise/song/bpm`
  mit `32.0` (128 × 0,25) ab; Port 7005 hielt die laufende Renoise (PID 4947) selbst
- **MIDI-Trigger**: Farbauswahl + Scan → Logs „Scan-Note 69 -> …:7005" und
  „Sweep beendet, 3 Noten, 101643 gezeichnet, 1/4 Takt (0.47 s)"; ein Mitschnitt auf
  Port **7007** (Basis+2) fing `/renoise/trigger/midi` mit `,i 127` ab

Damit sind die Abschnitte A und B **vollständig verifiziert**.
