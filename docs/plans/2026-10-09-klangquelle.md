# Klangquelle für das gerenderte Notenmaterial: Modul-Samples + Tongenerator

Datum: 2026-10-09 · core 165 Tests grün · **Urheberrechts-Hinweis beachten** (unten).

## Ziel

Das gerenderte Notenmaterial (Rhythmus-„Rendern", „Slot spielen", später Scan/Maske)
soll hörbar sein mit

1. den **Samples/Instrumenten des geladenen Moduls** — über die libopenmpt-ext-
   Schnittstelle `interactive.play_note` (libopenmpt 0.8.7; die C- und C++-APIs
   geben **keine** Sample-Daten heraus, `play_note` ist der einzige Weg), solange
   ein Modul im Player läuft, sonst
2. einem **Tongenerator** mit den gängigen Wellenformen (Sinus, Dreieck, Sägezahn,
   Rechteck, Rauschen).

**Copyright-Hinweis (vom Nutzer ausdrücklich gefordert):** Modul-Samples und
-instrumente sind urheberrechtlich geschützt. Sie dürfen hier zum Mitspielen
verwendet werden; eine **Wiederverwendung** (Aufnahme exportieren, in eigenen
Stücken verwenden, weitergeben) ist vorher rechtlich zu prüfen. Der Hinweis steht
in der UI bei der Klangquellen-Wahl und in dieser Doku.

## Step 1: Wellenformen im Kern (TDD)

- Files: `core/src/klang.rs`
- API:
  ```rust
  pub enum Welle { Sinus, Dreieck, Saegezahn, Rechteck, Rauschen }
  pub fn puffer_welle(freq: f32, rate: u32, laenge_ms: u32, welle: Welle) -> Vec<i16>
  ```
  Sinus = bestehendes `sinus_puffer` (bleibt), Ein-/Ausblenden wie dort.
- Tests: Rechteck hat nur ±Amplitude, Sägezahn steigt monoton je Periode,
  Dreieck ist symmetrisch, Rauschen ist nicht konstant, Sinus stimmt mit
  `sinus_puffer` überein, Länge/Rate stimmen.

## Step 2: Modul-Samples anspielen (core/openmpt.rs)

- Files: `core/src/openmpt.rs`
- Modul über die **ext**-API erzeugen: `openmpt_module_ext_create_from_memory`,
  `openmpt_module_ext_get_module`, `openmpt_module_ext_get_interface("interactive")`.
- `interactive`-Struct exakt in Header-Reihenfolge deklarieren (16 Felder;
  `play_note` Index 14, `stop_note` Index 15), sonst zeigt der Zeiger ins Leere.
- `Mod` hält künftig `ext: *mut openmpt_module_ext`, `ptr` (Basismodul) und
  `boxed_interactive: Box<Interactive>`; `Drop` ruft `openmpt_module_ext_destroy`.
- Neue Methoden:
  ```rust
  pub fn note_spielen(&self, instrument: i32, note: i32, volume: f32) -> Option<i32>
  pub fn note_stoppen(&self, channel: i32)
  ```
  Note auf 0..=119 klemmen (libopenmpt), Instrument 0..n-1 (Desktop-Recorder nutzt
  Instrument 30; auf Module mit weniger Instrumenten auf 0 fallen).
- Test: FFI nicht ohne Moduldatei prüfbar → Build- und Gerätetest; die
  Modul-Erzeugung selbst deckt der bestehende Player-Pfad ab.

## Step 3: Android — Klangquelle wählen

- Files: `android/src/player.rs` (Handgriff `ModulQuelle`), `android/src/klang.rs`
  (`spielen_welle`), `android/src/rhythm.rs` (Auswahl + Verdrahtung), `android/src/lib.rs`
- `Player::modul_quelle() -> ModulQuelle` (klonbar, `Arc` auf das geteilte Modul);
  `ModulQuelle::note_spielen(note, volume)` sperrt kurz und ruft den Kern.
- `RhythmPanel`: Felder `klangquelle: Klangquelle` (Modul/Tongenerator) und
  `welle: Welle`; Auswahlzeile „Klang: [Modul] [Welle: Sinus/Dreieck/…]" mit dem
  **Copyright-Hinweis** als kleinem grauen Text.
- `rendern_starten`/`slot_spielen`: bei „Modul" und laufendem Player die Samples
  anspielen, sonst den Tongenerator; Statuszeile nennt die benutzte Quelle.
- `JamApp` reicht die `ModulQuelle` jedes Frame ans Panel (`set_modul_quelle`).

## Step 4: Verifikation

- `cargo test` core (Wellenformen), fmt, `bash android/build-apk.sh`.
- Gerätetest: Modul starten → „Rendern" mit Klangquelle „Modul" (Log
  `modul: Note 60 -> Kanal N`), dann „Welle: Rechteck" und hören/Log
  `klang: Welle Rechteck`; Copyright-Hinweis im Screenshot.
