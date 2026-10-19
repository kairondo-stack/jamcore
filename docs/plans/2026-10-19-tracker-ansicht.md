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

## Ergebnis (2026-10-20)

- **Kern** (`core/src/tracker.rs`, `grid.rs`): `Note.channel: u8` (0…63),
  `zelle_text` (`C-4 01` / `--- --`), `nach_kanal`, `setzen`, `loeschen`,
  `sortiere`; Kern-Suite **381 Tests grün** (davon 9 neu für Kanäle/Zellen).
  Bewusst: `instrument` bleibt `u8`, `note_name` bleibt Renoise-Schreibweise
  (60 = `C-4`); `Pattern::insert` (Rampe/Euklid/Laub/CSV/XML) ersetzt weiter
  alle Noten eines Schritts — zellenweise arbeiten nur `setzen`/`loeschen`.
- **App** (`android/src/tracker.rs` + `rhythm.rs`): Umschalter
  „Ansicht [Raster] [Tracker]", 64 Kanal-Spalten (horizontal) und bis zu 512
  Zeilen (vertikal) in einem `ScrollArea`, Zellwahl per Tipp, Pads als
  Tastatur, „Zelle löschen", Schritt-/Kanal-Schalter, Wiedergabe polyphon
  (alle Noten, mehrere je Schritt).
- **Belegt am Emulator** (2026-10-20, 1080×2400, APK arm64/x86_64):
  - Umschalten loggt `tracker: Ansicht Tracker`; Ansicht zeigt
    `Step | Ch1 … Ch64` mit `--- --`-Zellen.
  - Tipp auf Zeile 452, Kanal 3: `tracker: Zelle Schritt 452 Kanal 3 gewählt (--- --)`
    (die Ansicht folgt dem laufenden Schritt); Kopfzeile `Kanal 3 / 64`.
  - Screenshots: `/tmp/opencode/laub_video/31_tracker.png`, `32_zelle.png`.
- Offen: Noteneingabe über Pads am Gerät durchspielen (Code-Pfad
  `setzen` + `note_klingen` gebaut, Log `tracker: Zelle … <- C-5 01`).
- **Noteneingabe belegt** (2026-10-20): Pad `C-3` gedrückt →
  `tracker: Zelle Schritt 452 Kanal 3 <- C-3 00`, Kopfzeile
  „Zelle Schritt 452 · Kanal 3 · C-3 00", Status „1 Noten im …";
  Screenshots `33_scroll.png` (Pads unter dem Tracker), `34_note.png`.
- Hinweis Bedienung: Der Tracker hat eine eigene Scrollfläche; um an die Pads
  zu kommen, die **Seite** außerhalb des Trackers ziehen (z. B. in der
  Werkzeugzeile) — sonst scrollt der Tracker selbst.
