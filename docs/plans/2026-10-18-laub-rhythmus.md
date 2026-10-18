# Plan 2026-10-18: Laub-Rhythmus (Foto → Blätter → Genre-Rhythmus)

Stand: 2026-10-18 · Nutzeridee (`~/Documents/Neue Idee.txt`): Laubblätter auf einem
Weg fotografieren/filmen, **lokal** erkennen, **Abstände** messen, ein **Genre**
vorgeben und daraus einen **Rhythmus** rendern; Blattfarben → Instrumente.
Abgestimmt: **Aufnahme → Analyse** (kein Live), Abstand→Schritt **relativ oder
absolut umschaltbar**, Farben → 4 Instrumente (wählbar) **oder** ein Instrument
mit Farbe = Lautstärke/Tonhöhe. Genres: **Techno, Groove, Breakbeat, Drum and
Bass**; Raster **96/128/192/256 Schritte pro Takt** (Pattern bis 1024 Schritte).

## Step 1: Kern – Blätter finden (TDD)

- Goal: Blob-Erkennung auf einem Bild (RGB): Blätter = Flecken, die sich vom Weg
  (Hintergrund) abheben; liefert je Blatt Mittelpunkt, Größe (Fläche) und
  Mittelfarbe; alles ohne ML (Farbschwelle + Zusammenhangskomponenten).
- Files: `core/src/laub.rs` (neu), `core/src/lib.rs`.
- API-Skizze:
  `pub struct Blatt { pub x: f32, pub y: f32, pub flaeche: u32, pub r: u8, pub g: u8, pub b: u8 }`,
  `pub fn blaetter(frame: &[u8], w: i32, h: i32, hintergrund: (u8,u8,u8), toleranz: u8) -> Vec<Blatt>`,
  `pub fn sortiere_wegrichtung(blaetter: &mut [Blatt])` (nach y, dann x),
  `pub fn farbklasse(b: &Blatt) -> u8` (0…3: rot/gelb/braun/grün).
- Test: `vier_flecken_werden_gefunden`, `hintergrund_zaehlt_nicht`, `flaeche_und_farbe`,
  `sortierung_nach_wegrichtung`, `farbklassen`.
- Verify: `cargo test --manifest-path core/Cargo.toml laub` → grün.
- Depends on: —

## Step 2: Kern – Abstände → Genre-Rhythmus (TDD)

- Goal: Blätter → Schrittpositionen im gewählten Raster, mit Genre-Vokabular.
- API-Skizze:
  `pub enum Abstandsmodus { Relativ, Absolut { geschwindigkeit_m_s: f32, schrittlaenge_m: f32 } }`,
  `pub struct Genre { pub name: &'static str, pub vokabular: &'static [u32] }` (typische Schrittintervalle),
  `pub fn genre(name: &str) -> Option<Genre>`,
  `pub fn schritte_aus_abstaenden(d: &[f32], raster_pro_takt: u16, modus: Abstandsmodus, genre: &Genre) -> Vec<u32>`,
  `pub fn rasterwahl() -> [u16; 4]` = [96,128,192,256].
- Vokabulare (Vorschlag, dokumentiert): Techno 4-on-the-floor (Intervall = Raster/4),
  Groove mit Swing-Offsets, Breakbeat 16tel/32tel-Splits, Drum and Bass schnelle
  16tel-Läufe + Synkopen. Werte in Schritten des Rasters.
- Test: `relativ_kleinster_abstand_ist_ein_schritt`, `absolut_rechnet_mit_gehen`,
  `genre_rundet_auf_vokabular`, `raster_skaliert`, `leere_und_kaputte_werte`.
- Verify: `cargo test --manifest-path core/Cargo.toml laub` → grün.
- Depends on: Step 1

## Step 3: App – Aufnahme, Analyse, Rendern

- Goal: Kamera-Foto aufnehmen (oder Bild laden), Blätter markieren, Abstände
  anzeigen, Genre + Raster + Modus wählen, „als Rhythmus rendern" ins Raster.
