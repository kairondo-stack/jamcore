# Ideen-Notiz 2026-10-18: Objekt-/Artenerkennung (neue Session)

Thema: **Erkennen, was auf dem Bild ist** — z. B. im Wald die **Baumart** in
Echtzeit beim Filmen. Lokal (ohne Cloud), auf dem Handy.

## Zwei Wege

1. **Klassische CV (sofort machbar, begrenzt)**
   - Vorstufe: Blatt-/Nadel-/Stamm finden — dafür gibt es die Blob-Erkennung
     aus `core/src/laub.rs` (Farbschwelle + Zusammenhangskomponenten).
   - Merkmale: Blattform (gelappt/ganzrandig/gesägt), Nadelbüschel, Rinde,
     Blattstellung, Farbverlauf.
   - Ergebnis: grobe Gruppen (Nadel/Laub, Formfamilie) — **keine** sichere Art.
2. **On-Device-ML (eigenes Thema)**
   - Kleines Modell (MobileNet/EfficientNet-Klasse, 5–20 MB) als TFLite oder
     ONNX in Rust (`tflitec`, `ort`, `burn`, `candle`), auf Android mit
     NNAPI/GPU-Beschleunigung.
   - Daten/Modelle: PlantNet-/iNaturalist-Linien, Leafsnap-artige
     Blattklassifikatoren; Lizenz und Artenliste (Deutschland ~90 Baumarten)
     vorher prüfen.
   - Echtzeit: alle 2–5 Frames klassifizieren, über ~2 s glätten
     (Mehrheitsentscheid) → stabile Anzeige „Eiche 82 %".

## Wiederverwendbar aus der JamCore-Session (2026-10)

- Kamera (`android/src/camera.rs`, CameraBridge) und Video (`video.rs`,
  MediaCodec) — Frames liegen schon als RGB in Rust.
- `core/src/laub.rs` — Blob-Erkennung, Fläche/Farbe, Sortierung (als Vorstufe).
- `core/src/livescan.rs` — Farb-/Toleranzvergleiche, Paletten.
- `core/src/akkord.rs`, `grid.rs`, `osc.rs` — Ergebnis → Klang/Rhythmus/OSC.
- `docs/2026-10-17-archiv-66-analyse.md` — was aus den alten C++-Versuchen
  schon da ist (Farbmaske, Kanten, Objektverfolgung `fingercups2`).

## Erste Schritte für die neue Session

1. Spike: ein TFLite-/ONNX-Modell auf dem Handy laden und **ein** Bild
   klassifizieren (Latenz messen, NNAPI an/aus).
2. Modell auswählen (Arten-Abdeckung, Lizenz, Größe) — ggf. zwei Kandidaten
   vergleichen.
3. Vorstufe aus `laub.rs` davorschalten (erst Blätter finden, dann klassifizieren)
   und die Glättung über mehrere Frames bauen.
4. Ergebnis anzeigen (Name + Konfidenz) und optional als Klang/Rhythmus nutzen.
