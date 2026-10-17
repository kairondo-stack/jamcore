# Plan 2026-10-17: Oszillator-Scan, Anzeige-Umschalter, Harmonie-Mapping

Stand: 2026-10-17 · Nutzerwunsch (abgestimmt):

1. **Anzeige-Umschalter** in Bild & B2 (und Desktop): „grün" wie heute ⇄
   **„echte Farben"**: nur die Auswahl in Originalfarben, Rest dunkel,
   **keine** grüne Umrandung.
2. **Oszillator-Scan**: senkrechte **Linie** im Bild, nur in einem
   **Verschiebe-Modus** beweglich (sonst macht ein Tipp die Farbauswahl); die
   Pixel der Linie (Auswahl+Toleranz, Takt-Maske, Harmonien) klingen als
   **Dauerton** — Ton folgt der Linienposition.
3. **x-Bereich-Modus**: Durchlauf über einen x-Bereich mit **Dauer 1/64 … 4
   Takt**; wählbar **Sweep** ⇄ **alle Spalten gleichzeitig** und **Takt-Maske**
   ⇄ **alle y-Werte**.
4. **Harmonie-Zeit**: die **x-Werte aus dem Kreis** (Harmonik-x-Bänder) werden
   auf **Gesamt-x** (z. B. 128 Schritte) oder **Bereichs-x** (z. B. 36) gemappt;
   der Durchlauf wandert durch diese Bereiche, die Harmonie wechselt mit.
5. **Desktop (rrn5)** bekommt dasselbe.

Arbeitsannahmen (dokumentiert, leicht änderbar): Mapping-Modus („gesamt x" ⇄
„Bereichs-x") als Umschalter neben der Dauer; im Linien-Modus gilt die Harmonie
nach der **Linienposition** (dieselbe Zuordnung, punktuell).

## Step 1: Kern – Linien-/Bereichs-Zeit und Harmonie-Mapping (TDD)

- Goal: Reine, getestete Funktionen: welcher x-Anteil ist bei welcher Zeit „dran"
  (Sweep oder alles), wie werden die Kreis-x-Bänder auf Gesamt- oder Bereichs-x
  gemappt, welche Harmonie gilt an einem x-Anteil, und welche Dauerton-Frequenzen
  entstehen aus den Trefferzeilen.
- Files: `core/src/oszillator.rs` (neu), `core/src/lib.rs`.
- API-Skizze:
  `pub enum Durchlauf { Sweep, Alles }`,
  `pub fn anteil_je_zeit(takte, dauer_takte, durchlauf) -> Option<f64>`
  (None = alles gleichzeitig),
  `pub enum XBasis { Gesamt, Bereich }`,
  `pub fn harm_anteile(basis: XBasis, gesamt_schritte: u16, bereich: (f32,f32)) -> (f32,f32)`,
  `pub fn harmonie_an(x_anteil: f32, baender: &[(f32, f32, usize)]) -> Option<usize>`,
  `pub fn note_fuer_zeile(y_anteil: f32, noten: i32, offset: i32) -> i32`.
- Test: `sweep_am_anfang_und_ende`, `alles_gleichzeitig`, `dauer_wird_geklemmt`
  (1/64…4 Takt), `gesamt_oder_bereich_mapping`, `harmonie_wechselt_mit_x`,
  `note_fuer_zeile_normiert`.
- Verify: `cargo test --manifest-path core/Cargo.toml oszillator` → grün.
- Depends on: —

## Step 2: App – Anzeige-Umschalter, Linie, Bereich

- Goal: In Bild **und** B2: Schalter „Anzeige: grün/echte Farben"; Linien-Modus
  mit Verschiebe-Knopf (Dauerton über `klang::puffer_spielen` bzw. neue
  Dauerton-Stimmen), x-Bereich-Modus mit Dauer-Regler (÷2 ×2 ÷3 ×3, 1/64…4 Takt),
  Sweep/Alle und Maske/Alle-y, Harmonie-Mapping (Gesamt/Bereich).
- Files: `android/src/image.rs`, `android/src/klang.rs` (Dauerton für Linien-Ton),
  `android/src/lib.rs` (nur falls nötig).
- Test: Kern-Tests aus Step 1; Sichtprüfung am Handy.
- Verify: Handy: „echte Farben" zeigt nur die Auswahl; Linie verschieben → Ton
  wandert; Bereich 1/4 Takt → Sweep-Dauer fühlbar; Harmonie wechselt im Verlauf.
- Depends on: Step 1

## Step 3: Desktop – dasselbe in rrn5

- Goal: Anzeige-Umschalter, Linien-Modus (Taste), x-Bereich mit Dauer, Sweep/Alle,
  Maske/Alle-y, Harmonie-Mapping; Ton über den vorhandenen OSC-Weg
  (`/renoise/trigger/note_on`) — rrn5 hat keinen lokalen Klang.
- Files: `rrn5/src/main.rs`, `rrn5/src/oszi.rs` (neu, dünn über
  `jamcore::oszillator`).
- Test: Kern-Tests; Startprobe headless + Xvfb-Smoke wie bisher.
- Verify: `cargo test --manifest-path rrn5/Cargo.toml`, Startprobe ohne Panic.
- Depends on: Step 1

## Rollback

Additiv: neuer Kern-Modul, neue Zeilen/Schalter in Bild/B2 bzw. rrn5; der
Linien-Modus ist per Schalter aus (Standard aus), der Anzeige-Umschalter steht
auf „grün" (wie heute).
