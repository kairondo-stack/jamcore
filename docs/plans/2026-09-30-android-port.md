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

### S5: Mikrofon-BPM (Risiko: mittel) — **geräteseitig fertig und verifiziert**
- Ziel: Tempo aus dem Mikrofon messen (4 s „Messen“ oder „Live“) und per OSC an
  Renoise übernehmen.
- Dateien: `core/src/micbpm.rs` (`pcm16_le_to_f32` + 4 neue Tests), `android/src/mic.rs`
  (Recorder + `MicPanel`), `android/src/lib.rs` (Tab „Tempo“).
- Aufnahme: `AudioRecord` ausschließlich über `AudioRecord$Builder` (die Konstruktoren
  sind auf dem Gerät per Greylist gesperrt, siehe `AGENTS.md`). Aufnahmethread liest
  `read(byte[], …)` PCM 16 Bit little-endian mono, `jamcore::micbpm::BpmEstimator`
  schätzt aus den Einsätzen, die UI liest nur Atomics (Pegel, BPM, Konfidenz,
  Fenster, Rate). Quellen-Kette: VOICE_RECOGNITION/16000 → MIC/44100 → MIC/16000,
  die erste funktionierende wird geloggt.
- Gerätetest (Pixel 10 Pro, 120-BPM-Klick-Spur vom Laptop über `pw-play`,
  Klicks alle 0,5 s, 1 kHz, 8 ms):
  - Live: **120.0–120.1 BPM, Konfidenz 100 %, Pegel 0.547** (logcat `gemessen` fehlt
    hier absichtlich, der Live-Modus zeigt nur an)
  - Messen (4 s): **119.9 und 120.0 BPM, Konfidenz 1.00** (logcat
    `gemessen 120.0 BPM (Konfidenz 1.00)`)
  - Übernehmen: logcat `BPM 120.0 -> Port 7003` (OSC an Renoise/Relay)
  - **Lautstärke ist entscheidend:** mit leiser Quelle (Pegel 0.089) ergab dieselbe
    Spur 135.7 BPM bei Konfidenz 0.50, bei Pegel 0.001 (stumm) gar keine Messung.
    Ab etwa Pegel 0.5 ist das Ergebnis exakt. Die UI zeigt Pegel und Konfidenz,
    damit man das sofort sieht.
- Tests: jamcore **50 grün** (darunter: PCM-Bytes → Werte, ungerades Byte, leeres
  Array, Minimum -1.0 exakt, kompletter PCM-Puffer → 120 BPM bei 44,1 kHz,
  16-kHz-Klickspur in der Form der Testspur → 120 BPM, Schätzer ohne Rate oder
  Fenster). Desktop: `tests/e2e/run_all.sh` → `== ALLES OK`, `cargo fmt` sauber.
- Abhängig: S2, S4.
- Offen / bewusst so:
  - **Nur eine Quelle praktisch erprobt** (VOICE_RECOGNITION/16000); die Fallbacks
    MIC/44100 und MIC/16000 sind im Code, wurden auf dem Gerät aber nicht gebraucht.
  - **Keine Verstärkung (AGC):** leise Quellen liefern unsichere Werte; das ist
    gewollt ehrlich (Pegel wird angezeigt) und kein stiller Fehler.
  - **Der gemessene Wert wird nur per OSC gesendet**, nicht in den Rhythmus-Screen
    übernommen; den Takt dort kann man getrennt stellen.
  - **Kein Vordergrunddienst:** im Hintergrund friert Android die App ein, die
    Aufnahme liefert dann nichts. Der Zustand wird sauber angezeigt: endet der
    Aufnahmethread (Kanal belegt oder entzogen), raeumt die App den Recorder ab
    statt alte Werte weiterzuzeigen.

### S6: Rhythmus-Screen (pattern_recorder-MVP) (Risiko: mittel) — **geräteseitig fertig und verifiziert**
- Ziel: Noten-Raster (Spalte = Schritt, Zeile = Ton), Takt aus Modulzeile oder
  eigener Uhr, bei jedem Tipp sofort OSC, CSV und Clipboard-XML für Renoise.
