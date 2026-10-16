# Arbeitsweise für dieses Projekt

Gilt für jede Arbeit an JamCore (Desktop-Tools in Rust, Android-Port, Nix-Pakete).

## Skills

Skills vor der Arbeit laden, nicht aus dem Gedächtnis arbeiten:

| Skill | Wann |
|---|---|
| `systematic-debugging` | Bei **jeder** Fehlermeldung, jedem Crash, jedem „funktioniert nicht“. Erst Ursache messen, dann fixen — kein Raten, kein Patchen auf Verdacht. |
| `test-driven-development` | Bei jedem neuen Feature: failing test zuerst (Desktop: `cargo test`, Jamcore-Logik: Unit-Tests mit echten Loopback-Sockets). |
| `verification-before-completion` | Vor jeder Fertigmeldung: Befehle laufen lassen und Ausgabe zeigen. Kein „sollte klappen“. |
| `code-review` | Vor Commit/Abgabe. |
| `writing-plans` | Vor neuen Abschnitten (S5–S12), Schritte mit Dateien, Test und Verifikation. |
| `brainstorming` | Bei neuen Features, deren Anforderungen unklar sind. |

## Subagenten für Parallelarbeit

Immer wenn Aufgaben unabhängig sind, parallelisieren statt nacheinander:

- **Verifikation an Subagenten delegieren** (sie laufen im selben Workspace):
  - `tests/e2e/run_all.sh` (Desktop-E2E, 4–6 min)
  - `cargo test` je Crate, Build-Checks, Review von Code
- **Recherche parallel**: libopenmpt/Live-Signaturen, NDK-Build-Flagen, Android-API-Verhalten.
- **Regeln für Subagenten**: detaillierter Auftrag mit exakten Befehlen,Pfaden und
  gewünschtem Berichtsformat; ausdrücklich „keine Dateien ändern, nichts committen",
  wenn nur Verifikation gewünscht ist; sie niemals parallel dieselbe Datei
  bearbeiten lassen wie ich.

## Umgebung

- Alles läuft über `nix-shell` (Projektordner). Ohne Nix-Shell fehlen SDL2,
  OpenCV, ALSA, libclang, Android-SDK/NDK, gradle, jdk17.
- In `nix-shell` **kein** `sccache` als `RUSTC_WRAPPER` (TMPDIR-Problem):
  `RUSTC_WRAPPER=` setzen, `TMPDIR=/tmp/opencode`.
- Cargo-NDK: `cargo ndk -t arm64-v8a --platform 26 -o app/src/main/jniLibs build --release`
  (Platform 26 nötig, sonst findet der Linker `libaaudio` nicht; minSdk ist 26).
  Der Emulator ist x86_64, dafür `-t x86_64` (macht `build-apk.sh` automatisch).
- APK: `bash android/build-apk.sh --install` (Default arm64-v8a **und** x86_64;
  `ABIS=arm64-v8a` nur Handy, `ABIS=x86_64` nur Emulator, `--abi <abi>` geht auch).
- Desktop-Nix-Pakete (Kopie): `~/jam-nix`, Start in `Start.txt`, `./sync.sh` zum
  Aktualisieren, `./update-hashes.sh` nach Cargo-Änderungen.

## Gerätetests (Android)

**Emulator statt Handy (Standard seit 2026-10-15).** Liegt in der Nix-Shell bereit
(Emulator 37.1.10, AOSP-Image API 36 x86_64, KVM; abschaltbar mit
`nix-shell --arg emulator false`). AVD `jamcore_api36` (Pixel-6-Profil,
1080x2400) liegt in `.android/` im Projekt (gitignored), Steuerung mit
`android/emulator.sh`:

- `bash android/emulator.sh start [--headless|--cold|--wipe]` – booten (Fenster
  standardmäßig; `--headless` für Automatisierung), `stop`, `restart`, `status`
- `bash android/emulator.sh install` – baut nur x86_64, installiert und startet
  die Activity; `app` startet sie frisch, `logcat` (= `-s jamandroid`), `shot`,
  `tap X Y`, `shell`, `wipe`
