# Funktionen der Android-App (JamCore)

Stand: 2026-10-08 · Debug-APK 30,9 MB · Kern-Tests 132 grün · auf Pixel 10 Pro (Android 17)
geräteseitig verifiziert.

Die App ist die Portierung der Desktop-Werkzeuge (siehe
`docs/funktionen-vor-der-portierung.md`) auf Android: egui-Oberfläche in Rust,
Audio/Kamera/Multicast über Java-Brücken (`android/app/src/main/java/org/jamcore/app/`),
hardwarefreie Logik im Crate `jamcore` (`core/`).

**Bauen/Installieren:** `bash android/build-apk.sh [--install]` — macht `cargo ndk`
(arm64-v8a, Platform 26) und Gradle, legt `android/app/build/outputs/apk/debug/app-debug.apk`
ab; mit `--install` zusätzlich `adb install -r` und Start der Activity
`org.jamcore.app/android.app.NativeActivity`.

---

## Tabs (Reihenfolge in der Kopfzeile)

| # | Tab | Funktion |
|---|---|---|
| 1 | **Überblick** | Kamera an/aus, Frame-Zähler und -Größe, gefundene Module, „Module neu suchen", „Zum Player" |
| 2 | **Player** | Modul-Liste (`.it/.xm/.mod/.s3m/.mtm`) aus den Suchordnern; Abspielen/Stopp/Dauerschleife; Peak, Position, Order/Row, Frames |
| 3 | **OSC** | Ziel-IP/Port (gilt für OSC **und** Tempo), Tempo-Slider 30–300 BPM + „Tempo senden", Verbinden/Trennen, Notenbuttons 48/60/64/67/72, Transport Start/Stop, BPM 128, Pattern-Follow an/aus |
| 4 | **Rhythmus** | Step-Editor: 8 Slots, 16 Schritte in zwei 8er-Blöcken, Reihen 36/48/60/72; Follow am Modul oder Eigenuhr; Rendern (Loop, OSC + lokaler Klang), Aufnahme-Modus, Transport-Mit, Transformationen, Skalenfilter, Presenter, CSV/XML-Export |
| 5 | **Tempo** | Mikrofon-BPM: „Messen (4 s)", „Live", „Stop", „Tap", „Übernehmen" (sendet BPM und setzt das App-Tempo); Pegel, Konfidenz, Samplerate, Fenster; Ziel kommt aus dem OSC-Tab |
| 6 | **Bild** | Kamera oder Videos aus `videos/` als Quelle; Live-Scan (Spalten-Sweep → OSC-Noten), Farbauswahl per Finger, Toleranz/Malen/Radieren/Palette, Takt-Maske, Pattern-Ausschnitt; Zwei-Finger-Zoom, Drei-Finger-Verschieben; Querformat mit Bild links und zwei Steuerspalten |
| 7 | **Kreis** | Quintenzirkel-Touchscheibe mit 12 Segmenten; Tippen toggelt und sendet beim Hinzufügen sofort `note_on`; Auswahl ergibt die Notenklassen |
| 8 | **Session** | Ableton-Link-Zustand (BPM, Beat, Peers, Transport), Link an/aus, „Link neu starten", „Tempo folgen", Quantum-Slider |
| 9 | **Tagebuch** | Einträge mit Name/Notiz (übernimmt Raster und Tempo); Liste neueste zuerst, CSV-Ansicht; oben zuklappbar der Bild-Pattern-Export (Renoise-XML kopierbar, CSV) |
| 10 | **Maske** | Takt-Maske-Tastenfeld: jeder Tastendruck belegt den nächsten Schritt; Feldarten „je Wert"/„verteilt"; Wisch zählt nur Anfangs- und Endtaste; Mehrfinger-Eingabe |
| 11 | **Kombi** | Zwei Module gleichzeitig: Hochformat untereinander, Querformat nebeneinander; Auswahl aus leer/Bild/Rhythmus/Maske/Tempo/Session/Tagebuch/Kreis |

Gemeinsam: obere Leiste mit App-Name, Tab-Zeile(n) und Systemleisten-Insets; untere
Statusleiste; das **eine Tempo** (`tempo_aktuell`: Link-Follow gewinnt, sonst OSC-Wert)
gilt für Takt-Scan, Rendern und Tagebuch.

