# Bild-Tab: Scan-Linie/-Bereich per Knopf verschieben (Punkt 2)

Nutzerklärung (2026-10-21): „Verschieben-Button für Linie oder Bereich, also
Scan-Bereich.“ Gemeint ist der **Bild/Scan-Tab** (`android/src/image.rs`):
die Scan-Linie bzw. den x-/y-Bereich **per Knopf** verschieben (nicht nur per
Finger/Fader), ohne die Breite zu ändern.

## Verhalten
- Je Achse (x und y, dort wo ein Bereich existiert) zwei Knöpfe:
  **◀** und **▶**, Schrittweite **1 %**; mit gedrückter Umschalttaste/„×10“-
  Knopf **10 %**.
- **Linie**: verschiebt die Scan-Linie (`scan_x`/`scan_y` bzw. die
  Linien-Position) und klemmt auf `0.0 … 1.0`.
- **Bereich** (x0/x1, y0/y1 bzw. Kreis/Ausschnitt): verschiebt **beide**
  Grenzen gemeinsam, Breite bleibt erhalten; an den Rändern wird gestoppt
  (keine Stauchung).
- Statuszeile/Log: `bild: Scan-Linie x 0.42 -> 0.43` bzw.
  `bild: Scan-Bereich x 0.20-0.60 -> 0.21-0.61`.
- Die Knöpfe wirken auch, während der Scan läuft (Live-Verschieben), und
  werden mit „vom Scan übernehmen“/Fadern synchron gehalten.

## Umsetzung
- Reine Kernfunktion mit Tests (z. B. in `core/src/scan.rs` oder als
  Hilfsfunktion in `image.rs`):
  ```rust
  /// Verschiebt [von, bis] um `delta`, behält die Breite, klemmt an 0..1.
  pub fn verschiebe_bereich(von: f32, bis: f32, delta: f32) -> (f32, f32);
  ```
  Tests: Mitte, linke/rechte Wand (Breite bleibt), delta 0, von > bis
  (normalisieren), Bereich = volle Breite (unverschiebbar).
- UI: kleine Zeile unter den x-/y-Fadern: `Verschieben ◀ ▶ ×10`.
- Keine Änderung an der Scan-Rechnung selbst.

## Verifikation
- Kern-Tests grün.
- Gerät: Bild laden, Linie/Bereich per Knopf verschieben → Markierung wandert,
  Fader zeigen den neuen Wert; Log-Zeilen; Screenshots.

## Rollback
Additiv: nur neue Knöpfe + reine Hilfsfunktion.
