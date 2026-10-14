# Plan 2026-10-14: Maske 2, ein BPM (Tempo), Bild 2, polyphoner Klang, Instrument-Mapping

Stand: 2026-10-14 · Design abgestimmt · **Step 1–3 fertig und am Geraet belegt,
Step 4 umgesetzt (Build/Geraetetest offen)**, Step 5–8 offen.

| Step | Status | Beleg |
|---|---|---|
| 1 Numpad-Spike | ✅ | `KEYCODE_NUMPAD_1/9/0/ENTER` kamen als `Num1/Num9/Num0/Enter` an (Log „key-spike", Pixel 10, 2026-10-14) |
| 2 Ziffer → Position | ✅ | `position_linear` + `position_numpad` + 2 Tests (Kern 225 gruen) |
| 3 Maske 2 | ✅ | Geraet: Touch „5" → `taste 5 (indexnum 5)`; Numpad 7/1 → die raeumlich gleichen Tasten; Enter → „Ende" (weitere Tasten ignoriert); „neu" nimmt wieder auf; Folge speist den gruenen Balken („Maske aus Folge: 1 Zeitpunkte auf 512 Spalten"); alte Maske: Numpad 5 → `feld 5 (indexnum 4)` (linear) |
| 4 Ein BPM/Tempo | ✅ | ein Wert `tempo_bpm`; Bild- und Rhythmus-Regler schreiben zurueck; Tempo+Session ein Tab; Reglerzug → `tempo_ins_link` (capture/set_tempo/commit). Geraet: Bild→140→`link tempo=140.00`, Tempo-Tab zeigt 140; Rhythmus→220→Link. |
| 5 Polyphon | ✅* | Kern: `Notenwert` (1/64…4 Takte, ms je BPM) + `Stimme`/`mischen` + 4 Tests (Kern 232 gruen); Android: Endlos-Mixer in 10-ms-Bloecken, aelteste Stimme weicht; UI „Stimmen 1–256" + „Laenge" mit ms. Geraet: Regler da (8 / 1/16 = 117 ms), kein Absturz. *Hoerprobe (mehrstimmig/Ausklingen) steht beim Nutzer aus. |
| 6 Buendelung | ✅ | Kern: `buendel_spanne`, `hat_im_buendel`, `toggle_buendel` + 3 Tests; Geraet: Buendelung 16 → 4 Bloecke (1-128, …), Zellen 1/33/65; Tippen → „16 Schritte gesetzt: C-2 (16 Noten)"; Cursor im richtigen Bund. |
| 7 Bild 2 | ✅ | Neuer Tab „B2": Quelle im **Vollbild** (deckend skaliert — 4:3 auf 20:9 haette sonst Balken), drei halbtransparente, immer sichtbare, scrollbare Leisten (oben Kamera/Scan/ins Pattern, links Quelle/Zoom/Takt/BPM/Schritte, rechts Maske/Toleranz/Auswahl/Malen/Kanal/Harmonics/Notenmenge). Derselbe `ImagePanel`-Zustand wie „Bild": Kamera/Scan/Maske laufen nur einmal. Geraet: Vorschau „Frame 118" im Vollbild hinter den Leisten, Hoch- und Querformat. |
| 8 Instrument-Tab | ✅* | Neuer Tab „Instr“ neben Player: Instrumente/Samples getrennt listen (`openmpt_module_get_instrument_name` neu gebunden, Platzhalter fuer leere Plaetze), je Eintrag ein Notenbereich von/bis mit Notennamen. Ueberlappung = **alle** klingen (`instrument::treffer`), ausserhalb still; ohne einen einzigen Bereich gilt weiter der Standard (Instrument 30), sonst waere der Modul-Klang verstummt. Kern: 4 Tests (236 gruen). Geraet: 7 Instrumente von `40nix_uwu_light.xm` gelistet, Eintrag 1 auf D-3…D#5 gesetzt, Footer „7 Eintraege, 2 mit Bereich“, kein Absturz. *Hoerprobe beim Nutzer. |