- **Screenshots sind 1:1 (1080x2400)** – die Skalierung des Handys entfällt,
  `tap` benutzt exakt die Screenshot-Koordinaten.
- Start ist Quick-Boot aus dem Snapshot (~5 s). Eine dort laufende App bleibt
  nach dem Restore **schwarz** (stale GL-Kontext): `emulator.sh start` beendet
  sie deshalb selbst, danach `emulator.sh app` (oder `start --cold`).
- Netz des Emulators ist NAT, kein LAN: Host = `10.0.2.2`; Multicast
  (Ableton Link 224.76.78.75) verlässt den Emulator nicht → `peers=0` ist dort
  normal; `adb reverse` weiterhin nur TCP.
- Details/Ergebnisse: `docs/plans/2026-10-15-emulator.md`.

**Handy (weiterhin möglich):**

- `adb shell input tap <x> <y>`; Screenshot ist 872x1968 skaliert, Gerät
  1080x2410 → x·1.2385, y·1.2246.
- **`input tap` wird von egui nicht als Klick erkannt** (gemessen 2026-10-16,
  Pixel 10 Pro): `adb shell input swipe X Y X Y-1 120` benutzen (1 px, ~120 ms)
  oder Tastatur (`input keyevent 61/66` = TAB/ENTER, Fokusring sichtbar).
- **Pixel-Taskbar** (ab Android 16/17 sichtbar): das App-Fenster beginnt dann
  bei `x=172` (`dumpsys window | grep frame`), alle Touch-x um +172 verschieben.
- Im **Hochkant** reagierte Touch phasenweise gar nicht, im **Querformat** wieder
  (Accelerometer dreht selbst) – vor Gerätetests Screenshot machen und die Lage
  prüfen.
- Start: `adb shell am start -n org.jamcore.app/android.app.NativeActivity`
  (die Activity ist `android.app.NativeActivity`, **kein** `.MainActivity`).
- Tabs liegen bei Screenshot-y≈148, also Geräte-y≈181. Fünf Tabs (x Gerät):
  Überblick 254, Player 402, OSC 505, Rhythmus 629, Tempo 778.
- `adb logcat -d | grep jamandroid` ist die Quelle der Wahrheit; Bildschirmfotos nur
  zur Kontanz.
- **Handy-Hotspot + Laptop = haeufigstes Setup, adb per TCP**: Im AP-Modus ist
  „Drahtloses Debugging“ ausgegraut (verlangt WLAN-Station). Einmal USB
  anstecken, dann `adb tcpip 5555` und `adb connect <gateway>:5555`
  (**die Hotspot-IP wechselt!** Gateway aus `ip route | awk '/default/{print $3}'`
  lesen — am 2026-10-16 war es 10.105.239.30 statt 10.179.32.50). Danach laeuft
  adb ohne Kabel bis zum Handy-Neustart. APK fuer das Handy **arm64-v8a** bauen
  (`bash android/build-apk.sh --abi arm64-v8a`) — ein x86_64-APK scheitert mit
  `INSTALL_FAILED_NO_MATCHING_ABIS`.
- `adb reverse tcp<port> tcp<port>` für TCP. **UDP kann adb reverse nicht** — OSC
  von Handy zu Laptop braucht beide Geräte im selben Netz (Laptop-IP in der App).
- Geräte-Seitennachweis ohne Netz: `adb shell "nc -u -l -p 7003 > /data/local/tmp/o.txt"`
  und danach `cat`/`od -c` — beweist, dass echte Bytes das Handy verlassen.
- `python3` gibt es in der Nix-Shell **nicht**; für Skripting `perl`, `sed`,
  `od` benutzen.
- Log-Präfix: Das Log-Target ist schon der Modulname (`jamandroid::rhythm`), also
  **kein** `jamandroid::rhythm:` zusätzlich in die Meldung schreiben.

## Fallen, die schon Zeit gekostet haben

- **Android 17 (Pixel 10 Pro)**: AAudio-C-API ist ein Stub (`nullptr`),
  `SL_IID_*` von libOpenSLES unbrauchbar, `AudioTrack.write(float[],…)` fehlt.
  Funktioniert: AudioTrack + `write(byte[],…)`, PCM 16-Bit.
