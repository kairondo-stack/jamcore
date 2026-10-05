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

### S2: Android-Gerüst + APK-Pipeline (Risiko: mittel) — **erledigt, auf Gerät verifiziert**
- Ziel: Leere egui-App auf Gerät startbar.
- Dateien: `android/Cargo.toml`, `android/src/{lib,insets}.rs`, `android/settings.gradle`,
  `android/build.gradle`, `android/gradle.properties`, `android/app/build.gradle`,
  `android/app/src/main/AndroidManifest.xml`, `android/build-apk.sh`,
  `shell.nix`-Erweiterung (Android SDK/NDK, `cargo-ndk`, gradle, jdk17, rustup).
- Test/Verify: `nix-shell --run "bash android/build-apk.sh --install"` →
  `app-debug.apk` (arm64-v8a, debug); `adb install -r` → `Success`;
  `adb shell am start -n org.jamcore.app/android.app.NativeActivity` →
  logcat `jamandroid: android_main gestartet`; Screenshot zeigt UI unter der
  Statusleiste; `adb shell input tap` → `jamandroid: button gedrueckt`.
  Gerät: Pixel 10 Pro, Android 17, 1080x2410.
- Abhängig: S1.
- Ergbnisse/Fallen (beim Umsetzen aufgetreten):
  - `androidsdk` (Komplett-SDK mit Emulator/System-Images) ist auf diesem Rechner
    nicht baubar (Download-Timeouts). `shell.nix` nutzt jetzt
    `composeAndroidPackages` mit `platform-tools` + `build-tools 34.0.0` +
    `platforms 34`; `ANDROID_HOME`/`ANDROID_NDK_ROOT` zeigen auf
    `<paket>/libexec/android-sdk` bzw. `<ndk>/libexec/android-sdk/ndk-bundle`.
  - eframe 0.36: `App::update` gibt es nicht mehr (Pflichtmethode ist `App::ui`),
    `TopBottomPanel` ist zu `egui::Panel::top` verschmolzen, und `accesskit`
    verträgt sich nicht mit `android-native-activity` →
    `eframe = { default-features = false, features = ["android-native-activity",
    "default_fonts", "glow", "wayland", "x11"] }`.
  - AAPT2 aus dem Gradle-Maven-Cache startet unter NixOS nicht (stub-ld).
    `build-apk.sh` setzt deshalb
    `-Pandroid.aapt2FromMavenOverride=$ANDROID_HOME/build-tools/34.0.0/aapt2`.
  - Android 15+ ist Edge-to-Edge, egui liefert auf Android keine Safe-Area
    (`get_safe_area_insets` ist iOS-only). `android/src/insets.rs` fragt die
    Insets per JNI ab (`getRootWindowInsets().getInsets(Type.systemBars())`),
    mit Fallback über die Ressource `status_bar_height`; Umrechnung px → egui-Punkte
    über `pixels_per_point`.
  - JNI-Fehler auf Android: `WindowInsets$Type.systemBars()` ist eine Methode,
    kein Feld; nach jedem JNI-Fehler muss die Ausnahme mit `exception_clear()`
    entfernt werden, sonst bricht CheckJNI den nächsten Aufruf ab.
  - Android-Toolchain: `rustup`-stable + `target add aarch64-linux-android`,
    cargo-ndk baut die `.so`; `gradle` baut das APK (kein Wrapper-JAR im Repo).

### S3: Modul-Player (libopenmpt + AudioTrack) (Risiko: hoch) — **erledigt, auf Gerät verifiziert**
- Ziel: `.it/.xm/.mod` abspielen, Live-Audio wie `it_live`.
- Dateien: `core/src/openmpt.rs` (gemeinsame FFI + Wrapper, von Desktop und
  Android genutzt), `core/build.rs` (libopenmpt 0.8.7 per ndk-build für arm64),
  `android/build.rs` (C++-Runtime ans Linkzeilen-Ende), `android/src/player.rs`
  (AudioTrack via JNI), `android/src/{jniutil,paths}.rs`, `android/src/lib.rs`
  (Player-Seite), `android/build-apk.sh`, `src/main.rs`/`Cargo.toml` (Desktop auf
  jamcore umgestellt).
