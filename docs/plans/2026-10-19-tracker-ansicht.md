# Plan 2026-10-19: Tracker-Ansicht im Rhythmus-Tab (64 Kanäle)

Stand: 2026-10-19 · Nutzerwunsch: Im **Rhythmus-Tab** eine **Tracker-Ansicht**
wie im Impulse Tracker / in der App „ModGuru" — Zeilen = Schritte, Spalten =
**64 Kanäle**, Zelle = `Note Instrument` (+ Volume/Effekt), mit dem bestehenden
Pad-Raster als zweiter Ansicht. Damit wird das Raster **polyphon**.

## Entscheidungen
- **Kanäle**: 64 (0…63), echt im Datenmodell (`Note.channel`), Vorgabe 0.
- **Zelle**: Note (Notenname), Instrument, Volume; Effektspalte zunächst nur
  als Feld (`effekt: u8`, `parameter: u8`) vorgesehen, nicht ausgebaut.
- **Ansichten**: Umschalter „Raster ⇄ Tracker" im Rhythmus-Tab; Raster bleibt
  unverändert (bestehende Funktionen: Euklid, Laub, Maske, CSV/XML).
- **Klang**: alle Kanäle klingen (polyphon) — Modul-Samples über die vorhandene
  `ModulQuelle`, Slices/Tongenerator über den Mixer.

## Step 1: Kern – Kanäle im Raster (TDD)
- Goal: `core/src/grid.rs`: `Note` um `channel: u8` (0…63, geklemmt) erweitern,
  Helfer `nach_kanal`/`kanal_spalte`, Zellentext (`C-5 01 v64`), Tests.
- Files: `core/src/grid.rs`, `core/src/lib.rs` (ggf. `pub mod tracker;`),
  `core/src/tracker.rs` (neu: Zellentext/Spaltenlogik).
- Test: `kanal_wird_geklemmt`, `zelle_text_note_instrument`, `leere_zelle`,
  `sortierung_nach_schritt_und_kanal`, `bestehende_tests_bleiben`.
- Verify: `cargo test --manifest-path core/Cargo.toml tracker` + volle Suite.

## Step 2: App – Tracker-Ansicht (Umschalter im Rhythmus-Tab)
- Goal: `android/src/tracker.rs`: monospace Zeilen (Schritt, 64 Kanal-Spalten
  horizontal scrollbar), aktuelle Zeile hervorgehoben, Tippen auf eine Zelle
  setzt/ändert Note (Instrument = aktuelle Auswahl), „löschen"; Umschalter
  „Raster/Tracker" in `rhythm.rs`; Wiedergabe polyphon (alle Kanäle).
- Files: `android/src/tracker.rs` (neu), `android/src/rhythm.rs`,
  `android/src/lib.rs` (nur falls nötig).
- Verify: Handy: Tracker öffnen, Zelle setzen, hören (Klangquelle), Raster zeigt
  denselben Stand; Logs.

## Rollback
Additiv: neues Kern-Modul + neue Ansicht; Raster bleibt unverändert.
