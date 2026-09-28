# Default Project — Tracker/Renoise-Werkzeuge

Rust-Ports der C++-Originale aus `~/Documents/cde` plus Live-Modul-Renderer.

## Crates

| Crate | Zweck |
|---|---|
| `it_live` | IT/Modul-Renderer (libopenmpt): `--dump`, `--wav` (offline) oder Live-Audio (cpal, `--device NAME`, `--seconds N`) mit taktkompatiblen Order-Wechseln |
| `rrn5` | Bild/Farb-Werkzeug: Farbmischmengen aus Bildern zu Noten, OSC an Renoise (7000/7001/7002), Clipboard-Export, MyImageShm lesen/schreiben, Bildverzeichnis per `--dir`/`FRAMES_DIR`, Kamera (`Z`) / Videodatei (`--video`, OpenCV), Standbild bei Pixelauswahl, `--shm every|throttle|off` (`8`) |
| `pattern_recorder` | Rhythmus-Aufnahme, CSV speichern/laden, Graph-Fenster, Renoise-PatternClipboard-Export/-Import, Schism-Export, Mikrofon-BPM (T+L live, T+M messen, T+Enter übernehmen), OSC (`RENOISE_OSC_PORT`, Default 7005) |
| `imgsh` | Bildbetrachter, schreibt Bilder in `MyImageShm` (von rrn5 mitgenutzt) |
| `bundle_udp` | OSC-Relay 7000/7005 → Ziele (`OSC_TARGETS`, Hostnamen erlaubt; Lauschports via `OSC_LISTEN_PORTS`) |

## Build & Tests

```sh
nix-shell --run "cargo build"                                # alle Crates
nix-shell --run "cargo test --manifest-path pattern_recorder/Cargo.toml"
nix-shell --run "cargo test --manifest-path rrn5/Cargo.toml"
nix-shell --run "bash tests/e2e/run_all.sh"                  # Unit-Tests + 8 E2E-Skripte
```

`shell.nix` liefert Rust, SDL2, libopenmpt, alsa-lib, xclip/xsel, xdotool und python3.

## E2E

Unter `tests/e2e/` liegen Skripte, die unter `Xvfb` echte Prozesse starten und
Artefakte prüfen (WAV-Signal, Shared Memory, OSC-Mitschnitt, CSV/XML/Clipboard,
Live-Audio auf einem ALSA-Null-Device). Für die Tastatur-/Maussteuerung der
SDL-Apps gibt es den Test-Hook `E2E_KEYS` (wird nur bei gesetzter
Umgebungsvariable aktiv):

```
E2E_KEYS="Tab,Right,Quit"                    # Tasten
E2E_KEYS="G,0,Return,5,Return,click:100:250,F,V,0,Quit"
E2E_KEYS="R,1,wait:150,1,wait:150,0"         # wait:<ms> für Timing
```

## Workflow (Kurz)

1. Rhythmus in `pattern_recorder` aufnehmen (`R`, `1`, Ziffern, `0`), mit
   `J`+`1-8` in ein Pattern legen, `S`+`1-8` als `pattern N.csv` speichern.
2. `rrn5` lädt Bildverzeichnis (`--dir`), Farbmischmengen per Maus, Noten per
   OSC an Renoise oder als PatternClipboard-XML ins Clipboard (`g`).
3. `pattern_recorder` importiert Renoise-Clipboard (`Z`) und exportiert nach
   Renoise (`S`, `Down`, `Left`) oder Schism (`S`, `Down`, `Right`).