- Dateien: `core/src/grid.rs` (Pattern-Raster + 24 Unit-Tests), `android/src/rhythm.rs`
  (Screen), `android/src/player.rs` (`grid_step`), `android/src/lib.rs` (Tab „Rhythmus“).
- API (`jamcore::grid`): `Pattern::{new,with_steps,insert,toggle,has_at,clear,to_csv,from_csv,
  to_renoise_xml,from_renoise_xml,timestamp,step_from_timestamp}`, `Note`, `note_name`,
  `pitch_from_note_name`. Zeitachse wie Desktop-Recorder und Renoise-Clipboard: ein Zyklus
  0.0–1.0, ein Schritt `1/steps_per_cycle`.
- Gerätetest (Pixel 10 Pro, `40nix_uwu_light.xm`):
  - Raster in zwei Blöcken à 8 Schritten (16 Spalten passen nicht in 317 pt Breite),
    4 Tonzeilen C-2/C-3/C-4/C-5, Zellen 22 pt
  - Follow an: Playhead wanderte von Schritt 4 auf 9 und läuft mit der Modulzeile mit
    (logcat `Follow = true`, `rms ... row=59` → `row=113`)
  - 5 Taps in 5 verschiedenen Spalten → CSV `0 / 0.125 / 0.25 / 0.375 / 0.9375`
    (Schritte 1, 3, 5, 7, 16) — Spalte ist Schritt
  - CSV auf dem Gerät: `0,0,60,0` … identisch zum Desktop-Format
    `timestamp,instrument,pitch,glide` (`pattern_recorder/src/main.rs:281`)
  - XML auf dem Gerät: `<Note>C-4</Note><Instrument>00</Instrument>` (Hex mit Padding, weil
    der Desktop mit `from_str_radix(_,16)` liest)
  - logcat je Tipp: `Note C-4 vel 100 -> 127.0.0.1:7003`
- Tests: `cargo test --manifest-path core/Cargo.toml` → **44 grün** (24 in `grid.rs`),
  `cargo fmt --check` sauber, `tests/e2e/run_all.sh` → `== ALLES OK`, Desktop-Build Exit 0
  mit nur den zwei vorbestehenden Warnungen. Code-Review eines Subagenten abgearbeitet
  (NaN-Timestamp, Runden am Patternende, Notennamen mit Kreuz und negativer Oktave,
  Instrument-Hex, Eigenuhr-Sättigung, Mutex des Audio-Threads).
- Abhängig: S2, S3 (Takt), S4 (OSC zum Hören).
- Offen / bewusst so:
  - **Semantik ist Step-Editor, kein Aufnahme-Modus.** Ein Tipp setzt die angefasste Zelle;
    der Cursor zeigt nur, wo das Modul ist. „Aufnehmen wie es läuft“ (jeder Tipp auf dem
    aktuellen Schritt) ist nicht gebaut — offener Punkt für S8/Ablösung.
  - **Kein Teilen über die System-UI.** CSV und XML landen im externen App-Ordner
    (`/sdcard/Android/data/org.jamcore.app/files/patterns/`), per `adb pull` erreichbar.
    Share-Intent und Clipboard brauchen eine Java-Klasse (`hasCode=true` ist vorbereitet).
  - **Nur eine Note je Schritt** (Raster-Design). Mehrstimmige Pattern-Zeilen des
    Desktop-Recorders kann das Raster nicht abbilden; Instrument und Glide werden
    beim CSV-Import gelesen, aber im Raster nicht erzeugt.
  - **Follow-Phase ist nicht gefangen**: die Skalierung `row * 4 / rows_per_beat` ist bei
    4 Zeilen pro Beat exakt, bei Halbzeilen (8) sind nur 8 der 16 Schritte erreichbar.
    Die UI liest dafür nur Atomics (Render-Thread schreibt Zeile und `rows_per_beat`),
    damit die Oberfläche den Mutex des Audio-Threads nicht anreift.
  - **XML-Parser ist zeilenbasiert** (wie der Desktop-Recorder): minifiziertes XML ohne
    Zeilenumbrüche liefert leer statt einer Fehlermeldung. `Volume`, `Start`, `Length` werden
    gelesen, aber ignoriert.
  - **Stolperstelle Desktop**: `pattern_recorder::notes::renoise_note_to_midi` liest die
    echte Renoise-Notation falsch („C-4“ → Pitch 0, weil es `-4` als Oktave parst) und
    schreibt selbst „C4“. Die App schreibt die echte Notation; ein von der App
    exportiertes XML sollte deshalb direkt in **Renoise** eingefügt werden, nicht über
    den Desktop-Recorder.