**Fixes beim B2-Test (2026-10-14):** Die Live-Textur wurde nur im „Bild“-Pfad
aktualisiert — B2 zeigte „Kamera startet …“ ohne Bild. Jetzt gemeinsamer Helfer
`textur_aktualisieren` fuer beide Layouts.

**OSC geloest (2026-10-14):** Ursache war doppelt — der IP-Tippfehler `127.22.99.181`
(Loopback) und die gewechselte Netz-IP. Gebaut: Reconnect, wenn das Feld-Ziel von der
laufenden Verbindung abweicht, und Merken von IP/Port in `osc.txt` (beim Verbinden/Senden).
Beweis: Listener am Laptop bekam `/renoise/trigger/note_on` (−1,−1,60,100). Aktuell gemerkt:
`10.77.0.209:7005`.

Neu angelegt: `docs/2026-10-14-api-pipeline.md` + `.svg` (Werte-Fluesse),
`docs/bedienungsanleitung.md` (Nutzersicht).

Entscheidungen des Nutzers (verbindlich):

- **Maske 2**: 10er-Feld in Numpad-Anordnung (7-8-9 / 4-5-6 / 1-2-3 / 0),
  **Ziffer = Feldposition**, USB-Nummernblock wirkt in **beiden** Masken.
  **Ende** (Display-Taste, Numpad-**Enter**) stoppt die Aufnahme bis „neu";
  die 8-s-Pause entfaellt in Maske 2 (alte Maske behaelt sie).
- **Bild 2**: Bildquelle immer im Vollbild; transparente, **immer sichtbare**,
  scrollbare Leisten; oben Kamera/Scan/ins Pattern, links Quelle/Zoom/Bereich/
  Schritte/BPM, rechts Aufnahme/Farbe, Toleranz, Malen, Harmonics, Notenmenge;
  Maskenleiste bleibt oben. **Derselbe** ImagePanel-Zustand wie „Bild".
- **Ein BPM**: ein zentraler Wert; Tempo- und Session-Tab werden **ein** Tab
  (Messung + Link + BPM + Tap); Regler in Rhythmus, Bild, Bild 2 schreiben ihn.
  **Reglerzug schreibt ins Link** (`capture_app_session_state` →
  `set_tempo` → `commit_app_session_state`).
- **Polyphoner Tongenerator**: 1–256 Stimmen (Vorgabe 8), Ueberlast stiehlt
  die aelteste Stimme. **Tonlaenge als Notenwert 1/64 … 4 Takte**, ms-Anzeige
  (haengt am BPM). Regler in Rhythmus + Bild 2.
- **Instrument-/Sample-Tab** (neben Player): beide Listen getrennt; jedem
  Eintrag laesst sich ein **Notenbereich** zuweisen (von/bis). **Ueberlappung
  heisst: alle passenden Instrumente klingen zusammen.** Ausserhalb aller
  Bereiche: **still**.
- **Rhythmus-Buendelung**: Patternlaenge bleibt; ein Regler buendelt Schritte
  je Zelle (1, 2, 4, 8, 16, 32). Zelle leuchtet, wenn ein gebuendelter Schritt
  eine Note hat; Tippen setzt/loescht alle Schritte der Zelle.

Reihenfolge nach Risiko: Numpad-Spike → Maske 2 → ein BPM → Klang polyphon →
Rhythmus-Buendelung → Bild 2 → Instrument-Tab.

## Step 1: Numpad-Spike (Risiko zuerst)

- Goal: Beweisen, dass ein USB-Nummernblock auf dem Geraet als
  `egui::Key::Num0..Num9` / `Enter` ankommt (Quelle egui-winit 0.36.2:
  `Digit0|Numpad0 → Num0`, `Enter|NumpadEnter → Enter` — im Registry-Quelltext
  bestaetigt).
- Files: `android/src/maske.rs` (temporaere Log-Zeile in `ui()`).
- Test: keine (Spike).
- Verify: `adb logcat | grep jamandroid` zeigt beim Tippen auf dem Numpad
  „taste 5" / „taste enter" → expected: jede Ziffer und Enter erscheinen.
