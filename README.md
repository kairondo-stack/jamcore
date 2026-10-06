# JamCore

Werkzeuge für Jam-Sessions: Desktop-Tools in Rust und eine Android-App
(Rust-Kern + NativeActivity, kleine Java-Brücken für Kamera, Video und
Mikrofon). Module abspielen, Tempo messen oder tippen, Rhythmen ins Raster
spielen (Rendern/Aufnahme), Kamera-Farbscan mit Live-Scan und Palette,
Quintenzirkel, Takt-Maske mit Wischprofilen, Tagebuch, OSC zu Renoise und
Ableton-Link-Synchronisation.

## Aufbau

| Verzeichnis | Inhalt |
|---|---|
| `core/` | Hardwarefreie Logik (Raster, YUV, Scan, Tempo, Session, Maske, Tagebuch); `cargo test --manifest-path core/Cargo.toml` |
| `src/`, `rrn5/`, `pattern_recorder/`, `imgsh/`, `bundle_udp/` | Desktop-Werkzeuge |
| `android/` | Android-App (`cargo ndk` + Gradle); Java-Brücken in `android/app/src/main/java/org/jamcore/app/` |
| `docs/plans/` | Plan und Stand (S1–S12) |
| `tests/e2e/` | Desktop-End-to-End-Skripte (`bash tests/e2e/run_all.sh`) |

## Bauen (NixOS, alles in der Nix-Shell)

```sh
nix-shell
TMPDIR=/tmp/opencode RUSTC_WRAPPER= cargo test --manifest-path core/Cargo.toml
TMPDIR=/tmp/opencode RUSTC_WRAPPER= bash tests/e2e/run_all.sh
```

Android (arm64-v8a, installiert und startet):

```sh
bash android/build-apk.sh --install
```

Release (signiert, Keystore siehe unten):

```sh
cd android && gradle --no-daemon assembleRelease
# APK: android/app/build/outputs/apk/release/app-release.apk
```

## Release-Keystore

`android/release.keystore` (Alias `jamcore`, Passwort `jamcore-release`) ist
**nicht** im Git und muss getrennt gesichert werden — ohne ihn lassen sich
keine Updates derselben App installieren. Das Debug-APK nutzt den
Standard-Debug-Keystore; Release und Debug haben verschiedene Signaturen
(vor dem Wechsel die App deinstallieren; App-Daten liegen unter
`/sdcard/Android/data/org.jamcore.app/files/`).

## Arbeitsweise

Vor Fehlersuche, neuen Features und Fertigmeldungen gelten die Regeln in
`AGENTS.md` (Skills, Subagenten, Verifikation auf dem Gerät). Der aktuelle
Stand steht in `docs/plans/2026-09-30-android-port.md`.
