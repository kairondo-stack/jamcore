# Stand 2026-10-18 (Abend) — morgen weiter

Committet + gepusht: `616606a` (Branch `opencode-rework-1-versuch`).

## Heute fertig
- **Slices direkt hörbar** (Ursache: Raster lag nur im RAM und wurde nur bei
  sichtbarem Slicer-Tab gefüllt → Fallback Tongenerator). Fix: Merker
  `slicer-export.txt`, `letzter_export()`, `raster_aus_letztem_export()` beim
  Klick auf „Slices". **Rec** schreibt bevorzugt in den letzten Export und legt
  eine **neue .it mit den Slices** an (am Handy byte-geprüft).
- **Laub-Kern** (`core/src/laub.rs`): Blob-Erkennung + Abstände → Genre-Raster
  (Techno/Groove/Breakbeat/DnB; relativ oder absolut; Raster 96/128/192/256) —
  17 Tests, Kern **345 grün**.
- **Stroboskop-Formel** (Basis auto/1800/3600 + Versatz, Stillstand/BPM-Anzeige),
  **eigener Oszillator-Bereich** (Fader + „vom Scan übernehmen").
- **Archiv /66** ausgewertet (`docs/2026-10-17-archiv-66-analyse.md`) und
  **Idee Artenerkennung** notiert (`docs/2026-10-18-idee-artenerkennung.md`).

## Läuft noch / offen
- **Video-Analyse Kamerafenster**: Hauptagent fertig (kein stabiles Flicker,
  kein Bildlauf, nur Vorschau-Stottern 15–28 fps; Belege in
  `/tmp/opencode/jamcore-video/av/BERICHT.md`). Zwei Helfer laufen noch:
  **Banding/Flicker** (`av2/`) und **wandernde Muster** (`av3/`).
  **Frage an den Nutzer:** Stelle im Video (mm:ss) nennen, wo das rhythmische
  Artefakt sichtbar ist — dann gezielt auswerten.
- **Kamera-Vorschau ist 90° verdreht** (kein Dreh-Code in `CameraBridge.java`/
  `camera.rs`) — Fix einplanen; fürs Laub heißt das: „Weg" liegt auf der y-Achse.
- **Tempo aus Video** (Plan Step 5, `docs/plans/2026-10-18-laub-rhythmus.md`).
- **Laub-Ansicht** in der App (Step 3 des Plans).
- **Desktop-Paket**: `./sync.sh` + Hashes + `nix-build` nachziehen (zuletzt
  gestern gebaut; Kern hat seither `laub.rs` bekommen).
- **Laptop-Deckel**: `services.logind.settings.Login.HandleLidSwitch = "ignore";`
  in `~/nixos-config/configuration.nix` (+ Rebuild) — Nutzer entscheidet.
