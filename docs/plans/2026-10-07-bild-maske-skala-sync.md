# Bild-Tab: Maske auf dem Bild, Zoom, Taktdauer-Fader, X-Auflösung = Patternlänge, eine IP/BPM

Datum: 2026-10-07 · Stand: core 132 Tests grün · Debug-APK 30,9 MB auf dem Gerät.

## Stand (2026-10-08)

| Step | Inhalt | Beleg |
|---|---|---|
| 1 | `core/src/scanmask.rs` (13 Tests) | `cargo test` core: 132 grün |
| 2 | `core/src/takte.rs` (7 Tests) | dito |
| 3 | Bild-Tab: Maske auf dem Bild, Zoom, Taktdauer, Scrollen | Gerätetest: `maske Schritt 211/512 aus`, roter Streifen an der Tippstelle |
| 3b | `core/src/gesten.rs` (6 Tests) + `android/src/touch.rs` | Log `gesten: 2 Finger, Zoom 1.51`, 3 Finger verschieben |
| 3c | Maske: nur Start-/Endtaste, Halten = nächster Schritt | Wisch 10→5 setzte nur Schritt 3 und 4 |
| 3d | Querformat: Bild links, Steuerung rechts in zwei Spalten | Screenshot: keine Überlappung, alle Zeilen sichtbar |
| 4 | X-Auflösung = Patternlänge, Start/Ende wählbar, CSV normiert | Tagebuch: „X-Auflösung 16 Schritte", XML `Line index="0..15"` |
| 5 | Eine IP für OSC und Tempo | Tempo-Tab zeigt „Ziel: 172.22.99.181:7005" ohne eigene Felder |
| 6 | Eine BPM (Follow gewinnt) + `tempo_gesamt` (2 Tests) | Log `Tempo 128.0 BPM -> 172.22.99.181:7005`, Tagebuch 128,0 BPM |

Offen: KeyEventRender.txt-Import (Step 7, wie gewünscht zurückgestellt); die
OSC-IP wird **nicht** gespeichert und ist nach jedem App-Start wieder 127.0.0.1.

## Ausgangslage (am Desktop nachgelesen, mit Dateistelle)

| Thema | Desktop | Datei |
|---|---|---|
| Maske speichern | `s` `0` fragt „Width?" und „Start?", schreibt `KeyEventRender.txt` mit **einer Zeile je x-Spalte**: `x;key;iialpha;xkeyk` | `pattern_recorder/src/main.rs:4560` |
| Maske laden | `space` liest die Datei **und** setzt das BPM auf `/renoise/song/bpm` | `rrn5/src/main.rs:2456` |
| Maske an/aus | `b` → `biialpha` | `rrn5/src/main.rs:2390` |
| Maske auswerten | `mask_alpha(&alphas, x)`; **außerhalb des Vektors gilt 0.0 = gesperrt**; `alpha` skaliert Farbe (`scale_rgb`) und Lautstärke | `rrn5/src/main.rs:756`, `core/src/livescan.rs:165` |
| Sweep = Beat | `pixelsPerBeat = xw * yh`, `timePerPixel = secPerBeat / pixelsPerBeat` | `rrn5/src/main.rs:632` |
| Taktdauer | `l` `p` = „Dauer in Beats?" (Vorgabe 4), `←` ÷2, `→` ×2, `↑` ÷3, `↓` ×3 | `pattern_recorder/src/main.rs:4458` |
| Andere Längen | „convert notes to Renoise pattern with length?" / „… Schism pattern with length?" | `pattern_recorder/src/main.rs:4344` |
| Patternlänge | `width` (Schritte), `width_per_loop = width / loops`, `zeitfakto = (60/bpm) * dauer` | `pattern_recorder/src/main.rs:4566` |

## Festlegungen

- **Maske**: ein Wert je Spalte des Bildes, auf/ab (später 0..1 aus patternrec).
  „sync zur X-Auflösung" ist der Normalfall, „frei" bleibt als Alternative.