- Depends on: —
- Rollback: Log-Zeile entfernen.

## Step 2: Kern — Ziffer wird Position

- Goal: Reine Zuordnung `Ziffer (+ Feldgroesse) → Position` als Kernfunktion.
- Files: `core/src/maske.rs`, Tests im selben File.
- Test: `ziffer_trifft_position`: 1→0 … 9→8, 0→9; Feld mit 4 Tasten: nur
  Positionen < 4 treffen; Ziffer 5 im 4er-Feld → None.
- Verify: `cargo test --manifest-path core/Cargo.toml maske` → expected: gruen.
- Depends on: —

## Step 3: Maske 2 (Panel, Tab, Ende, Numpad)

- Goal: Neuer Tab „M2" mit 10er-Numpad-Feld, Aufnahme wie Maske, Ende-Knopf,
  Tastatureingabe; alte Maske hoert ebenfalls auf das Numpad.
- Files: `android/src/maske2.rs` (neu), `android/src/maske.rs` (Tastenpfad
  teilen), `android/src/lib.rs` (Tab, Tab-Leiste in zwei Reihen).
- Test: Kern-Tests aus Step 2 decken die Zuordnung; Geraetetest deckt den Rest.
- Verify: Geraet: Numpad 5 → Feld 5 angeschlagen (Log „feld … bei …");
  Enter/„Ende" → `fest`; naechster Tastendruck stoppt (keine neue Aufnahme);
  „neu" nimmt wieder auf. `cargo test --manifest-path core/Cargo.toml`.
- Depends on: Step 1, Step 2

## Step 4: Ein BPM + Tempo-Tab

- Goal: Ein zentraler BPM-Wert in `JamApp`; alle Regler (Rhythmus, Bild,
  Bild 2 spaeter) schreiben ihn; Tempo- und Session-Tab werden ein Tab;
  ein Reglerzug schreibt per `capture_app_session_state → set_tempo →
  commit_app_session_state` ins Link.
- Files: `android/src/lib.rs` (Zustand, Tab-Zusammenlegung),
  `android/src/osc.rs` (tempo_bpm entfaellt), `android/src/rhythm.rs`
  (`bpm`/`render_bpm` lesen den zentralen Wert), `android/src/image.rs`
  (`scan_bpm` liest ihn), `android/src/mic.rs` (Uebernehmen schreibt ihn).
- Test: `core` bleibt gruen (`session::tempo_gesamt`); neu:
  `tempo_wird_geklemmt` (Bereich 40–220) im Kern.
- Verify: Geraet: BPM in Rhythmus aendern → Tempo-Tab und Bild zeigen denselben
  Wert; ein Testton im Bild-Scan laeuft mit dem neuen Tempo
  (`1/4 Takt = … s`-Zeile). `cargo test --manifest-path core/Cargo.toml`.
- Depends on: Step 3 (nicht fachlich, nur Reihenfolge)

## Step 5: Polyphoner Tongenerator

- Goal: Stimmen-Mixer mit waehlbarer Stimmenzahl (1–256, Vorgabe 8) und
  Tonlaenge als Notenwert (1/64 … 4 Takte, ms-Anzeige); aelteste Stimme wird
  gestohlen.
- Files: `core/src/klang.rs` (Mixer + Notenwert→ms), Tests dort,
  `android/src/klang.rs` (Stream-Schleife statt 120-ms-Stuecke),
  `android/src/rhythm.rs` (Regler Stimmen/Laenge), `android/src/image.rs`
  (Laenge/Stimmen an den Spieler reichen).
- Test: Kern: `notenwert_in_ms` (1/64 bei 120 BPM = 31 ms, 1/4 = 500 ms,
  4 Takte = 8000 ms), `mixer_summiert_und_klemmt` (zwei Stimmen addieren sich,
  ohne Uebersteuerung; aelteste wird gestohlen).
- Verify: Geraet: Akkord im Rhythmus klingt mehrstimmig; Laenge 1/4 ≈ 0,5 s
  hoerbar; `cargo test --manifest-path core/Cargo.toml`.
- Depends on: Step 4 (BPM fuer ms-Anzeige)

## Step 6: Rhythmus-Buendelung

- Goal: Anzeige-Regler „Buendelung" (1, 2, 4, 8, 16, 32 Schritte je Zelle);
  Zelle leuchtet bei irgendeiner Note im Buendel; Tippen setzt/loescht das
  ganze Buendel.
- Files: `core/src/grid.rs` (Buendel-Abfrage + Toggle, Tests),
  `android/src/rhythm.rs` (Raster zeichnen ueber Buendel).
- Test: `buendel_leuchtet_bei_einer_note`, `toggle_setzt_alle_schritte`,
  `buendel_rest_am_ende` (Patternlaenge kein Vielfaches).
- Verify: `cargo test --manifest-path core/Cargo.toml grid` → gruen; Geraet:
  512er-Pattern, Buendelung 16 → 16 Zellen je Reihe, Tipp setzt 16 Schritte.
- Depends on: —

## Step 7: Bild 2 (Vollbild-Layout)

- Goal: Zweiter Tab „B2": Bild im Vollbild, transparente Leisten wie
  abgestimmt; derselbe Zustand wie „Bild" (Kamera/Scan nur einmal).
- Files: `android/src/image.rs` (zweites Layout `ui_vollbild`, Leisten mit
  Alpha), `android/src/lib.rs` (Tab).
- Test: keine Kern-Tests (Layout); Geraetetest.
- Verify: Geraet: Kamera laeuft in B2, Scan/ins Pattern oben erreichbar,
  Leisten scrollen und sind halbtransparent; zurueck in „Bild" ist der Zustand
  gleich (gleiche Schritte/Ziel).
- Depends on: Step 4

## Step 8: Instrument-/Sample-Tab mit Notenbereichen

- Goal: Tab „Instr." neben Player: Instrumente und Samples getrennt listen,
  je Eintrag Bereich von/bis (Notennamen + Slider); Noten spielen **alle**
  passenden Eintraege; ausserhalb still.
- Files: `core/src/openmpt.rs` (Namen/Zaehlung: `get_instrument_name`
  ergaenzen, `get_sample_name` vorhanden), `core/src/instrument.rs` (neu:
  Bereichslogik + Tests), `android/src/player.rs` (Listen),
  `android/src/instr.rs` (neu: UI), `android/src/lib.rs` (Tab).
- Test: Kern: `bereich_trifft_note`, `ueberlappung_spielt_alle`,
  `ausserhalb_bleibt_still`, `bereich_von_groesser_bis` (tauschen).
- Verify: Geraet mit Modul: zwei Instrumente auf C3–C5 legen, Note C4 →
  beide klingen; Note ausserhalb → still. `cargo test --manifest-path core/Cargo.toml`.
- Depends on: Step 5 (Klangweg), Step 4

## Aufwand (grob, inkl. Geraetetest)

| Step | Aufwand |
|---|---|
| 1 Numpad-Spike | 0,5 h |
| 2 Zuordnung | 0,5 h |
| 3 Maske 2 | 1 Tag |
| 4 Ein BPM/Tempo | 0,5–1 Tag |
| 5 Polyphon | 1 Tag |
| 6 Buendelung | 0,5 Tag |
| 7 Bild 2 | 1,5–2 Tage |
| 8 Instrument-Tab | 1–1,5 Tage |
| **Summe** | **~5,5–7 Tage** |

## Risiken

- **USB-Numpad**: Tastencodes muessen am Geraet bestaetigt werden (Step 1);
  Quelle bestaetigt die egui-Zuordnung bereits.
- **Link set_tempo**: „letzter Schreiber gewinnt" im Netz; Follow bleibt an.
- **Bild 2**: Leisten verdecken im Hochformat die Bildraender (gewollt);
  zwei Spalten muessen schmal genug fuer 317 pt bleiben.
- **Instrument-FFI**: `get_instrument_name` fehlt noch in der Bindung
  (hinzufuegen, inkl. `openmpt_free_string`).
- **Tab-Leiste**: 13 Tabs — auf zwei Reihen ueber die volle Breite umbauen.
