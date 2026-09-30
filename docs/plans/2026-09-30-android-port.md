# Plan: Android-App (Rust) für das Programmpaket

Datum: 2026-09-30
Status: Entwurf zur Review (kein Code begonnen)
Grundlage: bestehende Crates `it_live`, `rrn5`, `pattern_recorder`, `imgsh`, `bundle_udp`

## Ziel

Eine Android-App, die die Funktionen der Desktop-Tools bündelt:
Rhythmus (Aufnahme/CSV/Clipboard), Bild/Video (Farbauswahl → Noten, Live-Scan,
Quintenzirkel), Modul-Player (libopenmpt), OSC/Relay. Bedienung Touch-first,
Hintergrund-fähig (Audio + Relay).

Zusätzlich:
1. **Kombinierbar & Jam-fähig:** Kamera und/oder rrn5-Scan und/oder
   pattern_recorder gleichzeitig nutzbar; Handy und Rechner verbinden sich zu
   einer Session und jammen zusammen (gemeinsames Tempo, Noten/Patterns
   austauschen).
2. **Musiktagebuch:** offline im Zug aus dem Fenster Rhythmen/Melodien
   aufnehmen (Mikrofon, Tippen, Kamera-Farben), simpel als CSV speichern
   (mit Zeitstempel, optional Ort/Notiz/Foto) für spätere Weiterverarbeitung.

## Nicht im Scope (erste Ausbaustufen)

- X11-Fenster-Capture (`x11cap`) — später via MediaProjection.
- Renoise-PatternClipboard-XML per Clipboard **zwischen Apps** — Android-Share
  statt xclip, Import via Teilen-Menü.
- Shared Memory zwischen Apps — entfällt (alles in EINER App).
- Video-Datei-Decode (MediaCodec) erst nach MVP (Kamera zuerst).

## Architektur-Entscheidungen

1. **Eine App** statt fünf: gemeinsame `core`-Library + ein UI-Crate.
2. **UI:** `android-activity` + `egui`/`eframe` (Immediate-Mode, touch-freundlich,
   schnelle Buttons/Slider) statt SDL2. SDL2 bleibt für Desktop-Crates erhalten.
3. **Wiederverwendung:** reine Module (`micbpm`, `livescan`, `quint`, OSC-Wire,
   OpenMPT-FFI) wandern unverändert in `core`; sie sind hardwarefrei und bereits
   unit-getestet.
4. **Audio:** `oboe-rs` (AAudio) für Wiedergabe (OpenMPT-Callback wie cpal-Engine)
   und Mikrofon (micbpm-Pipeline).
5. **Kamera:** Camera2/CameraX über dünnen JNI-Bridge (JPEG/YUV → RGBA) für MVP;
   Video später MediaCodec.
6. **Relay:** `bundle_udp`-Logik als Foreground-Service mit Notification.
7. **Persistenz/Teilen:** SAF + `ACTION_SEND`/ClipboardManager statt Dateipfade.
8. **Session/Jam mit Ableton Link:**
   - **Tempo/Phase/Transport:** Ableton Link (Multicast UDP 20808, keine
     Discovery nötig). Rust-Anbindung: `rusty_link` (offizieller `abl_link`
     C-Wrapper) als Primärweg, Fallback `ableton-link-rs` (native
     Protokoll-Implementierung), falls der SDK-Build auf NDK zickt.
   - **Wichtig:** Renoise kann Link → Rechner, Handy und Renoise hängen am
     selben Tempo/Phase-Raster; Link ersetzt eigenes Clock-Protokoll.
   - **Note-/Pattern-Austausch:** bleibt OSC (`/jam/note_on`, BPM nur noch
     informativ) plus Transfer kleiner Patterns als OSC-Blob oder TCP-Upload,
     Empfänger speichert `pattern N.csv`.
   - **Pacing:** `secPerBeat`/`livescan`-Takt und `micbpm`-Anzeige folgen der
     Link-Phase; lokal abschaltbar (Link enabled/disabled je Screen).
9. **Musiktagebuch:** offline-first, Einträge app-privat:
   `entries/<datum-zeit>/meta.json` + `pattern.csv` + optional `melodie.csv`,
   `foto.jpg`, `notiz.txt`; Export als ZIP/CSV über Android-Share (SAF).
   Kein Cloud-Zwang; später optional Ordner-Sync über das Session-Modul.

## Schritte

### S1: `core`-Crate extrahieren (Risiko: niedrig)
- Ziel: Hardwarefreie Logik in `core/` verschiebbar, Desktop bleibt grün.
- Dateien: `core/Cargo.toml`, `core/src/{micbpm,livescan,quint,osc,openmpt}.rs`,
  Anpassung der Desktop-Crates auf `core::…`.
- Test/Verify: `nix-shell --run "bash tests/e2e/run_all.sh"` → Exit 0 (Unit + E2E unverändert).
- Rollback: Revert des Commits; Desktop-Crates unverändert funktionsfähig.