### S7: Bild/Video-Screen (rrn5-MVP, Kamera) (Risiko: hoch)
- Ziel: Kamera-Vorschau, Farbauswahl per Touch (Finger = Linksklick: einfrieren
  bei Berührung, malen beim Ziehen), Live-Scan (frei/Takt/Palette),
  Quintenzirkel als Touch-Scheibe, OSC-Ausgabe.
- **Voraussetzung Java-Brücke (Befund vor dem Start):** Camera2 verlangt
  Pflicht-Callbacks (`CameraDevice.StateCallback`,
  `CaptureSession.StateCallback`, `ImageReader.OnImageAvailableListener`), die
  sich in reinem Rust/JNI nicht erzeugen lassen. Die App bekommt deshalb eine
  kleine Java-Klasse (`android/app/src/main/java/org/jamcore/app/CameraBridge.java`)
  samt Gradle-Source-Set; sie öffnet die Kamera, hält den ImageReader
  (YUV_420_888) und ruft eine native Methode mit den drei Plane-Puffern auf.
  (Für `AudioRecord`/`AudioTrack` war das nicht nötig, die haben
  argumentlose Methoden bzw. erlaubten den Builder.)
- Dateien: `android/src/camera.rs` (JNI an CameraBridge, Frame-Puffer),
  `core/src/yuv.rs` (YUV_420_888 → RGBA, hardwarefrei testbar),
  `android/src/image.rs` (Canvas, Auswahl, Scan), `core::{livescan,quint}`
  (vorhanden und getestet).
- Teilschritte:
  - **S7a** `core/src/yuv.rs` (TDD): Planes mit Row-/Pixel-Strides → RGBA;
    Testflächen, Stride-Fälle (Padding, PixelStride 2), Schwarz/Weiß.
  - **S7b erledigt (2026-10-06):** `android/app/src/main/java/org/jamcore/app/CameraBridge.java`
    (Camera2, YUV_420_888, 640x480, Gradle kompiliert `src/main/java` automatisch);
    die native Methode `Java_org_jamcore_app_CameraBridge_nativeFrame` rechnet
    über `jamcore::yuv` nach RGBA. Gerätetest: Log `start angestossen`,
    `Vorschau laeuft`, `camera frames=141 size=640x480` (~30 je Sekunde),
    keine Crashs. **Falle:** `find_class` findet App-Klassen aus angehängten
    Threads nicht (Bootstrap-ClassLoader) – die Klasse über
    `activity.getClassLoader().loadClass("org.jamcore.app.CameraBridge")` holen.
  - **S7c erledigt (2026-10-06):** Bild-Tab mit Vorschau: Textur wird nur bei
    neuem Zählerstand geladen (`ColorImage::from_rgba_unmultiplied`), ~20
    Bilder/s Repaint, Seite skaliert auf die verfügbare Breite. Gerätetest:
    Status `Frame 122 (640x480)`, farbkorrektes Bild (Laptop/Tisch) — die
    YUV-Konvertierung stimmt auf echter Hardware. Die Tab-Leiste ist jetzt
    seitlich wischbar (sechs Tabs). Offen für S7d/S7e: Auswahl, Malen,
    Live-Scan, Quintenzirkel, OSC.
  - **S7d-a erledigt (2026-10-06):** Finger auf der Vorschau halten = Bild
    einfrieren + Farbe unter dem Finger; Ziehen wandert die Auswahl; alle
    Pixel in Toleranzweite werden markiert (grün) und gezählt, Toleranz-Regler
    und Hintergrundfarbe sichtbar, „Auswahl löschen“ gibt die Live-Anzeige
    wieder frei. Gerätetest: `bild: Auswahl RGB(47, 45, 40), 108174 Pixel`,
    Markierung im Screenshot deutlich sichtbar.
  - **S7d-b erledigt (2026-10-06):** Quintenzirkel als Touch-Scheibe im Tab
    „Kreis“: zwölf Segmente mit Notennummern, Tippen toggelt und sendet beim
    Hinzufügen sofort `note_on` (Port 7000 über die OSC-Basis des OSC-Tabs),
    Statuszeile und „Notenklassen“ zeigen die Harmonie
    (`core::quint::note_classes`). Gerätetest: `quint: Note 60 (Klasse 0)`,
    Segment hellblau, Klassen `[0]`; zweites Segment → `[0, 6]`.
  - **S7e erledigt (2026-10-06), Palette offen:** Live-Scan über die Spalten
    (Schrittweite 2). „frei“ mit fester Spaltenrate, „Takt“ macht einen Sweep
    pro Beat (BPM-Regler; Kopplung an Modul/Tempo-Tab offen). Treffer werden
    in ihrer Farbe in den Sweep-Puffer gezeichnet (nur sie leuchten), der
    Notenwert kommt aus der Zeile (oben hoch, unten tief, 36..72), die
    Sendefrequenz ist über „Noten/s“ begrenzt (`note_allowed`), OSC geht an
    Port 7000 über die Basis des OSC-Tabs. Gerätetest: Logs
    `Scan-Note 65 -> 127.0.0.1:7003` und `Sweep beendet, 6 Noten, 38340
    gezeichnet (frei)`; im Takt-Modus 0,51 s je Sweep bei 120 BPM.
    **Palette-Modus offen:** er braucht das „Malen“ (zweiter Puffer), das die
    App noch nicht hat.