---

## Rust-Module (`android/src/`)

### lib.rs — App-Gerüst
- `Tab`-Enum (11 Tabs), `JamApp` hält alle Panels, `tempo_bpm`, Player, Link-Session
  (`jamcore::rusty_link::AblLink`, Start 120 BPM, aktiviert), `kamera_laeuft`
- `pub fn current_app()` — globaler `AndroidApp`-Zugriff für alle JNI-Aufrufe
- `tempo_aktuell()` — das eine Tempo; `tempo_senden()` schickt `/renoise/song/bpm`
- `android_main` (entry point), `view_*` je Tab, `teil_ui`/`session_block` für Kombi
- holt beim Start das **MulticastLock**, loggt 1×/s Link-Zustand und Kamera-Status,
  fragt Java-Fehlermeldungen ab

### camera.rs — Kamera (Camera2 über Java)
- `start`/`stop` über `CameraBridge` (640×480, YUV_420_888), `stand()` (Frame-Nr., Größe),
  `letztes_bild()` (RGBA-Kopie), `java_fehlermeldung()`
- `ablegen()` rechnet YUV→RGBA über `jamcore::yuv` (auch vom Video-Modul benutzt)
- Frame-Callback `Java_org_jamcore_app_CameraBridge_nativeFrame`

### video.rs — Videodateien als Quelle
- `start(pfad, fps)`, `stop`, `fps_setzen`, `java_meldung` über `VideoBridge`
  (MediaExtractor/MediaCodec, Loop am Stream-Ende)
- gleiche RGBA-Ablage wie die Kamera

### image.rs — Bild-Tab (Live-Scan, Maske, Zoom)
- Quellen: Kamera **oder** Videos aus dem App-Ordner `videos/` (mp4/mkv/webm/avi/mov/3gp),
  fps-Slider 1–30, nur eine Quelle gleichzeitig
- **Live-Scan**: Sweep über die X-Auflösung; je Spalte Treffer → `note_on` (max. 8/s);
  fünf Notenzuordnungen (`note_volume`: Zeile, y, y umgekehrt, Spalte, x umgekehrt)
- **Takt-Maske** (`ScanMaske`, rrn5-`b`): Leiste oben im Bild, Tippen/Wischen schaltet
  Spalten an/aus; `alpha` sperrt Spalten und skaliert Farbe und Lautstärke; `abschnitte`
  fasst gleiche Werte zu Rechtecken zusammen (auch bei 872 Spalten flüssig)
- **X-Auflösung = Schrittzahl = Patternlänge**: ein Schritt je Punkt Breite (Zoom ×),
  „Schritte festhalten" pinnt; „Pattern von…bis" + „ganzes Bild" wählt den Ausschnitt
- **Zoom/Pan**: Zwei-Finger-Zoom 20–400 %, drei Finger verschieben, Bild bleibt im Kasten
- Farbauswahl mit Toleranz (Gesamt + R/G/B), Hintergrundfarbe, Malen/Radieren, Palette-Scan
- Hochformat einspaltig (ScrollArea), Querformat Bild links + zwei Steuerspalten

### maske.rs — Maske-Tab (Tastenfeld)
- Feldarten `JeWert` (0..wert_max) und `Verteilt`; Spalten 2–16, Reihen 2–8, Wert max 8–128
- **jeder Tastendruck belegt den nächsten Schritt**; ein Wisch zählt nur Anfangs- und
  Endtaste; gehaltene Taste + zweite Taste = zwei Schritte hintereinander (Mehrfinger)
- Regenbogen-Spur der gedrückten Tasten, Schrittanzeige mit Notenpunkten, „leeren" leert
  das Pattern
- **Tippfolge für die Scan-Maske**: der erste Druck auf ein Feld beginnt die Aufnahme (es
  gibt keinen Start-Knopf), jeder weitere Druck ist ein Tipp mit Zeitstempel, „Ende" hält die
  Maske fest. Spalte = Zeit / letzter Zeit × Schrittzahl — genau die Rechnung des
  pattern_recorder (`scldiv` = letzter Zeitstempel, Zeile 2253–2256; KeyEventRender-Writer
  ab Zeile 4888). Der erste Tipp liegt immer auf Spalte 0, weil dort der Timer beginnt.
  Die Wischlogik gilt dabei unverändert: ein Wisch liefert zwei Schritte **und** zwei Tipps.