- Files: `android/src/laub.rs` (neu: Ansicht), `android/src/rhythm.rs` (Noten
  setzen wie Euklid), `android/src/lib.rs` (Tab „Laub"), `core/src/laub.rs` (nur
  lesen).
- Verify: Handy/Emulator: Foto (oder Testbild per `adb push`) → Blätter markiert,
  Abstände sichtbar, Rendern setzt Noten; Logs für den Beleg.
- Depends on: Step 2

## Step 4: Desktop (später)

- rrn5/pattern_recorder: Foto laden → Blätter → Rhythmus (gleiche Kern-Funktionen).

## Befunde aus dem Bildschirmvideo (2026-10-18, 8:02 min, 1080×2410, 30 fps, **ohne Ton**)

- Inhalt: durchgehend der **Bild-Tab**, der Nutzer arbeitet an **Farbauswahl +
  Toleranz** (5 Auswahlfarben, Toleranz 24…75); der **Sweep lief nie**
  („0 Noten, 0 gezeichnet", Ton aus).
- **Kein Rhythmus im Video**: kein Audio-Stream, Helligkeit *und* Bewegung mit
  monoton fallender Autokorrelation (keine Periode), grüne Leiste statisch.
  Nur 10 Szenenwechsel (die zwei Kamera-Neustarts) und 57 Bewegungsereignisse —
  taugen als **One-Shot-Trigger**, nicht als Tempo.
- **Der belastbare Träger ist räumlich**: Objekt-Abstände (Blob-Erkennung)
  47–105 px → bei 128 BPM und „ganzes Bild = 512 Schritte = 1 Takt"
  **86–193 ms ≈ 1/16 … 1/8-Note** — musikalisch direkt verwertbar. Rohe Textur
  (11–42 px → 20–77 ms) ist zu fein. Stützt den Laub-Plan (Blob statt FFT).
- **Falle 1 — Vorschau 90° verdreht**: es gibt **keine** Drehung in
  `CameraBridge.java`/`camera.rs`; der Weg liegt dadurch auf der **y-Achse**.
  Für „Abstände entlang des Wegs" also y messen (oder Drehung nachrüsten).
- **Falle 2 — Screenrecorder ohne Ton**: für Audio-Onsets muss mit Ton
  aufgenommen werden (die App selbst kann noch kein Video aufnehmen).

## Step 5: Tempo aus Video (stummes Video → BPM) — mit eingeplant

- Goal: Aus einem **Video ohne Ton** das Tempo lesen: die App zeichnet ihre
  Sweeps/Stroboskop/Spiralen im Takt; aus der **Periodizität der Bildinhalte**
  (Helligkeits-/Bewegungs-Zeitreihe je Frame) wird die BPM geschätzt und als
  zentrales Tempo übernommen. Nützlich unabhängig vom Laub-Thema.
- Files: `core/src/videotempo.rs` (neu; Muster/Vorbild: `core/src/micbpm.rs`,
  das dieselbe Schätzung aus einer Audio-Hüllkurve macht), `core/src/lib.rs`,
  `android/src/image.rs` oder `android/src/slicer.rs` (Knopf „Tempo aus Video",
  nutzt die vorhandene Video-Dekodierung `android/src/video.rs`), `lib.rs`
  (`tempo_setzen`).
- Test (Kern, synthetisch): `sinus_mit_120_bpm_wird_erkannt`,
  `rauschen_ohne_takt_liefert_nichts`, `zu_kurze_reihe`, `fps_und_bpm_skalieren`,
  `halbzeit_und_doppelzeit` (Oktav-Fehler benennen).
- Verify: `cargo test --manifest-path core/Cargo.toml videotempo` → grün;
  Gerätetest: das vorliegende Bildschirmvideo laden → erkannte BPM im Log,
  Vergleich mit dem im Video sichtbaren Tempo.
- Depends on: — (unabhängig von Step 1–4)

## Rollback

Additiv: neuer Kern-Modul + neuer Tab; nichts Bestehendes wird umgebaut.
