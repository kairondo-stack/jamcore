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
- APK: `bash android/build-apk.sh --install`.
- Desktop-Nix-Pakete (Kopie): `~/jam-nix`, Start in `Start.txt`, `./sync.sh` zum
  Aktualisieren, `./update-hashes.sh` nach Cargo-Änderungen.

## Gerätetests (Android)

- `adb shell input tap <x> <y>`; Screenshot ist 872x1968 skaliert, Gerät
  1080x2410 → x·1.2385, y·1.2246.
- Tabs: Überblick/Player/OSC liegen bei y≈180 (x: Überblick 265, Player 402, OSC 540).
- `adb logcat -d | grep jamandroid` ist die Quelle der Wahrheit; Bildschirmfotos nur
  zur Kontanz.
- `adb reverse tcp<port> tcp<port>` für Ports vom Handy zum Laptop.

## Fallen, die schon Zeit gekostet haben

- **Android 17 (Pixel 10 Pro)**: AAudio-C-API ist ein Stub (`nullptr`),
  `SL_IID_*` von libOpenSLES unbrauchbar, `AudioTrack.write(float[],…)` fehlt.
  Funktioniert: AudioTrack + `write(byte[],…)`, PCM 16-Bit.
- **libopenmpt**: Git-Baum hat kein CMake → `build/android_ndk/Android.mk` +
  `ndk-build`; Pfad darf keine Leerzeichen enthalten; `APP_ABI`/`APP_PLATFORM`/
  `APP_STL` nur per eigener `Application.mk`.
- **Linken**: `libc++_shared.so` per `cargo:rustc-link-arg` ans **Ende** der
  Linkzeile; Suchpfad muss das **versionierte** API-Verzeichnis sein, sonst
  löst `-lc`/`-lz` statisch auf und die App stürzt beim Laden ab.
- **libopenmpt-FFI**: `openmpt_module_read_float_stereo` (nicht `read_stereo`,
  das ist int16), `openmpt_module_get_metadata(mod, key)` hat zwei Argumente.
- **Hyprland-Config** `~/.config/hypr/hyprland.lua`: snake_case-Schlüssel
  (`dpms_timeout`, nicht `dpmsTimeout`), kein Git → vorher Backup anlegen.
- **Stromprofil**: stand wieder auf `balanced` (78 °C, Lüfter laut).
  `powerprofilesctl set power-saver` → 31–40 °C.

## Zustand

Siehe `docs/plans/2026-09-30-android-port.md` (S1–S3 fertig und auf Gerät
verifiziert) und `git log`.