### rhythm.rs — Rhythmus-Tab
- Step-Editor mit 8 Slots; Tipp setzt Note (lokal `klang::spielen` + OSC), „Aufnahme"
  legt Tipps auf den laufenden Render-Schritt
- „Rendern" (Hintergrund-Thread „jam-render"): 16tel-Loop, `note_on`, optional
  Renoise-Transport (`transport_mit`)
- Follow am Modul (`set_follow_step`) oder Eigenuhr; `set_tempo` bekommt das App-Tempo
- Transformationen (`transponieren`, `umkehren`, `quantisieren`, `glide`, `instrument`),
  Skalenfilter (Moll/Dur/Pentatonik), Grundton, Presenter-Ansicht
- Dateien: CSV sichern/laden, XML schreiben nach `…/patterns/pattern N.csv|xml` (atomar)

### player.rs — Modul-Player
- `Player::play(path, looping, 44100)` lädt über `jamcore::openmpt` und schreibt über
  `AudioTrack` (PCM16 stereo, MODE_STREAM, JNI); Thread „jam-audio", Blöcke à 2048 Frames
- `frames`, `peak`/`take_peak`, `position_seconds`, `order_row`, `finished`,
  `grid_step(steps_per_beat)` (Modulzeile als 16er-Schritt für Follow), `stop`
- Warum AudioTrack: auf Android 17 sind AAudio (Stub) und OpenSL ES unbrauchbar

### mic.rs — Tempo-Tab (Mikrofon-BPM)
- `Recorder::start` probiert drei Quellen/raten-Kombinationen (VOICE_RECOGNITION/16000,
  MIC/44100, MIC/16000) über `AudioRecord$Builder` (direkte Konstruktoren sind gesperrt),
  prüft `getState() == 1`; Thread „jam-mic"
- `level()` (Peak, BPM, Konfidenz, Fenster, Rate, error, finished) über Atomics
- `MicPanel`: „Messen (4 s)" (nur frisches Fenster + Konfidenz > 0,3), „Live", „Stop",
  „Tap" (`bpm_aus_taps`), „Übernehmen" (`uebernommenes_tempo()` → App-Tempo), Zielanzeige

### osc.rs — OSC-Tab
- `OscPanel` mit `host`/`port` (öffentlich), `last`, `error`
- `connect`, `disconnect`, `note`, `start`, `stop`, `bpm`, `pattern_follow`, `raw`
- alle Renoise-Adressen über Basisport 7000 (`jamcore::osc`)

### quint.rs — Kreis-Tab
- 12 Annulus-Segmente (`jamcore::quint`); Tippen toggelt, sendet `note_on` und zeigt die
  Notenklassen; „Auswahl löschen"

### session_ui.rs — Session-Tab
- zeigt BPM, Beat → nächster Rasterpunkt + Wartezeit (`naechster_beat`,
  `wartezeit_bis_beat`), Peers, Transport
- Aktionen: `LinkUmschalten(bool)`, `LinkNeustart` (Interface-Suche nur beim Start);
  „Tempo folgen" (`tempo_folgen`), Quantum 1–16

### diary.rs — Tagebuch-Tab
- Einträge anlegen/listen/speichern; je Eintrag eine Textdatei in `…/tagebuch/`
  (atomar über tmp + rename), Noten im Desktop-CSV-Format (`jamcore::diary`, `grid`)
- Anzeige: Tempo, Notiz, Datei, CSV; neueste zuerst

### klang.rs — lokaler Klang
- kurze Sinus-Töne (`jamcore::klang`) über `AudioTrack`, Thread „jam-klang" mit
  Warteschlange (120 ms, 44100 Hz, beide Kanäle), nicht blockierend

### layout.rs — Kombi-Layout
- `KombiTeil` (Leer/Bild/Rhythmus/Maske/Tempo/Session/Tagebuch/Kreis),
  `Anordnung` (ab 560 pt Breite und breiter als hoch → nebeneinander)
- `wahlzeile` (Oben/Unten bzw. Links/Rechts), `bereiche` (Stapel mit ScrollAreas bzw.
  `Panel::left` + `CentralPanel`); 4 Unit-Tests