### S2: Android-Gerüst + APK-Pipeline (Risiko: mittel)
- Ziel: Leere egui-App auf Gerät startbar.
- Dateien: `android/Cargo.toml`, `android/src/lib.rs`, `android/AndroidManifest.xml`,
  Gradle-Wrapper, `shell.nix`-Erweiterung (Android SDK/NDK, `cargo-ndk`).
- Test/Verify: `nix-shell --run "cd android && ./gradlew assembleDebug"` →
  `app-debug.apk`; `adb install -r app-debug.apk`; `adb shell am start …` →
  `adb logcat -d | grep android_app` zeigt „UI gestartet“.
- Abhängig: S1.

### S3: Modul-Player (libopenmpt + oboe) (Risiko: hoch)
- Ziel: `.it/.xm/.mod` abspielen, Live-Audio wie `it_live`.
- Dateien: `core/src/openmpt.rs` (NDK-Build via CMake), `android/src/player.rs`
  (oboe-Callback mit `openmpt_module_read_stereo`), Dateiauswahl (SAF).
- Test: Unit (core unverändert) + Gerät: 3-s-Datei rendern/abspielen, Log
  „Live-Wiedergabe beendet“; RMS im Callback > 0 (Log).
- Abhängig: S2.
- Rollback: Player-Screen deaktivieren, Rest der App bleibt.

### S4: OSC + Relay-Service (Risiko: mittel)
- Ziel: Noten/BPM an Renoise senden; Relay-Ports als Service.
- Dateien: `core/src/osc.rs` (bereits vorhanden), `android/src/relay.rs`,
  Manifest-Eintrag `foregroundServiceType="dataSync"`, Notification.
- Test: Gerät sendet an Laptop-Listener (`tests/e2e/osc_listen.py 7003`),
  Erwartung: JSONL enthält `/renoise/trigger/note_on`.
- Abhängig: S2.

### S5: Mikrofon-BPM (Risiko: mittel)
- Ziel: `T+M`/`T+L` als Touch: „Messen“ / „Live“, Anzeige + Übernehmen.
- Dateien: `android/src/mic.rs` (oboe-Capture → `core::micbpm`), UI-Screen.
- Test: Unit (micbpm unverändert); Gerät: Klatschen/120-BPM-Klick → Anzeige
  ±2 BPM; Übernehmen sendet OSC-BPM.
- Abhängig: S2, S4.

### S6: Rhythmus-Screen (pattern_recorder-MVP) (Risiko: mittel)
- Ziel: Aufnahme per Note-Pads (injizieren die bisherigen Key-Aktionen),
  Play/Loop, CSV export/import (Share/SAF), Clipboard-XML via Teilen.
- Dateien: `android/src/rhythm.rs`, Portierung der Zustandslogik aus
  `pattern_recorder` (nur benötigte Pfade), `core`-Helfer.
- Test: Unit: `record_key_action`, CSV-Roundtrip, XML-Builder (bereits in core);
  Gerät: aufnehmen → CSV teilen → Datei prüfen (adb pull).
- Abhängig: S2.

### S7: Bild/Video-Screen (rrn5-MVP, Kamera) (Risiko: hoch)
- Ziel: Kamera-Vorschau, Farbauswahl per Touch (Finger = Linksklick: einfrieren
  bei Berührung, malen beim Ziehen), Live-Scan (frei/Takt/Palette),
  Quintenzirkel als Touch-Scheibe, OSC-Ausgabe.
- Dateien: `android/src/camera.rs` (JNI/Camera2 → RGBA), `android/src/image.rs`
  (Canvas + Auswahl + Scan), Wiederverwendung `core::{livescan,quint}`.
- Test: Unit: `livescan::*`, `quint::*` (core); Gerät: Testvideo auf Bildschirm
  abfilmen? Nein — stattdessen Kamera auf farbige Fläche: Live-Scan-Log
  „Sweep beendet, N Noten, N gezeichnet“; OSC-Mitschnitt am Laptop.
- Abhängig: S2, S4.

### S8: Video-Dateien + Feinschliff (Risiko: mittel)
- Ziel: Videodateien via MediaCodec (oder ExoPlayer-Bridge) als Quelle,
  SHM-Modus entfällt (intern), FPS-Regler, Akku-Sparmodus (Pacing wie Desktop).
- Test: Gerät: Datei öffnen, 20 fps-Pacing (Log), CPU im Rahmen.
- Abhängig: S7.

### S2b: Ableton-Link-Spike (Risiko: hoch, zuerst!)
- Ziel: Beweisen, dass Link auf Desktop **und** Android baubar/lauffähig ist,
  bevor UI-Arbeit darin investiert wird.
- Dateien: `core/Cargo.toml` (Abhängigkeit `rusty_link`), `core/examples/link_probe.rs`
  (setzt Tempo, wartet auf Peer, zeigt `time_for_is_playing`/Phase),
  `shell.nix` (CMake/Ninja/Clang für den SDK-Build), NDK-Buildskript.
