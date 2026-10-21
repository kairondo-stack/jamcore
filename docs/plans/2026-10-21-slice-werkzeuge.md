# Slice-Tab: Schneid-Werkzeuge + Umkehren (Reverse)

Nutzerwunsch (2026-10-21): „Schneid-Werkzeuge, Reverse für Ausgewähltes/Bereiche im Slice-Tab.“

## Ausgangslage
`android/src/slicer.rs` kann heute: Audio öffnen/aufnehmen, Auto-Schnitte
(gleichmäßig/transient), Schnitte von Hand setzen/verschieben/löschen,
Slices anhören, Slices → Raster/.it exportieren, **Sample-Tausch** je
Instrument (`setze_sample`, `sample_fuer_instrument`).

Es fehlt: **Bearbeiten des Audios selbst** (Umkehren, Zuschneiden, Bereich
löschen, Ein-/Ausblenden, Normalisieren) und eine **Bereichsauswahl**.

## Kern (TDD, `core/src/audioedit.rs` neu)
Reine Funktionen auf `&mut [f32]` (oder i16), alle mit Tests:

```rust
/// Kehrt die Proben im Bereich [von, bis) um.
pub fn umkehren(pcm: &mut [f32], von: usize, bis: usize);

/// Entfernt [von, bis) und liefert die neue Länge; `schnitte` (Sample-Indizes)
/// werden mitverschoben/gekürzt.
pub fn bereich_loeschen(pcm: &mut Vec<f32>, von: usize, bis: usize);

/// Behält nur [von, bis) (Zuschneiden).
pub fn zuschneiden(pcm: &mut Vec<f32>, von: usize, bis: usize);

/// Lineare Ein-/Ausblendung über `dauer` Proben.
pub fn einblenden(pcm: &mut [f32], von: usize, bis: usize, dauer: usize);
pub fn ausblenden(pcm: &mut [f32], von: usize, bis: usize, dauer: usize);

/// Hebt den Bereich auf den Spitzenwert 1.0 (0.0 bleibt 0.0).
pub fn normalisieren(pcm: &mut [f32], von: usize, bis: usize);
```
Tests: exakte Sequenzen (z. B. `[1,2,3,4]` umkehren → `[4,3,2,1]`), Grenzen
(0, len, von==bis, von>bis → unverändert), Schnitte-Verschiebung, Normalisieren
mit Stillbereich.

## App (`android/src/slicer.rs`)
1. **Bereichsauswahl** in der Wellenform: Ziehen mit dem Finger setzt
   `von`/`bis` (in Proben), sichtbar als farbiger Balken; Anzeige
   „Bereich 1.23–2.45 s (1.22 s)“; Knopf „Auswahl aufheben“.
   Ohne Auswahl gilt der **gewählte Slice** als Bereich.
2. **Werkzeug-Zeile** (nur bei geladenem Audio):
   - **Umkehren** (Reverse) — Bereich oder Slice; Log
     `slicer: Umkehren 1.23-2.45 s (Slice 3)`.
   - **Zuschneiden** (nur Bereich behalten)
   - **Bereich löschen**
   - **Einblenden / Ausblenden** (10 ms … 1 s, Vorgabe 50 ms)
   - **Normalisieren**
   - **Rückgängig** (ein Schritt, Snapshot vor jeder Bearbeitung)
3. Nach jeder Bearbeitung: Wellenform neu zeichnen, Schnitte/Raster neu
   berechnen (`raster_aktualisieren`), Slices-Export aktualisieren.
4. Sicherheitsnetz: Bearbeitung nur bei pausierter Wiedergabe; vorher
   `slice_spielen`-Stimmen stoppen (kein Knacksen).

## Verifikation
- `cargo test --manifest-path core/Cargo.toml` (neue Tests grün)
- Gerät: Audio laden (Testdatei), Bereich ziehen, Umkehren → Wellenform
  sichtbar gespiegelt, Zuschneiden/Löschen ändern die Länge, Rückgängig stellt
  her; Logs + Screenshots.

## Rollback
Additiv: neues Kern-Modul + neue Knöpfe; bestehende Schnitt-Logik unverändert.