- **Ein Sweep über die Bildbreite ist die Einheit.** Wie viele Takte das sind,
  sagt der **Taktdauer-Fader**: `÷2 ×2 ÷3 ×3`, Bereich **1/36 Takt bis 36 Takte**.
  Damit braucht es keine Unterscheidung Sweep/Takt mehr.
  Vorgabe **1/4 Takt** = ein Sweep ist ein Beat (wie `rrn5`), also bei 120 BPM 0,5 s.
- **X-Auflösung = Schrittzahl = Patternlänge.** Kein zusätzlicher Teiler: die Anzahl
  Schritte ist die angezeigte Breite des Bildes in Punkten mal Zoomfaktor
  (Zoom 1.0 im Hochformat ≈ 317 Schritte, im Querformat ≈ 872). Jeder Schritt
  sampelt die Kameraspalte `x = schritt * kamera_breite / schritte` (nächster
  Nachbar). Wer die Länge festhalten will, hakt „Schritte festhalten" an, sonst
  ändert Zoom die Patternlänge mit. Deshalb: **Zoom zuerst, dann Takte, dann
  Pattern**.
- CSV bleibt **normiert**: `timestamp,instrument,pitch,glide` mit `timestamp` von 0.0
  bis 1.0 chronologisch (`Pattern::timestamp`, `core/src/grid.rs:108`). Die
  Schrittzahl steckt nur beim **Einlesen** (`step = round(timestamp * schritte)`,
  `from_csv`) und im XML als `Line index="0..n"`. Erst die **Konvertierung** nach
  Schism/Renoise hat ein Längenfeld („andere Längen", wie `length` am Desktop).

---

## Step 1: `core/src/scanmask.rs` — Maske je Spalte (TDD)

- Goal: hardwarefreie Maske mit rrn5-Semantik.
- Files: `core/src/scanmask.rs` (neu), `core/src/lib.rs` (`pub mod scanmask;`)
- API:
  ```rust
  pub enum Aufloesung { Sync, Frei { schritte: usize } }
  pub struct ScanMaske { alpha: Vec<f32>, aufloesung: Aufloesung }
  impl ScanMaske {
      pub fn neu(breite: usize, aufloesung: Aufloesung) -> Self;   // alles 1.0
      pub fn laenge(&self) -> usize;
      pub fn setze(&mut self, x: usize, durch: bool);
      pub fn setze_alle(&mut self, durch: bool);
      pub fn resync(&mut self, schritte: usize);
      pub fn alpha(&self, x: i32, schritte: usize) -> f32;
      pub fn eintrag_fuer_x(&self, x: i32, schritte: usize) -> Option<usize>;
      pub fn abschnitte(&self, schritte: usize) -> Vec<(usize, usize, f32)>; // Runs zum Zeichnen
      pub fn als_keyevent(&self, schritte: usize) -> String;   // "s 0"
      pub fn aus_keyevent(text: &str) -> Self;                  // "space"
  }
  ```
- Test (im Modul, 9 Stück):
  `neue_maske_ist_alles_durch`, `sync_ist_ein_wert_pro_spalte`,
  `ausserhalb_der_maske_ist_gesperrt` (Länge 10, Abfrage 10 ⇒ 0.0),
  `ausserhalb_der_spalten_ist_gesperrt` (Sync **und** Frei),
  `frei_streckt_ueber_die_breite`, `setzen_schaltet_ja_nein`,
  `resync_behaelt_werte_und_sperrt_den_rest`,
  `keyevent_rundlauf` (`x;key;alpha;xkeyk`, 0/1),
  `keyevent_liest_0_als_aus`
- Verify: `nix-shell --run "cargo test --manifest-path core/Cargo.toml scanmask"`
  → `9 passed; 0 failed`; danach ganze Crate → `≥ 106 passed; 0 failed`
- Depends on: — · Rollback: `git revert` (nur neue Datei + eine Zeile in lib.rs)

## Step 2: `core/src/takte.rs` — Taktdauer-Fader (TDD)

- Goal: Ein Sweep = so viele Takte, wie der Fader sagt (1/36 … 36), mit ÷2 ×2 ÷3 ×3.
- Files: `core/src/takte.rs` (neu), `core/src/lib.rs`
- API:
  ```rust
  pub const VORGABE: f64 = 0.25;                 // 1/4 Takt = ein Beat (rrn5)
  pub const MIN: f64 = 1.0 / 36.0;
  pub const MAX: f64 = 36.0;
  pub fn teiler(takte: f64, faktor: Teiler) -> f64;   // Teiler::Halbieren/Zweimal/Dritteln/Dreimal
  pub fn klemmen(takte: f64) -> f64;                  // in [MIN, MAX]
  pub fn text(takte: f64) -> String;                  // "1/4 Takt", "3 Takte", "1/36 Takt"
  pub fn sweep_sekunden(takte: f64, bpm: f64) -> f64; // Takte * 4 * 60 / bpm
  ```
- Test (7 Stück):
  `teiler_halbiert_und_verdoppelt`, `teiler_drittelt_und_verdreifacht`,
  `klemmen_bergenz_und_tiefgrenze`, `vorgabe_ist_ein_viertel_takt`,
  `text_schreibtfrac_und_ganzzahlig` („1/4 Takt“, „3 Takte“, „1/36 Takt“),
  `sweep_dauert_einen_beat` (1/4 Takt, 120 BPM ⇒ 0,5 s),
  `sweep_kann_kurz_und_lang_werden` (4 Takte, 120 BPM ⇒ 8 s; 1/36 ⇒ 1/18 s;
  36 Takte ⇒ 72 s),
  `unbrauchbare_werte_werden_vorgabe` (NaN, Tempo 0, negatives Tempo)
- Verify: `nix-shell --run "cargo test --manifest-path core/Cargo.toml takte"`
  → `7 passed; 0 failed`; ganze Crate → `≥ 113 passed; 0 failed`
- Depends on: —

## Step 3: Bild-Tab — Maske auf dem Bild, Zoom, bedienbar im Querformat

- Goal: Maske liegt als Leiste auf dem Bild und wird dort getippt; Bildzoom live;
  im Querformat bleiben die Regler erreichbar; 16-Schnipsel-Leiste und Fader
  „Noten/Sek" fallen weg.
- Files: `android/src/image.rs`, `android/src/layout.rs` (Bild im Kombi-Tab)
- Änderungen in `image.rs`:
  - Neu: `maske: ScanMaske`, `maske_an: bool` (= rrn5 `b`), `schritte_fest: bool`,
    `schritte_fest_wert: u16`, `takte: f64`, `bild_zoom: f32`.
  - Entfallen: `maske: [bool; 16]` samt Balken-Schleife, der `scan_max_noten`-Regler
    (fest `const MAX_NOTEN_PRO_SEK: u32 = 8;`).
  - `schrittzahl(platz_pt: f32) -> u16` = `klemme(rund(platz_pt * bild_zoom))`,
    bei `schritte_fest` der eingefrorene Wert. Mindestens 16, höchstens 4096.
  - `scan_schritt()`: läuft über **Schritte**, nicht über Kameraspalten; je Schritt
    `x = schritt * kamera_breite / schritte`. Spaltendauer aus
    `sweep_sekunden(takte, bpm)` / Schritte; `alpha` aus der Maske, Treffer mit
    `scale_rgb`, Lautstärke `vol * alpha`.
  - Leiste 18 pt oben im Bildrechteck: **ein Streifen je Schritt**, grün (durch) /
    dunkel (gesperrt); benachbarte gleiche Werte als ein Rechteck
    (`ScanMaske::abschnitte`), damit auch 872 Schritte flüssig bleiben.
  - Zeiger in der Leiste → `setze(schritt, !durch)`, Ziehen malt weiter; Zeiger
    darunter macht unverändert die Farbauswahl.
  - Steuerung: „Maske an/aus", „Schritte festhalten" (zeigt die Zahl),
    „Zoom %" mit „Breite"/„Höhe", Taktdauer-Fader mit „÷2 ×2 ÷3 ×3".
  - Bildgröße: `min(zoom * platz, 0.55 * flaeche)`, Seitenverhältnis beachtet,
    horizontal zentriert; ganzer Inhalt in `ScrollArea::vertical()`
    (`id_salt("bild_inhalt")`).
- Test: die Rechnung steckt in Step 1/2 (core). UI nur Gerätetest.
- Verify: `nix-shell --run "bash android/build-apk.sh --install"`; dann
  Hoch- **und** Querformat: `adb shell screencap -p /sdcard/b.png && adb pull …`,
  Bild + darunter Masken-/Zoom-/Taktregler ohne Scrollen sichtbar;
  `adb logcat -d | grep bild` nach einem Tippen in die Leiste zeigt keinen
  Farbauswahl-Log, der Sweep sendet nur in erlaubten Spalten Noten.
- Depends on: Step 1, Step 2

## Step 3b: Mehrfinger-Gesten (zwei Finger Zoom, drei Finger verschieben)

- Goal: Bild mit zwei Fingern auf-/zuziehen (Zoom 20 … 400 %) und mit drei Fingern
  im Bild verschieben; beides ohne die Maske oder die Farbauswahl zu stören.
- Files: `android/src/touch.rs` (neu), `android/src/image.rs`
- Warum eigens: egui liefert auf Android **kein** `Event::Zoom`; winit reicht je
  Finger ein `egui::Event::Touch` durch (`egui-winit/src/lib.rs:878`), also wird
  Pinch und Pan aus den Touch-Punkten selbst gerechnet.
- API (`touch.rs`, ohne egui-Abhängigkeit im Kern, damit `cargo test` läuft):
  ```rust
  pub struct Punkt { pub x: f32, pub y: f32 }
  pub enum Phase { Start, Zug, Ende }
  pub struct Geste { pub zoom: f32, pub pan: (f32, f32) }   // Zoom-Faktor je Ereignis
  pub struct Gesten { punkte: Vec<(u64, Punkt)>, abstand: f32, mitte: Punkt }
  impl Gesten {
      pub fn aktualisiere(&mut self, beruehrung: (u64, Phase, Punkt)) -> Option<Geste>;
      pub fn finger(&self) -> usize;
  }
  ```
  Nur bei **zwei** Fingern wird gezoomt (Abstandsverhältnis zur Vorstufe) **und**
  verschoben (Mittelpunktweg); bei **drei** Fingern nur verschoben; bei einem
  Finger nichts — der bleibt Farbauswahl bzw. Maske.
- Test (`touch.rs`, 6 Stück): `zwei_finger_auseinander_zoomt_rein`,
  `zwei_finger_zusammen_zoomt_raus`, `drei_finger_verschieben_ohne_zoom`,
  `ein_finger_zoomen_nicht`, `zweiter_finger_beginnt_die_geste`,
  `finger_weg_laesst_keine_geste`.
- Bild: `bild_zoom` 0,2 … 4,0; der Bildbereich bekommt einen Versatz
  (`bild_versatz: (f32, f32)`), auf die Box beschnitten; das Kamerabild bleibt in
  der Mitte, wenn nichts verschoben ist.
- Verify: Build; Gerätetest mit `adb shell input` gelingt nur mit einem echten
  Mehrfinger-Event, daher als Beleg `adb logcat`: `gesten: 2 Finger, Zoom 1.42`
- Depends on: Step 3

## Step 3c: Maske nimmt nur Start- und Endtaste, Halten = nächster Schritt

- Goal: Der Wisch legt **nur die Tasten an, bei denen er startet und aufhört**,
  nicht alle dazwischen. Eine gehaltene Taste plus eine zweite → die zweite ist
  der **nächste** Schritt. Die Schritte kommen hintereinander in das Raster
  (Schrittzähler), statt sie profilgewichtet zu verteilen.
- Files: `android/src/maske.rs`, `core/src/maske.rs` (bleibt für den Desktop),
  `android/src/touch.rs` (Mehrfinger-Druckstart)
- Änderungen:
  - `MaskePanel.naechster_schritt: u16` als Schrittzähler; jeder Tastendruck
    belegt den nächsten Schritt (Umlauf bei `pattern.steps_per_cycle()`), der
    Status zeigt Schritt und Wert.
  - `wisch_auswerten` → `presse_auswerten`: nur der erste und der letzte
    getroffene Knopf zählen; leerer Wisch → nichts.
  - Mehrfinger: jeder neue `TouchId` mit `Phase::Start` im Feld ist ein weiterer
    Tastendruck (Reihenfolge = Reihenfolge des Druckens).
  - Die Profil-Anzeige („Letzter Wisch: n Tasten, linear") entfällt hier, weil es
    keine Zwischenschritte mehr gibt; `core::maske` bleibt für den Desktop
    unangetastet (dort wird es nicht mehr referenziert — Umstellung dort später).
- Test: keine neue Kernlogik (das Raster macht `Pattern::insert`), deshalb
  Gerätetest: zwei Tasten nacheinander antippen → Schritte 0 und 1 belegt.
- Verify: Build; Gerätetest: Tasten 5 und 9 nacheinander → Log
  `maske: druck taste 5 -> schritt 1, taste 9 -> schritt 2`
- Depends on: Step 3b

## Step 4: Patternlänge = X-Auflösung, Konvertierung mit anderer Länge

- Goal: Die Schrittzahl kommt aus der Bildbreite; der CSV bleibt normiert 0..1, und
  bei der Konvertierung nach Schism/Renoise bleibt eine **andere Länge** wählbar.
- **Zusätzlich (Nutzerwunsch 2026-10-08): Start- und Endpunkt im Pattern wählbar** —
  im Bild-Tab „Pattern von…bis…" mit „ganzes Bild"; `Pattern::mit_abschnitt(von, bis, x_auflosung)`
  macht daraus die Patternlänge, `abschnitt_step` rechnet jede X-Spalte hinein.
- Files: `core/src/grid.rs`, `android/src/lib.rs`, `android/src/maske.rs`, `android/src/image.rs`
- Test (TDD, `core/src/grid.rs`):
  - `schritte_aus_breite(317.0, 1.0) == 317`, `(872.0, 1.0) == 872`, Untergrenze 16,
    Obergrenze 4096
  - `csv_bleibt_normiert` — dieselben Noten in einem 16- und einem 40-Schritt-Pattern
    liefern **denselben** CSV (Zeitstempel 0..1)
  - `csv_rueckrechnung_quantisiert_die_auflosung` — `from_csv(t, 40)` macht aus
    `0.025` Schritt 1, `from_csv(t, 16)` daraus Schritt 0
  - `xml_indiziert_je_schritt` — `to_renoise_xml` bei 40 Schritten endet auf
    `Line index="39"`; dieselben Noten mit Länge 16 enden auf `index="15"`
- Änderungen:
  - `core`: `pub fn schritte_aus_breite(breite_pt: f32, zoom: f32) -> u16`
  - `JamApp.schritte: u16` (Vorgabe 16). Bild-Tab schreibt die berechnete Zahl
    hinein, solange „Schritte festhalten" nicht gesetzt ist; Rhythmus-Tab zeigt sie
    und blättert bei mehr als 32 Schritten in 8er-Blöcken weiter.
  - Tagebuch/Maske: CSV wie bisher (normiert), XML mit `steps_per_cycle = self.schritte`.
  - Neue Knöpfe „als Renoise-XML" und „als Schism" mit einem **Längenfeld**
    (Vorgabe = aktuelle Schrittzahl) — wie die `length`-Frage am Desktop.
- Verify: `nix-shell --run "cargo test --manifest-path core/Cargo.toml grid"`
  → neue Tests grün; Build; Gerätetest: 317-Schritt-Pattern exportieren → CSV
  unverändert normiert, XML mit `index="0..316"`; Konvertierung mit Länge 16
  liefert 16 Zeilen.
- Depends on: Step 3

## Step 5: Eine IP-Eingabe für OSC und Tempo

- Goal: Host/Port stehen einmal im OSC-Tab (heute zweimal tippen).
- Files: `android/src/mic.rs`, `android/src/lib.rs`
- Änderungen: `mic.rs` verliert `host`/`port`, `fn target()` und die beiden
  `text_edit_singleline`; dafür `pub fn set_target(&mut self, host: &str, port: &str)`,
  und „Übernehmen" meldet das Ziel im Status. `view_mic` ruft
  `self.mic.set_target(&self.osc.host, &self.osc.port)`; der Kommentar „das Ziel
  stellt der Tempo-Tab selbst ein" entfällt.
- Verify: Build; IP einmal im OSC-Tab setzen, Tempo-Tab zeigt dieselbe IP,
  „Übernehmen" loggt `BPM … -> host:port` (`adb logcat -d | grep jamandroid::mic`)
- Depends on: —

## Step 6: Eine BPM für Tempo, Takt-Scan, Rendern und Tagebuch

- Goal: Überall dieselbe Zahl; bei aktivem Link-Follow gewinnt das Session-Tempo.
- Files: `core/src/session.rs`, `android/src/lib.rs`, `android/src/rhythm.rs`
- Test (TDD): `tempo_gesamt(folgen: bool, session_bpm: f64, eigenes_bpm: f32) -> f32`
  — `follow_genutzt`, `ohne_follow_eigenes_tempo`, `session_ungueltig`
- Änderungen: `JamApp.tempo_bpm: f32` (Vorgabe 128) mit Slider im OSC-Tab;
  `fn tempo_aktuell(&mut self) -> f32` über `tempo_gesamt`; Bild-Tab setzt das
  Tempo **immer** (nicht nur bei Follow); `rhythm::set_tempo` setzt `render_bpm`;
  Tagebuch/Überblick nehmen `tempo_aktuell()` statt `mic.bpm()`; „Übernehmen" im
  Tempo-Tab schreibt in `JamApp.tempo_bpm`.
- Verify: `nix-shell --run "cargo test --manifest-path core/Cargo.toml session"`
  → 3 neue Tests grün; Build; Gerätetest: Tempo messen und übernehmen, danach im
  Bild-Tab (Takt-Modus) und im Rhythmus-Render derselbe Wert
- Depends on: Step 5

## Step 7: KeyEventRender.txt-Import (vom Nutzer zurückgestellt)

Der Kern kann ab Step 1 serialisieren und lesen (`als_keyevent`/`aus_keyevent`,
getestet). Offen bleiben Dateipfade im App-Ordner, die Knöpfe „Laden"/„Sichern" und
die Abfrage von „Width?" und „Start?" aus dem Recorder.

---

## Reihenfolge und Risiko

1, 2 sind hardwarefrei und sofort testbar. 3 ist der riskanteste Schritt (Maske
neben Farbauswahl auf demselben Bild, Querformat). 4 ändert Raster und Export.
5, 6 sind mechanisch. Nach jedem Schritt: `cargo test` (core), `cargo fmt --check`,
für 3–6 zusätzlich Build + Gerätetest.