- Test/Verify: `bash android/build-apk.sh --install` → `Success`; logcat
  `Modul … geladen (18 Kanäle, 26 Orders)`, `AudioTrack 44100 Hz stereo PCM16`,
  `Wiedergabe startet`; Bildschirm: `Peak: 0.064  Position: 0.2 s  Order/Row: 0/6
  Frames: 10240`, Status `spielt: 40nix_uwu_light.xm`; Audio-HAL aktiv
  (`AHal::Cs35l43Codec: [PCM Source] = 9`, `is_any_active_track_on_sink: true`).
  Desktop: `tests/e2e/run_all.sh` → `== ALLES OK` (10 Skripte), 74 Unit-Tests.
- Abhängig: S2.
- Rollback: Player-Screen deaktivieren, Rest der App bleibt.
- Erfahrungen/Fallen (wichtig für spätere Schritte):
  - **Android 17 (Pixel 10 Pro) hat die Legacy-APIs abgeschnitten:** `libaaudio.so`
    ist nur ein Shim ohne C++-Symbole, `AAudio_createStreamBuilder` ist eine
    8-Byte-Funktion und liefert `nullptr`; die `SL_IID_*`-Symbole von
    `libOpenSLES.so` sind für Apps unbrauchbar (statt der ID kommt Müll aus der
    Stringtabelle, `GetInterface` gibt `nullptr`); `AudioTrack.write(float[],…)`
    existiert nicht mehr. Funktioniert hat: **AudioTrack + `write(byte[],…)` mit
    PCM 16-Bit**. Auch Oboe hätte nicht geholfen, weil es dieselbe C-API nutzt.
  - **libopenmpt per NDK statt CMake:** der Git-Baum hat kein `CMakeLists.txt`
    (das liegt nur im Release-Tarball von lib.openmpt.org, der nicht erreichbar
    war). Offizieller Weg ist `build/android_ndk/Android.mk` + `ndk-build`; das
    Hilfsprojekt muss in einem Pfad ohne Leerzeichen liegen (unser Projektordner
    enthält eins), das Ergebnis wird nach `target/openmpt/<Version>-<abi>-<ndk>`
    kopiert. `APP_ABI`/`APP_PLATFORM`/`APP_STL` müssen per eigener
    `Application.mk` gesetzt werden, `ndk-build` ignoriert die Env-Varianten.
  - **Link-Reihenfolge:** `libc++_shared.so` muss per `cargo:rustc-link-arg` ans
    *Ende* der Linkzeile (ein `-l` aus einem abhängigen Crate landet davor und
    lässt offene Symbole). Der Suchpfad muss das **versionierte** API-Verzeichnis
    (`sysroot/usr/lib/<triple>/26`) sein — im unversionierten liegen auch `.a`,
    und `-lc`/`-lz` lösen sonst statisch auf (eigenes `getauxval` → SIGSEGV beim
    Laden). `libc++_shared.so` muss zusätzlich ins `jniLibs` kopiert werden.
  - **Echter FFI-Bug gefunden (auch Desktop betroffen):** `openmpt_module_read_stereo`
    ist die **int16**-Variante; für f32-Puffer braucht es
    `openmpt_module_read_float_stereo`. Ebenso hat `openmpt_module_get_metadata`
    zwei Argumente (`mod`, `key`) — mit einem Argument las der Desktop Müll.
    Vorheriger WAV-RMS (9789) war consequently Unsinn, echter Wert: 778.
  - egui repaintet nur bei Eingaben; für Live-Werte `request_repaint_after`
    nötig, sonst stehen Peak/Position eingefroren.
  - Noch offen: blockierendes Laden (Dateilesen + `Mod::from_bytes` + `join()`
    im egui-Thread friert die UI kurz ein), `/sdcard/Download` braucht auf
    Android 13+ eine Berechtigung (App-eigener Ordner funktioniert immer),
    Dateiauswahl per SAF fehlt (MVP: Ordner-Scan).