- Test: Unit: `yuv`, `livescan`, `quint` (core); Gerät: Kamera auf farbige
  Fläche halten, Sweep-Log, OSC-Mitschnitt am Laptop.
- Abhängig: S2, S4.
- Rollback: Falls Camera2 über die Java-Brücke an Android-17-Fallen scheitert,
  Video-Dateien (S8) als Bildquelle nehmen und die Kamera später nachziehen.

### S8: Video-Dateien + Feinschliff (Risiko: mittel) — **Kern erledigt, verifiziert**
- Ziel: Videodateien via MediaCodec als Quelle, FPS-Regler, Pacing.
- Dateien: `android/app/src/main/java/org/jamcore/app/VideoBridge.java`
  (MediaExtractor + MediaCodec, synchron, Endlosschleife per seekTo+flush),
  `android/src/video.rs` (JNI), `android/src/image.rs` (Videoliste, fps-Regler).
- Gerätetest (Pixel 10 Pro): Screencast-MP4 (H.264, 1080x2410) in den
  App-Ordner `videos/` kopiert, im Bild-Tab gefunden und abgespielt —
  Log `start ... (video/avc, 20 fps)`, Status `Frame 59 (1080x2410)`,
  Bild in der Vorschau farbkorrekt. Die dekodierten Frames gehen durch
  dieselbe Pipeline wie die Kamera (`camera::ablegen`).