### touch.rs — Mehrfinger-Adapter
- liest `egui::Event::Touch` (egui liefert auf Android kein `Event::Zoom`) und füttert
  `jamcore::gesten::Gesten`; liefert Zoom/Pan sowie Start/Zug/Ende je Finger
- beim Loslassen wird nicht mehr gezoomt

### insets.rs / jniutil.rs — Android-Fundament
- `root_insets_px`/`status_bar_px` (Systemleisten), `external_files_dir`
- `with_env` (Thread an die JVM hängen), `call_object*`, `jstring`, Ausnahmen räumen

### link_support.rs — MulticastLock
- `aktivieren`/`freigeben` über `LinkSupport.java`; ohne Lock sieht Ableton Link im WLAN
  keine Peers

### paths.rs — Modulsuche
- Endungen `it/xm/mod/s3m/mtm`; Suchordner: `…/module`, Basisordner, `/sdcard/Download`,
  `/sdcard/Documents` (Desktop-Fallback: `~/Downloads`, `~/Documents`, `.`)

---

## Java-Brücken (`android/app/src/main/java/org/jamcore/app/`)

| Datei | Zweck |
|---|---|
| `CameraBridge.java` | Camera2: Gerät, CaptureSession, ImageReader (640×480, YUV_420_888); HandlerThread „jam-camera"; ruft je Frame `nativeFrame(y/u/v, strides, w, h)`; meldet Fehler über `status()` |
| `VideoBridge.java` | MediaExtractor + MediaCodec; dekodiert synchron im Thread „jam-video", ruft `nativeVideoFrame(...)`, loop am Ende (`seekTo(0)` + `flush`) |
| `LinkSupport.java` | `WifiManager.MulticastLock` `acquire`/`release` für Ableton Link |

---

## Was nur mit Java/JNI funktioniert

- **Kamera** (`camera.rs`): die Pflicht-Callbacks (`CameraDevice.StateCallback`,
  `CaptureSession.StateCallback`, `ImageReader.OnImageAvailableListener`) lassen sich in
  reinem JNI nicht erzeugen
- **Video** (`video.rs`): MediaExtractor/MediaCodec sind Java-APIs
- **Mikrofon** (`mic.rs`): `AudioRecord`-Konstruktoren sind Greylist-gesperrt (nur
  `Builder`), AAudio C-API ist auf Android 17 ein Stub (`nullptr`)
- **Player/Klang** (`player.rs`, `klang.rs`): nur `AudioTrack.write(byte[]/float[])` läuft;
  `SL_IID_*` von libOpenSLES ist unbrauchbar
- **MulticastLock** (`link_support.rs`): Java-API
- **Insets/Pfade** (`insets.rs`): `WindowInsets`, `getExternalFilesDir` sind Java-APIs
- **Klassen laden**: App-Klassen nur über den Activity-ClassLoader (`find_class` scheitert
  an angehängten Threads)

---

## Dateien auf dem Gerät

Basis: `/storage/emulated/0/Android/data/org.jamcore.app/files/`

| Ordner | Inhalt | Modul |
|---|---|---|
| `module/` | Module für den Player | paths.rs |
| `videos/` | Videoquellen für den Bild-Tab | image.rs |
| `patterns/` | `pattern N.csv` / `.xml` | rhythm.rs |
| `tagebuch/` | `JJJJ-MM-TT hh_mm.txt`-Einträge (+ `video=`-Verweis) | diary.rs |

---

## Grenzen/Notizen

- OSC läuft über **UDP**; `adb reverse` kann nur TCP — Handy und Laptop müssen im selben
  Netz sein, Ziel-IP im OSC-Tab. Die IP wird **nicht** gespeichert (Neustart = 127.0.0.1)
- Ports in der NixOS-Firewall freigeben: UDP 7000–7010 und 20808
- Link: `MulticastLock` + Firewall + „Link neu starten" nach Netzwechsel
- Log-Target ist der Modulname (`jamandroid::<modul>`) — Meldungen sollen ihn nicht
  wiederholen
- Kern-Tests: `nix-shell --run "RUSTC_WRAPPER= TMPDIR=/tmp/opencode cargo test --manifest-path core/Cargo.toml"`
  (132 Tests); Android-Crate nur per `cargo ndk`/`build-apk.sh` baubar