### S4: OSC an Renoise/Relay (Risiko: mittel) — **geräteseitig fertig und verifiziert**
- Ziel: Noten/Transport/BPM vom Handy an Renoise oder an das Relay (`bundle_udp`).
- Dateien: `core/src/osc.rs` (gemeinsamer OSC-Ausgang `OscOut`/`Renoise`, 5
  Unit-Tests mit echten Loopback-Sockets), `android/src/osc.rs` (`OscPanel`),
  `android/src/lib.rs` (Tab „OSC“), `core/Cargo.toml` + `android/Cargo.toml`
  (`rosc = "0.9"`).
- API: `/renoise/trigger/note_on` (-1 -1 note volume), `/renoise/song/bpm`,
  `/renoise/transport/{start,stop}`, `/renoise/song/edit/pattern_follow`;
  Ports 7000/7001/7002 → Basis +0/+1/+2 (wie im C++-Original).
- Test/Verify:
  - `cargo test --manifest-path core/Cargo.toml` → 20 passed (5 OSC-Tests:
    Port-Mapping, Adress-Decode, Float-Argument als big-endian f32, Port-Mapping
    7001 → base+1, ungültiger Host)
  - Gerät: App starten, Tab „OSC“, Host `127.0.0.1`, Port `7003` → Tipp auf
    „60“ liefert im logcat `OSC Note 60 -> Port 7003`
  - **Paket-Nachweis auf dem Gerät:** `adb shell "nc -u -l -p 7003 > /data/local/tmp/osc_hex.txt"`
    ergab nach dem Tipp 52 Byte `/renoise/trigger/note_on,iiii … 0x64` (Velocity 100)
  - Desktop: `tests/e2e/run_all.sh` → `== ALLES OK`, keine neuen Warnungen
- Abhängig: S2.
- Offen / nächste Schritte:
  - **Weg zum Laptop ist ungetestet:** `adb reverse` unterstützt nur TCP, OSC läuft
    über UDP; das Handy war beim Test nur im 5G-Netz (kein WLAN). Für den echten
    Betrieb braucht es beide Geräte im selben Netz (Laptop-IP in der App eintragen)
    oder einen TCP→UDP-Tunnel. Derselbe Test wird beim nächsten Geräteversuch
    mit `osc_listen.py 7003` auf dem Laptop und der Laptop-IP in der App wiederholt.
  - **Vordergrund-Dienst ist nicht umgesetzt:** die App hat keinen Java-Code
    (`hasCode=false` → kein dex), ein echter `foregroundService` mit Notification
    braucht eine Java-Klasse und `foregroundServiceType="dataSync"`. Bewusst
    offen gelassen (Plan-Schritt), weil ein NativeActivity-Hintergrunddienst erst
    mit Java-Unterbau sinnvoll ist.
  - `rrn5/src/osc.rs` und `pattern_recorder/src/osc.rs` haben noch eigene
    env-gesteuerte Kopien; Umstellung auf `jamcore::osc` ist ein späterer
    Aufräumschritt (Desktop bleibt bewusst unangetastet, damit die E2E-Suite grün
    bleibt).

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

**Ergebnis Desktop-Spike (2026-09-30): bestanden.**
- `rusty_link` 0.4.9 vendort das komplette Link-SDK (GPLv2) und baut es per
  CMake aus dem Quelltext (Nix: `cmake`, `ninja`, `clang`, `libclang` ergänzt).
- Zwei Prozesse über Loopback-Multicast: Master setzt 100 BPM, zweiter Prozess
  (Start 140 BPM) übernimmt innerhalb < 1 s (`peers=1, tempo=100.00`).
- Phase: für Sample-genaue Sync im Audio-Callback `capture_audio_session_state`
  nutzen (LinkHut-Muster), nicht den App-State-Poll.
- **Android-TODO:** NDK-Build steht aus (braucht Android-SDK/NDK, S2). SDK ist
  quelloffen und Android-fähig; voraussichtlich kleine Anpassung nötig, damit
  die C++-Runtime verlinkt wird (Android: `c++_shared` statt `stdc++`) —
  Workaround über `build.rs` in `jamcore` (target_os="android").

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