- Offen: Videos von außerhalb des App-Ordners brauchen SAF oder
  READ_MEDIA_VIDEO; CPU-Messung steht aus; SHM/externe Bildausgabe entfällt
  bewusst (die App zeigt selbst an).
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
- **Ergebnis Android-Spike (2026-10-06): bestanden.**
  - `android/Cargo.toml` bindet `jamcore` mit Feature `link` ein; das Link-SDK
    kompiliert für arm64-v8a (APK 30 MB).
  - Zwei Ergänzungen in `android/`: `link-android-toolchain.cmake` +
    `link_shim/android-link.cmake` setzen `LINK_PLATFORM_LINUX=1` (Android ist
    für Link Linux) über `CMAKE_PROJECT_INCLUDE`; ein Erweitern von
    `CMAKE_CXX_FLAGS` in der Toolchain-Datei wird von der cmake-Kiste
    überschrieben, `CMAKE_PROJECT_INCLUDE` läuft nach `project()`. Dazu
    `link_shim/htonll.h`, weil Bionic kein `htonll` hat (glibc-Erweiterung).
    `build-apk.sh` exportiert `CMAKE_TOOLCHAIN_FILE` (die cmake-Kiste liest die
    Umgebungsvariable).
  - **Falle:** CMake lädt die Toolchain-Datei nur beim ersten Configure; ein
    vorhandenes `android/target/<triple>/release/build/rusty_link-*` ignoriert
    sie (Cache hält den alten, leeren Pfad). Bei Änderungen daran löschen.
  - Gerätetest (Pixel 10 Pro): Log `link session gestartet (tempo=120)` und
    `link tempo=120.00 peers=0 beat=…`; die Beat-Differenz ist exakt 2,0
    Beats/s = 120 BPM, kein Absturz.
  - Offen für S9 (bewusst nicht Spike): Multicast-Peers brauchen ein
    gemeinsames Netz **und** auf Android ein `WifiManager.MulticastLock`
    (Java-API), sonst kommen die Multicast-Pakete nicht an; phasegenaue
    Übernahme gehört in den Audio-Callback (`capture_audio_session_state`,
    LinkHut-Muster); das Spike-Logging wird bei der UI-Integration ersetzt.

### S9: Session/Jam-Modul (Risiko: mittel) — **Kern erledigt, verifiziert**
- Ziel: Tempo/Phase über Link teilen; Noten live, Patterns senden.
- Dateien: `core/src/session.rs` (6 Tests: Rasterpunkt, Wartezeit, Tempo-Folgen,
  Unsinn-Eingaben), `android/src/session_ui.rs`, Tab „Session“, Tab-Leiste
  bricht jetzt um (acht Tabs).
- Gerätetest (Pixel 10 Pro): Session-Tab zeigt `120.0 BPM`, „Link an“, Beat und
  nächsten Rasterpunkt (`Beat 117.241 → 120 in 1.38 s`, bei 120 BPM korrekt);
  „Link aus“/„Link an“ loggt `session: Link …`; „Tempo folgen“ setzt das
  Scan-Tempo im Bild-Tab (Status `Scan-Tempo folgt der Session (120.0 BPM)`).
- **MulticastLock erledigt (2026-10-06):** `CHANGE_WIFI_MULTICAST_STATE` im
  Manifest, `LinkSupport.java` holt/gibt das Lock, Rust ruft es beim ersten
  Frame (wenn Link an) und beim Umschalten. Gerätetest:
  `WifiService: acquireMulticastLock ... jamcore-link`, Log `MulticastLock aktiv`.
- **Peer-Test bestanden (2026-10-06), Handy-Hotspot als Standardfall:**
  Zwei Hürden gelöst: Die NixOS-Firewall des Laptops blockte eingehendes UDP
  (`networking.firewall.allowedUDPPorts = [ 20808 ]` + `allowPing`), und Link
  sucht Interfaces nur beim Start – nach dem Hotspot-Einschalten hilft der
  Knopf „Link neu starten“ (Multicast-Gruppe auf `ap_br_wlan2` prüfbar über
  `/proc/net/igmp`). Ergebnis: Laptop `peers=1 tempo=100.00`, Handy
  `link tempo=100.00 peers=1` – das Handy folgt dem Laptop-Tempo.
- Offen: Desktop-Gegenstück `--link on` in `bundle_udp`/rrn5,
  Pacing-Anbindung in `micbpm`, Pattern-Transfer als CSV.
- Abhängig: S2b, S4.
- Rollback: Session-Screen deaktivierbar; Einzelbetrieb unverändert.

### S10: Musiktagebuch (Risiko: niedrig) — **Kern erledigt, verifiziert**
- Ziel: Schnell aufnehmen: Noten/Tempo als Eintrag speichern; Liste/Detail;
  Export später per Share.
- Dateien: `core/src/diary.rs` (Entry, Textformat mit Escaping, Kalender
  ohne Bibliothek, 6 Tests), `android/src/diary.rs` + Tab „Tagebuch“.
