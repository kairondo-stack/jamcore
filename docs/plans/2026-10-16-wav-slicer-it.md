# Plan 2026-10-16: WAV-Slicer + IT-Export (App, später Desktop)

Stand: 2026-10-16 · Nutzerwunsch: im Handy-Recorder ein WAV wählen, automatisch
(nach **Transienten/Energie**) und von Hand slicen, die Slices als **.it-File**
(Pattern leer) sichern und **Slices im Rhythmus-Raster setzen**. WAV kommt per
Datei-Browser **und** aus einer Aufnahme in der App. Später: einzelne Patterns
ins .it-File aufnehmen; danach dasselbe im Desktop-Paket.

## Entscheidungen

- Auto-Slicen **umschaltbar**: „gleichmäßig" (Anzahl → gleiche Längen) **oder**
  „Transienten/Energie" (Onset-Erkennung über den Energie-Fluss mit adaptiver
  Schwelle, auf die Ziel-Anzahl begrenzt). Hand-Schnitte danach frei setzbar.
- .it-File: **Slices als Samples/Instrumente, Pattern leer** (Container);
  der Writer wird so gebaut, dass Patterns später nachgereicht werden können.
- Raster: Slices sind im Rhythmus-Tab als Instrumente wählbar (wie Modul-Samples).
- Desktop kommt nach der App (gleiche Kern-Funktionen).

## Step 1: Kern – WAV lesen und slicen (TDD)

- Goal: **gängige Audioformate** (wav, mp3, ogg/vorbis, flac, m4a/aac, aiff —
  via `symphonia`, reines Rust, läuft auf Desktop **und** Android) in f32-Mono
  laden; Hüllkurve, **zwei Schnittmodi** (gleichmäßig / Transienten+Energie),
  Slice-Liste (Start/Ende in Samples), Hand-Bearbeitung, Ziel-Anzahl und
  Empfindlichkeit.
- Files: `core/src/audio.rs` (neu, Laden), `core/src/slice.rs` (neu),
  `core/src/lib.rs`, `core/Cargo.toml` (`symphonia`).
- API-Skizze: `pub enum Schnittmodus { Gleichmaessig, Transienten }`,
  `pub fn schnitte(proben: &[f32], rate: u32, modus: Schnittmodus, anzahl: usize,
  empfindlichkeit: f32) -> Vec<Slice>`, dazu `verschiebe`, `loesche`, `setze`.
- Test: `gleichmaessig_teilt_in_n` (4 Slices → 4 gleich lange),
  `transienten_findet_klicks` (4 Klicks in Stille → 4 Slices),
  `transienten_auf_anzahl_begrenzt` (8 Klicks, Ziel 4 → 4),
  `leises_signal_bleibt_ein_slice` (beide Modi),
  `anzahl_wird_geklemmt`, `hand_schnitt_sortiert`,
  `schnitt_loeschen_und_verschieben`, `stereo_wird_gemischt`.
- Verify: `cargo test --manifest-path core/Cargo.toml slice` → grün.
- Depends on: —

## Step 2: Kern – IT-Writer (Slices + leeres Pattern) (TDD)

- Goal: .it-Datei schreiben: Header, Orders, Sample-Köpfe + PCM-Daten,
  ein leeres Pattern; API so, dass Patterns später ergänzt werden können.
- Files: `core/src/itwrite.rs` (neu), `core/src/lib.rs`.
- Test: `kopf_und_anzahl_stimmen` (eigener Mini-Parser im Test liest zurück),
  `sample_daten_und_laengen`, `leeres_pattern_hat_64_zeilen`,
  `namen_werden_geklemmt`.
- Verify: `cargo test --manifest-path core/Cargo.toml itwrite` → grün **und**
  Gegenprobe mit libopenmpt: `it_live` (Root-Crate, linkt libopenmpt) bekommt
  einen Test, der ein erzeugtes .it lädt und `num_samples` prüft:
  `cargo test --test itwrite_openmpt` bzw. ein `#[test]` in `src/main.rs`.
- Depends on: Step 1 (Slices als Eingabe)

## Step 3: App – WAV wählen und aufnehmen

- Goal: Datei-Browser listet auch `.wav`; ein Aufnahme-Knopf nimmt über das
  Mikrofon auf und legt die Datei im App-Ordner `recordings/` ab.
- Files: `core/src/modulebrowser.rs` (Endungen um Audio erweitern),
  `android/src/browser.rs` (Anzeige/Filter), `android/src/wavrec.rs` (neu,
  Aufnahme → WAV, Vorbild `android/src/mic.rs` + `core/src/wav.rs`),
  `android/src/lib.rs` (Knopf).
- Test: `core`-Tests für die Filter-Erweiterung; Aufnahme am Gerät.
- Verify: Handy: aufnehmen → Datei erscheint im Browser und ist abspielbar.
- Depends on: Step 1

## Step 4: App – Slicer-Ansicht und Raster-Anbindung

- Goal: Wellenform mit Schnittmarken; „Auto (Transienten)" mit Anzahl-Regler,
  Tippen setzt Schnitte, Ziehen verschiebt, langes Tippen löscht; „als .it
  sichern"; Slices als Instrumente im Rhythmus-Raster wählbar.
- Files: `android/src/slicer.rs` (neu), `android/src/rhythm.rs` (Instrumenten-
  Auswahl um Slices erweitern), `android/src/lib.rs` (Tab).
- Test: Kern-Tests aus Step 1/2; Sichtprüfung am Handy.
- Verify: Handy: WAV slicen → .it sichern → im Player laden (Samples sichtbar);
  Slice im Raster setzen → klingt.
- Depends on: Step 2, Step 3

## Step 5: Desktop-Paket (später, gleiche Kern-Funktionen)

- Goal: Slicer + IT-Export in `pattern_recorder` (und `rrn5`, wenn sinnvoll).
- Files: `pattern_recorder/src/*` (neu: `slicer.rs`), `~/jam-nix` (Sync/Hashes).
- Verify: Desktop baut, WAV slicen → .it → mit `it_live` abspielen.
- Depends on: Step 4

## Rollback

Additiv: neue Kern-Module, neuer Tab/neue Ansicht; keine bestehenden Pfade
werden umgebaut (außer der Endungsliste im Browser).