- Test:
  - Desktop: zwei `link_probe`-Prozesse → Tempo/Phase wird in beiden sichtbar
    (Log „link tempo=… beat=…“ innerhalb 1 s), Loopback-Multicast erlaubt.
  - Android: `cargo ndk … build` erfolgreich, App-Log zeigt Link-Session.
- Verify: `nix-shell --run "cargo run -p core --example link_probe"` (2× parallel),
  `adb logcat | grep "link session"`.
- Abhängig: S1/S2.
- Rollback: Fallback `ableton-link-rs` oder eigenes OSC-Clock-Modul (alter Plan).
- Lizenzhinweis: Link-SDK ist GPLv2+ (kommerzielle Option) — für Privatprojekt ok.

### S9: Session/Jam-Modul (Risiko: mittel)
- Ziel: Tempo/Phase über Link teilen; Noten live, Patterns senden.
- Dateien: `core/src/session.rs` (Link-Session, Enable/Disable, Quanten),
  `android/src/session_ui.rs`, Desktop-Gegenstück in `bundle_udp`/rrn5
  (`--link on`), Pacing-Anbindung in `livescan`/`micbpm`.
- Test: Unit: Link-Quantisierung (Phase→nächster Beat), Enable/Disable;
  E2E: zwei Prozesse (Desktop) teilen Tempo (Assert per Log/OSC),
  Pattern-Transfer als CSV-Upload.
- Abhängig: S2b, S4.
- Rollback: Session-Screen deaktivierbar; Einzelbetrieb unverändert.

### S10: Musiktagebuch (Risiko: niedrig)
- Ziel: Schnell im Zug aufnehmen: Mikrofon-BPM messen, Rhythmus tippen,
  optional Kamera-Farbscan → als Eintrag mit CSV speichern; Liste/Detail-Ansicht;
  Export per Share.
- Dateien: `android/src/diary.rs` (Liste/Detail, Capture-Flow),
  `core/src/diary.rs` (Entry-Metadaten, CSV-Format = vorhandenes
  `timestamp,instrument,pitch,glide`, ZIP-Export).
- Test: Unit: Entry-Serialisierung, CSV-Roundtrip, ZIP-Export (tempdir);
  Gerät: 3 Einträge anlegen → Export teilen → `adb pull` prüfen.
- Abhängig: S2, S5, S6 (MVP).
- Rollback: eigenes Modul, keine Auswirkung auf Jam/Player.

### S11: Kombi-Layout & Feinschliff
- Ziel: Mehrere Module gleichzeitig sichtbar (Split-View/Panels):
  Kamera+Scan | Rhythmus | Session | Diary; Tablet/Querformat-Layouts.
- Dateien: `android/src/layout.rs`.
- Verify: Gerät: zwei Module parallel, Session-Sync aktiv, CPU/Pacing im Rahmen.

### S12: Verpackung/Release
- Ziel: Icon, Version, Signatur, README (Build via Nix), Release-APK.
- Verify: `./gradlew assembleRelease` + Installation auf frischem Gerät.

## Teststrategie

- **Desktop bleibt Referenz:** `tests/e2e/run_all.sh` muss nach jedem Schritt
  grün bleiben (48 Unit-Tests + 9 E2E-Skripte).
- **Android:** Unit-Tests laufen auf Host (core), Gerätetests via `adb logcat`
  mit stabilen Log-Markern (z. B. `SWEEP beendet`, `Mic-BPM`, `OSC note_on`).
- **Netzwerk:** OSC-Mitschnitt wie bisher mit `tests/e2e/osc_listen.py`.

## Risiken & Gegenmaßnahmen

- **libopenmpt/Android-Build:** CMake-Rezept dokumentieren; Fallback: vorkompiliertes `.so`.
- **Kamera/JNI:** dünn halten (nur Frame → RGBA), Kamera-Fehler nicht fatal.
- **Video-Decode:** erst S8; bis dahin nur Kamera.
- **Hintergrund-Service-Restriktionen:** Foreground-Service + Notification, kein
  stiller Background-Audio.
- **Akku/Thermik:** Framepacing wie Desktop, Default 20 fps, Scan-Pacing aus BPM.
- **Ableton Link:** SDK-Build auf NDK (CMake/Clang) und Lizenz (GPLv2+); Spike
  S2b klärt das vorab. Multicast kann in fremden Netzen (Zug-WLAN) blockiert
  sein → Jam nur im lokalen/Ad-hoc-Netz; **Tagebuch funktioniert rein offline**.
- **Renoise + Link:** Gegenprobe mit laufendem Renoise (Link aktivieren) in S2b
  optional, um Tempo/Phase im Gesamtsystem zu bestätigen.

## Grober Aufwand

- S1–S2: 3–5 Tage · S2b (Link-Spike): 2–4 Tage · S3: 3–5 Tage · S4–S5: 3–4 Tage · S6: 4–6 Tage ·
  S7: 5–8 Tage · S8: 3–5 Tage · S9: 3–5 Tage · S10: 3–4 Tage ·
  S11–S12: 2–3 Tage
- MVP für „Diary + Jam im Zug“: S1–S7 + S9/S10 ≈ 6–8 Wochen konzentriert.