- **OSC-Ports in der NixOS-Firewall freigeben**: `networking.firewall`
  blockt eingehendes UDP; fuer Renoise/bundle_udp (z.B. 7005) und die
  Desktop-Werkzeuge ist `allowedUDPPortRanges = [ { from = 7000; to = 7010; } ]`
  gesetzt (plus 20808 fuer Link). Ohne die Regel sendet die App, aber in
  Renoise kommt nichts an (Diagnose 2026-10-07: `nc -u` vom Handy kam an,
  die App-Note nicht).
- **Ableton-Link über Handy-Hotspot (gelöst 2026-10-06)**: Zwei Ursachen für
  `peers=0`: (1) Die NixOS-Firewall des Laptops blockt eingehendes UDP –
  `networking.firewall.allowedUDPPorts = [ 20808 ]` und `allowPing = true`
  öffnen Link (der Port deckt auch das Multicast 224.76.78.75:20808 ab).
  (2) Link sucht Netzwerk-Interfaces nur beim Start: Läuft die App schon,
  bevor der Hotspot an ist, fehlt die Multicast-Gruppe auf `ap_br_wlan2`
  (prüfbar mit `cat /proc/net/igmp`, Eintrag `4B4E4CE0` = 224.76.78.75) –
  der Knopf „Link neu starten“ im Session-Tab behebt das.
  Mit MulticastLock, Firewall-Ausnahme und Neustart nach Hotspot-Wechsel:
  `peers=1`, das Tempo wird übernommen.
