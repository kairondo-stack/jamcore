# Schrittweite im Maske-Tab: Wischen als Stepbreiten-Rampe + Krümmungs-Modulation

## Festgelegt (Nutzer, 2026-10-10)

1. **Maske macht nur Zeitabstände.** Kein Notenschreiben mehr; die Folge wird wie
   `KeyEventRender.txt` auf die X-Auflösung des Bilds gemappt (grüner Balken über dem Bild).
   Noten kommen aus dem Rhythmus-Tab.
2. **Wischen**: Startfeld = `startIndexnum`, Endfeld = `endIndexnum`. Die Beschleunigung im
   Wischen moduliert die Timestamps im Zeitabschnitt Anfang-Index…Ende-Index.
   **Bei gleichmäßigem Wischen ändert sich kein Timestamp.**
3. **Modulation**: Die Geschwindigkeit skaliert die *Krümme*, nicht die Schrittbreite selbst.
4. **Rhythmus-Tab**: Pattern-Länge zwischen 1 und 128 wählbar.
5. **Maske-Anzeige**: **kein 16er-Raster.** Die Leiste darunter ist auf die
   **Patternlänge** gerastert (derselbe Wert wie im Rhythmus-Tab, 1–128);
   darauf liegen **nur die Zeitabstände** der aufgezeichneten Felder.

## C++-Vorlage (`pattern_recorder_202601.cpp`)

- Z. 2470–2480: `notes[i].pitch` = `startIndexnum`, `notes[i+1].pitch` = `endIndexnum`,
  `scl == 1` markiert den Abschnitt.
- Z. 2486: `rschrittweitanz = (e−s)²/(e−s)` → Anzahl Schritte = `e − s` (Vorzeichen beachten).
- Z. 2800–2801: `schrittwa = 1/startIndexnum`, `schrittwe = 1/endIndexnum`
  (Indexnum = Unterteilungen; **kleiner = breiterer Schritt**, daher „43 auf 23").
- Z. 2808/2852: `Indexnumlaenge` je nach `kk`/`kl`/`kr` (Tasten `s`/`a`/`d`, `w` = aus):
  `1 − 1/start − 1/end` | `1 − 1/start` | `1 − 1/end` | `1`.
- Z. 2819–2846 (linke Rampe) und 2862–2889 (rechte):
  `kd = Indexnumlaenge − schrittwe·n`, `sumanz = n(n+1)/2`, `kdtanz = kd/sumanz`,
  Breite Schritt `i` = `schrittwe + (n−i+1)·kdtanz` (bremst) bzw. `schrittwa + i·kdtanz`
  (beschleunigt).
- Z. 2893–2898: `näschritts` = Mittelwert der beiden Rampen (halbe Differenz).
- Z. 2946–2949: `t = totalDuration·xn + startTime`.
- Z. 2955: jeder Schritt wird eine Note mit Pitch `60+i` — der Pitch kodiert den Schritt.

## Schritt 1: Kern — Schrittweiten-Rampe (C++-Port, ohne Modulation)

- Ziel: Die Zeitrechnung des C++ als reine Funktion im Kern, testbar ohne Bildschirm.
- Dateien: `core/src/schrittweite.rs` (neu), `core/src/lib.rs` (Modul eintragen)
- Test: `cargo test --manifest-path core/Cargo.toml`
  - `schrittweite(2) == 0.5`, `schrittweite(4) == 0.25`, Division durch 0 → 0.0
  - `rampen_zeitpunkte`: `start == end` → linear; Summe der Breiten == Abschnittslänge
  - `timestamps`: Folge aus (Zeit, Indexnum) über mehrere Abschnitte, monoton steigend
- Verify: `cargo test --manifest-path core/Cargo.toml` → `N passed; 0 failed`
- Risiko: mittel (Vorzeichen `e − s` bei „43 auf 23"). Rollback: Datei löschen, `lib.rs` zurück.

## Schritt 2: Kern — Krümmungs-Modulation aus der Wischgeschwindigkeit

- Ziel: Beschleunigungsprofil aus (Wischweg, Wischzeit) → Krümmungsfaktor je Schritt.
- Dateien: `core/src/schrittweite.rs`
- Test: 
  - **gleichmäßiges Wischen → alle κ == 1.0** und Timestamps bitgleich zur Rechnung aus Schritt 1
  - langsam-schnell → κ > 1 früh, < 1 spät, Start und Ende des Abschnitts bleiben verankert
  - Division durch 0 (keine Zeit vergangen) → κ = 1.0
- Verify: `cargo test --manifest-path core/Cargo.toml` → neuer Test grün
- Rollback: `git checkout core/src/schrittweite.rs`

## Schritt 3: Kern — Timestamps → Maskenspalten

- Ziel: Aus den berechneten Timestamps die Spaltenzahl des Bild-Sweeps (KeyEventRender-Maßstab).
- Dateien: `core/src/schrittweite.rs`
- Test:
  - linear gleichmäßige Timestamps → Spalten wie bisher (0,3,9,15 bei 4 Tipps über 16 Schritte)
  - alle Spalten < `schritte`, keine doppelten Lücken, monoton
- Verify: `cargo test --manifest-path core/Cargo.toml`
- Regression: der Gerätetest aus `87ba98a` (4 Tipps 0/2,14/7,32/12,53 s → „12 von 16 gesperrt") bleibt gültig

## Schritt 4: Maske-Tab — nur Zeitabstände aufzeichnen

- Ziel: Kein Schreiben ins Rhythmus-Pattern mehr; stattdessen (Zeit, Indexnum, Geschwindigkeitsprofil).
- Dateien: `android/src/maske.rs`
- Test: `cargo test --manifest-path core/Cargo.toml` (unverändert grün) + `cargo fmt --check`
- Verify: `nix-shell --run "bash android/build-apk.sh"` → `BUILD SUCCESSFUL`
- Rollback: `git checkout android/src/maske.rs`

## Schritt 5: Bild-Tab — grüner Balken aus den Timestamps

- Ziel: `ImagePanel` bekommt die Zeitrechnung aus dem Kern statt der linearen Näherung.
- Dateien: `android/src/image.rs`, `android/src/lib.rs`
- Verify: Build + Gerätetest: 4 Tipps → Spalten wie Schritt 3, „N von 16 gesperrt"

## Schritt 6: Maske-Anzeige — Raster nach Patternlänge, nur Zeitabstände

- Ziel: Leiste darunter in der **Patternlänge** des Rhythmus-Tabs gerastert (kein festes
  16er-Raster); darauf die aufgezeichneten Felder als Marken an ihren **Zeitabständen**, dazu
  die Abstände als Zahl.
- Dateien: `android/src/maske.rs`
- Verify: Gerätetest mit Screenshot (Koordinaten nach jedem Screenshot neu messen)

## Schritt 7: Rhythmus-Tab — Länge 1..128

- Ziel: Pattern-Länge wählbar statt fest 16; Raster, Export und Rendern folgen ihr.
- Dateien: `android/src/rhythm.rs`, evtl. `core/src/grid.rs`
- Test: Kern-Test, dass `Pattern::with_steps(n)` für 1..=128 sauber umschachtelt (Schritt % n)
- Verify: `cargo test` + Gerätetest (Länge 3 und 128)

## Schritt 8: Gerätetest end-to-end + Doku

- Ziel: Ganze Kette belegen: Tipp und Wisch → Zeitabstände → grüner Balken → Render.
- Verify: `adb logcat -d | grep jamandroid` zeigt Tipp-/Wisch-/Spaltenmeldungen
- Doku: `docs/funktionen-android-app.md`, `docs/fehlende-funktionen-android.md`