- Ein Eintrag übernimmt das aktuelle Raster (`rhythm.pattern()`) und, wenn
  gemessen, das Tempo aus dem Tempo-Tab; gespeichert wird je Eintrag eine
  Datei in `tagebuch/` (atomar, Name aus Datum/Uhrzeit in UTC).
- Gerätetest (Pixel 10 Pro): zwei Einträge angelegt
  (`2026-10-06_11_52.txt`, `2026-10-06_11_53.txt`), Inhalt geprüft — Kopf
  `JamCore-Tagebuch v1`, Noten im Desktop-CSV (`0,0,48,0`, `0.125,0,36,0`),
  Liste zeigt „2026-10-06 11:52 · Zug-Skizze (2 Noten)“, Detail mit CSV.
- Offen: **ZIP-Export und Share-Intent** (brauchen Java/SAF), Notizfeld ist
  einzeilig (Escaping für Umbrüche ist getestet).
- Abhängig: S2, S5, S6 (MVP).
- Rollback: eigenes Modul, keine Auswirkung auf Jam/Player.

### S11: Desktop-Parität (Zwischenstand 2026-10-06) — **Kern erledigt, auf Gerät verifiziert**
Anlass: Nutzer-Hinweis, dass rrn5 und pattern_recorder am Handy noch nicht
vollständig seien. Abgearbeitet:
- **Rendern** (Taste `o`): Raster in Schleife im 16tel-Takt, OSC + **lokaler
  Klang** (AudioTrack-Sinus), optional Renoise-Transport, eigenes BPM;
  Playhead zeigt die Abspielposition. Gerätetest: „Rendern läuft
  (2 Noten, 128 BPM)", Logstart/-stopp.
- **Aufnahme-Modus**: Pad-Tipp landet live auf dem laufenden Schritt
  (Gerätetest: bei durchgehend Spalte 1 getippt stehen die Noten auf
  Schritt 8 und 15).
- **Acht Pattern-Slots** wie der Desktop (`pattern N.csv`/`pattern N.xml`,
  Zählung ab 0). Gerätetest: `pattern 0.csv` und `pattern 1.csv` getrennt.
- **Transformationen**: ±12, Umkehren, Q2/Q4, Glide, **Skalenfilter**
  (Moll/Dur/beide Pentatoniken + Grundton) — `jamcore::transformation`
  (zehn Tests). Gerätetest: +12 (36→48, 72→84), Umkehren (0→15, 8→7).
- **noteadjust-Modi** im Scan (Note aus Zeile/Spalte, Lautstärke aus x) über
  `jamcore::livescan::note_volume` (vier Tests). Gerätetest: „Zeile" gemischt
  (72/61/62), „Spalte" monoton (47→53→59→65→71).
- **BPM-Tap**: `transformation::bpm_aus_taps` (Ausreißerfilter) plus
  Knopf im Tempo-Tab (Gerätetest: 78 BPM bei 0,77 s Tippabstand).
- **Presenter-Ansicht** (Taste J): Umschalter mit großem Tempo, Notenliste
  und Zeitachse (x = Schritt, y = Tonhöhe, Playhead).
- **Farbbereiche je Kanal** (R/G/B) + **Schrittweite** (`iadd`, 1..4) im
  Bild-Tab (Gerätetest: R von 24 auf 0 senkt die Treffer 37105 → 1404).
- **Malen/Merken + Palette-Scan**: Trefferfarben landen in einem Merker,
  der Scan matcht wahlweise gegen die Palette (Gerätetest: 487 Farben,
  Sweep-Log).
- Nebenbei: `adb wireless` eingerichtet (Pairing + mDNS), Projekt-Backup als
  `git bundle` erstellt.

Rest für die Parität:
- Rest: **Takt-Masken** (`biialpha`, rhythmische Spaltenmasken) und
  Alpha-Skalierung beim Malen, Radierer;
- pattern_recorder-Rest: `remove_duplicates`, Schism-Export,
  Notenlänge/Instrument-Eingabe, `remove_and_record`.
- Offen bleibt das Kombi-Layout:

### S11b: Kombi-Layout & Feinschliff (offen)
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