- **Camera2 braucht eine Java-Bruecke**: Die Pflicht-Callbacks
  (`CameraDevice.StateCallback`, `CaptureSession.StateCallback`,
  `ImageReader.OnImageAvailableListener`) lassen sich nicht in reinem JNI
  erzeugen. `android/app/src/main/java/org/jamcore/app/CameraBridge.java`
  haelt Kamera/Session/ImageReader und ruft je Frame die native Methode; Gradle
  kompiliert `src/main/java` automatisch. **Falle:** `find_class` findet
  App-Klassen aus per `attach_current_thread` angehaengten Threads nicht
  (Bootstrap-ClassLoader, „Java exception was thrown") – ueber
  `activity.getClassLoader().loadClass("org.jamcore.app.CameraBridge")` holen.
- **AudioRecord-Aufnahme**: Die Konstruktoren sind per Greylist gesperrt —
  fünf Argumente geben „Invalid number or type of arguments", sechs
  `NoSuchMethodError: <init>(IIIIII)V`. Es geht nur über
  `AudioRecord$Builder` (`()V`, dann `setAudioSource`/`setBufferSizeInBytes`
  mit `(I)Landroid/media/AudioRecord$Builder;`, `setAudioFormat` mit einem
  `AudioFormat$Builder`-Objekt, dann `build()`).
  `getState()`: **`STATE_INITIALIZED` ist 1** (nicht 3!) — mit dem falschen
  Wert verwirft man funktionierende Quellen.
- **Mikrofonquellen**: `VOICE_RECOGNITION` nimmt nur 16000 Hz, `MIC` auch
  44100. Im Code werden die Kombinationen durchprobiert und geloggt.
- **Pegel ist alles**: Bei Pegel < 0,1 liefert die BPM-Schätzung Unsinn
  (120-BPM-Spur → 135,7), bei Stille gar nichts. Für Tests die Quelle laut
  genug machen (Pegel > 0,5 ist exakt); die App zeigt Pegel und Konfidenz.
- **Layout-Falle bei Gerätetests**: Die Button-Zeilen im Tempo-Tab wandern je
  nach Zustand (mit Pegel-Zeile ≈ 40 px tiefer). Vor dem Tap immer einen
  Screenshot machen und die Koordinate daraus ableiten.
- **libopenmpt**: Git-Baum hat kein CMake → `build/android_ndk/Android.mk` +
  `ndk-build`; Pfad darf keine Leerzeichen enthalten; `APP_ABI`/`APP_PLATFORM`/
  `APP_STL` nur per eigener `Application.mk`.
- **Linken**: `libc++_shared.so` per `cargo:rustc-link-arg` ans **Ende** der
  Linkzeile; Suchpfad muss das **versionierte** API-Verzeichnis sein, sonst
  löst `-lc`/`-lz` statisch auf und die App stürzt beim Laden ab.
- **libopenmpt-FFI**: `openmpt_module_read_float_stereo` (nicht `read_stereo`,
  das ist int16), `openmpt_module_get_metadata(mod, key)` hat zwei Argumente.
- **Android-Crate nur per `cargo ndk` baubar**: `cargo build --manifest-path
  android/Cargo.toml` bricht in `ndk-sys` mit „ndk-sys only supports compiling
  for Android" ab. Für einen Compile-Check immer
  `bash android/build-apk.sh` (ohne `--install` reicht).
- **egui 0.36 (eframe 0.36)**: `egui` ist kein direkter Dependency, sondern nur
  über `use eframe::egui` erreichbar. `allocate_painter` liefert
  `(Response, Painter)`; `rect_filled(rect, radius, farbe)` und
  `rect_stroke(rect, radius, stroke, StrokeKind)` brauchen das Rect als erstes
  Argument; `Rect::grow` und `RichText::tiny` gibt es nicht.
- **16 Spalten passen nicht aufs Handy**: die Breite ist ~317 pt, deshalb
  Blöcke à 8 Schritten (Raster in `android/src/rhythm.rs`).
- **Format-Strings und negative Zahlen**: `format!("-{octave}")` mit `octave = -1`
  ergibt `"--1"` (das `-` ist Text, der Wert bringt sein eigenes Minus mit).
  Bei der Oktave einfach `octave.to_string()` verwenden.
- **Ableton-Link auf Android**: Das Link-SDK kennt Android nicht als Plattform
  (nur Windows/macOS/Linux; kein `LINK_PLATFORM_ANDROID`). Nötig sind
  `LINK_PLATFORM_LINUX=1` und ein `htonll`-Shim (Bionic hat es nicht), gesetzt
  über `CMAKE_PROJECT_INCLUDE` aus `android/link_shim/` – ein Erweitern von
  `CMAKE_CXX_FLAGS` in der Toolchain-Datei wird von der cmake-Kiste
  überschrieben. `CMAKE_TOOLCHAIN_FILE` muss als Umgebungsvariable gesetzt
  sein (macht `build-apk.sh`). Nach Toolchain-Änderungen
  `android/target/<triple>/release/build/rusty_link-*` löschen, sonst lädt
  CMake die Toolchain nicht nach (Cache). Für Peers fehlt noch ein
  `WifiManager.MulticastLock` (Java), sonst kommen Multicast-Pakete nicht an.
- **Clippy in der Nix-Shell**: nix liefert rustc/cargo 1.97, der rustup-Shim
  `clippy-driver` ist 1.99 → `E0514 found crate compiled by an incompatible
  version of rustc`. Workaround: `~/.rustup/toolchains/stable-*/bin` vorn in den
  PATH und `CARGO_TARGET_DIR=/tmp/opencode/clippy-target`.
- **Hyprland-Config** `~/.config/hypr/hyprland.lua`: snake_case-Schlüssel
  (`dpms_timeout`, nicht `dpmsTimeout`), kein Git → vorher Backup anlegen.
- **Bildschirm wurde komplett schwarz** (Laptop lief weiter, DPMS und Backlight laut
  sysfs an, kein Idle-Daemon, `general:dpms_timeout` gibt es in Hyprland 0.56
  nicht): PSR/Panel-Problem, Fix `amdgpu.dcdebugmask=0x10` in
  `boot.kernelParams` (2026-10-06 gesetzt). Helligkeit stand zudem bei 6 %
  (`brightnessctl --device=amdgpu_bl1 set 80%`).
- **Stromprofil**: stand wieder auf `balanced` (78 °C, Lüfter laut).
  `powerprofilesctl set power-saver` → 31–40 °C.

## Zustand

Siehe `docs/plans/2026-09-30-android-port.md` (S1–S6 fertig, S2b-Spike bestanden, auf
Gerät verifiziert; S7–S12 offen) und `git log`.