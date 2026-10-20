//! Laub-Tab (Plan `docs/plans/2026-10-18-laub-rhythmus.md`, Step 3):
//! Foto aufnehmen, Bild **oder Video** laden → Blätter finden → Abstände
//! anzeigen → Genre, Raster und Modus wählen → „als Rhythmus rendern“ schreibt
//! die Schrittnummern ins Rhythmus-Raster; „Anhören“ spielt dieselben Schritte
//! vorher in Schleife, ohne das Pattern zu verändern.
//!
//! Das Rechnen liegt im Kern (`jamcore::laub`, Step 1+2); hier stehen
//! Aufnahme, Anzeige, Bedienung und der Weg ins Raster
//! ([`crate::rhythm::RhythmPanel::laub_setzen`]). Das Standbild liegt als RGB8
//! in [`LaubPanel::bild`] (die Kamera liefert RGBA), die Anzeige als Textur.
//!
//! # Vorschau
//!
//! Läuft die Kamera und ist kein Standbild eingefroren, zeichnet der Tab das
//! neueste Kamerabild mit denselben Zoom/Pan-Regeln wie das Standbild.
//! „Foto aufnehmen“ friert ein, „Live“ schaltet zurück. Kopiert wird nur bei
//! neuer Framenummer ([`LaubPanel::vorschau_aktualisieren`]) — 1,2 MB je
//! Kamerabild und Anzeigeframe wären sonst reine Verschwendung. Nach einer
//! Sekunde ohne neues Bild gilt die Kamera als aus und das Standbild kehrt
//! zurück, auch wenn sie in einem anderen Tab gestoppt wurde.
//!
//! # Taktgruppen
//!
//! Ein Takt ist die Abstandsfolge **einer** Gruppe aus
//! [`jamcore::laub::gruppiere_abstaende`]. Eine Taktgruppe ordnet mehrere
//! Takte hintereinander an (`G1 G2 G1 G3`), weil die Abstände eines Genres
//! variieren. „Taktgruppe → Patterns“ schreibt die Folge ab Zeile 0 im
//! Abstand von `feinheit` Zeilen; was nicht mehr in den Slot passt, setzt sich
//! im nächsten fort (Ring 8).
//!
//! # Anhören-Marker
//!
//! Beim Anhören schreibt der Thread den gerade klingenden Rasterschritt in
//! [`LaubPanel::spiel_position`]. Die Zeichnung markiert die Blätter dieses
//! Schritts weiß und zeigt unten eine Schrittleiste; Stop setzt 0. Der
//! Status-/Log-Text bleibt dabei unverändert (kein Spam je Schritt).
//!
//! # Video („Video laden …“)
//!
//! Der Videodekoder (`crate::video`, Java-Brücke `VideoBridge`) liefert seine
//! Bilder in denselben Framepuffer wie die Kamera. Statt nur die ersten
//! Sekunden zu prüfen, sieht [`LaubPanel::video_schritt`] (von der JamApp je
//! Anzeigeframe gerufen, auch außerhalb des Tabs) **über die ganze Dauer**
//! verteilt Bilder an: höchstens eines alle 400 ms Wanduhr, bis zu
//! [`VIDEO_FRAMES_MAX`] Stück bzw. [`VIDEO_FRIST_S`] Sekunden. Das Bild mit
//! den **meisten Blättern** gewinnt. Danach wird der Dekoder gestoppt und
//! dieses Bild läuft durch dieselbe Kette wie ein Foto.
//!
//! Warum Wanduhr statt `jamcore::laub::stichproben_zeiten`: die Java-Brücke
//! liefert keine Videodauer, und der Dekoder läuft nach dem Dateiende in
//! Schleife. Die Drosselung braucht beides nicht und tastet ein
//! Echtzeit-Video trotzdem gleichmäßig ab (15 Dekodier-fps genügen für 2,5
//! Proben je Sekunde).
//!
//! Warum der beste **einzelne** Frame und keine Sammlung über die Zeit: beim
//! Filmen bewegt sich die Kamera, dasselbe Blatt liegt in verschiedenen Bildern
//! an verschiedenen Bildpositionen — ein Vereinigen in Bildkoordinaten würde
//! es mehrfach zählen. Ein einzelnes Bild hält die Abstände ehrlich.
//!
//! # Kamera-Sammeln
//!
//! „Sammeln 10 s“ wertet alle 500 ms ein Kamerabild aus (ohne Anzeige) und
//! übernimmt am Ende das Bild mit den meisten Blättern — dieselbe Idee wie
//! beim Video, nur live und ohne Dekoder. Es läuft nur, solange der Laub-Tab
//! sichtbar ist; ein Tab-Wechsel bricht es ab.
//!
//! # Farben → Instrumente
//!
//! Die vier Farbklassen des Kerns (rot/gelb/braun/grün, [`farbklasse`]) werden
//! zu den Instrumentnummern 0…3 — bei der Klangquelle „Slices“ im
//! Rhythmus-Tab sind das die ersten vier Slices. Der Kern liefert nur die
//! **Menge** der Schrittnummern (sortiert, ohne Doppelte); damit die Farbe je
//! Schritt nicht verloren geht, rechnet die App die Position jedes Blattes in
//! [`spiegel_positionen`] nach (Spiegel der Kern-Rechnung) und vergleicht das
//! Ergebnis zur Sicherheit mit dem Kern ([`farben_je_schritt`]). Die **erste**
//! Blattfarbe auf einem Schritt gewinnt — bei einem gefalteten Takt ist das
//! das erste Blatt, das dort ankommt.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, RichText, Stroke};
use jamcore::laub::{
    farbklasse, genre, rasterwahl, schritte_aus_abstaenden_bpm, Abstandsmodus, Blatt, Genre,
};

use crate::camera;

/// Die vier Genre-Knöpfe — Namen wie in `jamcore::laub::genre`.
const GENRES: [&str; 4] = ["Techno", "Groove", "Breakbeat", "Drum and Bass"];

/// Vorgabe-Toleranz (größter Kanalabstand zum Hintergrund, der noch Weg ist):
/// 60 lässt Herbstlaub auf hellem Weg stehen, ohne den Weg selbst zu treffen.
const TOLERANZ_STANDARD: u8 = 60;

/// Vorgabe-Maßstab im absoluten Modus (Pixel je Meter): Das Kamerabild ist
/// 640 Punkte breit; bei einem Foto aus gut 1 m Höhe sind 200 px/m ≈ 3 m Weg.
const PX_PRO_METER: f32 = 200.0;

/// Lange Kante eines geladenen Bildes (wie Kamera und Bild-Tab).
const BILD_KANTE: u32 = 640;

/// Harte Obergrenze ausgewerteter Videobilder. Bei [`VIDEO_ABTAST_MS`] sind
/// das gut 30 s Film — genug, um Blätter auch in der zweiten Hälfte eines
/// Videos zu finden; die Auswertung je Bild kostet auf dem Handy spürbar Zeit,
/// deshalb keine unbegrenzte Sammlung.
const VIDEO_FRAMES_MAX: usize = 80;

/// Bildrate beim Video-Prüfen: 15 fps. Der Dekoder läuft damit ungefähr in
/// Echtzeit; ausgewertet wird trotzdem nur alle [`VIDEO_ABTAST_MS`].
const VIDEO_FPS: u32 = 15;

/// Wanduhr-Abstand zwischen zwei ausgewerteten Videobildern: 400 ms
/// (≈ 2,5 Bilder je Sekunde Film). Der Dekoder liefert 15 fps; dazwischen
/// liegende Bilder werden bewusst übersprungen, sonst kostet ein langes Video
/// Minuten statt Sekunden.
const VIDEO_ABTAST_MS: u64 = 400;

/// Frist ohne ein einziges Bild (Sekunden): kommt in dieser Zeit kein Frame,
/// gilt der Dekoder als hängend und das Prüfen endet mit Meldung.
const VIDEO_START_FRIST_S: u64 = 10;

/// Ende, wenn der Dekoder so lange kein neues Bild mehr liefert (Sekunden).
/// Der Java-Dekoder läuft normalerweise in Schleife; das greift also nur bei
/// einem hängenden oder am Dateiende stehengebliebenen Dekoder.
const VIDEO_ENDE_OHNE_BILD_S: f64 = 1.5;

/// Obergrenze für das ganze Prüfen (Sekunden). Sie deckt auch Videos ab, deren
/// Dauer die App nicht kennt (die Java-Brücke liefert sie nicht): nach 45 s
/// ist Schluss und das beste bisherige Bild wird übernommen.
const VIDEO_FRIST_S: f64 = 45.0;

/// Dauer des Kamera-Sammelns und Zahl der Proben (eine alle 500 ms).
const SAMMELN_DAUER_S: f32 = 10.0;
const SAMMELN_PROBEN: usize = 20;
const SAMMELN_ABSTAND_MS: u64 = 500;

/// Gruppen im Laub-Tab: so viele Abstände bilden einen Takt. 0 wäre „alles
/// ein Takt“; die UI bietet 2/4/8, Vorgabe 4.
const GRUPPEN_VORGABE: usize = 4;
const GRUPPEN_MAX: usize = 8;

/// Wählbare Längen einer Taktgruppe in Takten (ein Pattern-Slot fasst
/// höchstens acht; mehr ergäbe für die Anzeige keinen Sinn).
const TAKTGRUPPE_WAHL: [usize; 4] = [1, 2, 4, 8];

/// Vorgabe-Länge einer Taktgruppe: 4 Takte, eine musikalische Phrase (das
/// Nutzerbeispiel ist `G1 G2 G1 G3`).
const TAKTGRUPPE_VORGABE: usize = 4;

/// Nach so langer Bildstille gilt die Kamera als aus: Die Vorschau holt das
/// Standbild zurück, auch wenn die Kamera in einem anderen Tab gestoppt wurde.
/// Ein Kamerabild kommt sonst etwa 30-mal je Sekunde.
const VORSCHAU_STILL_MS: u64 = 1000;

/// Auffrischung der Live-Vorschau: ~30 fps, wie die Kamera liefert. Ohne
/// diesen Anstoß zeichnet egui nur bei Eingaben und das Bild stünde still.
const VORSCHAU_MS: u64 = 33;

/// Was der Laub-Tab von außen braucht (JamApp zeigt die Fenster).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Aktion {
    Keine,
    /// „Bild laden …“: JamApp öffnet den Datei-Browser (Filter Bilder).
    BrowserBild,
    /// „Video laden …“: JamApp öffnet den Datei-Browser (Filter Videos).
    BrowserVideo,
}

/// Ein laufendes Video-Prüfen: Bilder ansehen, das beste übernehmen
/// ([`LaubPanel::video_schritt`]).
struct Videolauf {
    /// Der geladene Pfad (für Status und Log).
    pfad: String,
    /// Beginn des Prüfens (für die Frist [`VIDEO_FRIST_S`]).
    start: std::time::Instant,
    /// Zuletzt gesehene Framenummer aus [`camera::stand`].
    letzte_nummer: u64,
    /// Wann zuletzt ein **neues** Dekodierbild ankam — daraus folgt das Ende
    /// „seit [`VIDEO_ENDE_OHNE_BILD_S`] kein neues Bild“.
    letzte_bildzeit: Instant,
    /// Wann zuletzt ein Bild ausgewertet wurde (Drosselung auf
    /// [`VIDEO_ABTAST_MS`]).
    letzte_pruefung: Option<Instant>,
    /// Wie viele Bilder geprüft wurden.
    geprueft: usize,
    /// Das bisher beste Bild (die meisten Blätter).
    bester: Option<BestesBild>,
}

/// Ein laufendes Kamera-Sammeln („Sammeln 10 s“): über die Dauer verteilt
/// Bilder auswerten, das beste behalten.
struct Sammellauf {
    /// Beginn des Sammelns (Dauer und Log).
    start: Instant,
    /// Zuletzt ausgewertete Framenummer — dasselbe Bild wird nicht doppelt
    /// gezählt, wenn die Kamera gerade nicht liefert.
    letzte_nummer: u64,
    /// Wann zuletzt eine Probe fiel (Takt von [`SAMMELN_ABSTAND_MS`]).
    letzte_probe: Option<Instant>,
    /// Zahl der ausgewerteten Bilder.
    geprueft: usize,
    /// Das bisher beste Bild.
    bester: Option<BestesBild>,
}

/// Das bisher beste Videobild samt Zählung.
struct BestesBild {
    /// Wie vieltes geprüftes Bild (1-basiert) — „bester Frame 17“.
    index: usize,
    w: usize,
    h: usize,
    /// Das Bild in Analysegröße (lange Kante höchstens [`BILD_KANTE`]).
    rgba: Vec<u8>,
    /// Gefundene Blätter in diesem Bild.
    blaetter: usize,
}

pub struct LaubPanel {
    /// Anzeige-Textur des Standbilds oder der Live-Vorschau.
    texture: Option<egui::TextureHandle>,
    /// Neues Bild für die Textur, bis der nächste `ui`-Aufruf es hochlädt.
    textur_offen: Option<(usize, usize, Vec<u8>)>,
    /// Maße der aktuellen Textur: Livebild und Standbild können verschiedene
    /// Größen haben, Zoom/Pan und Marker rechnen mit diesen Maßen.
    anzeige_masse: (usize, usize),
    /// In der Textur liegt gerade das Livebild (sonst das Standbild).
    textur_live: bool,
    /// Vorschau live (`true`) oder eingefrorenes Standbild (`false`).
    vorschau_live: bool,
    /// Zuletzt in die Textur übernommene Kameraframenummer: nur ein **neues**
    /// Bild wird kopiert, nicht jedes Anzeigeframe (1,2 MB je Kopie).
    vorschau_nummer: u64,
    /// Wann zuletzt ein neues Kamerabild ankam; daraus folgt „Kamera aus“.
    vorschau_bildzeit: Option<Instant>,
    /// Das analysierte Standbild als RGBA — für die Rückkehr aus der
    /// Live-Vorschau, ohne die Analyse-RGB neu umzurechnen.
    standbild_rgba: Option<(usize, usize, Vec<u8>)>,
    /// Das analysierte Standbild als RGB8 (die Kamera liefert RGBA).
    bild: Option<(usize, usize, Vec<u8>)>,
    /// Gefundene Blätter in Wegrichtung (erst y, dann x).
    blaetter: Vec<Blatt>,
    /// Abstände zwischen aufeinanderfolgenden Blättern in Pixeln.
    abstaende: Vec<f32>,
    /// Hintergrundfarbe des Wegs (Tipp ins Bild).
    hintergrund: (u8, u8, u8),
    /// Beim neuen Bild den Hintergrund selbst wählen (bis der Nutzer tippt).
    auto_hintergrund: bool,
    /// Größter Kanalabstand, der noch Hintergrund ist.
    toleranz: u8,
    /// Gewähltes Genre (Index in [`GENRES`]).
    genre: usize,
    /// Gewähltes Raster (Schritte je Takt, `rasterwahl`).
    raster: u16,
    /// Relativer Modus (Vorgabe): nur die Verhältnisse zählen.
    relativ: bool,
    /// Gehgeschwindigkeit im absoluten Modus (m/s).
    geschwindigkeit: f32,
    /// Schrittlänge im absoluten Modus (m).
    schrittlaenge: f32,
    /// Maßstab im absoluten Modus (Pixel je Meter).
    px_pro_meter: f32,
    /// Das eine Tempo der App (JamApp setzt es je Frame).
    bpm: f32,
    /// Zoom im Bild (1.0 = ganze Breite, bis 8.0) und Verschiebung in Punkten.
    zoom: f32,
    pan: egui::Vec2,
    /// Laufendes Video-Prüfen („Video laden …“), sonst `None`.
    videolauf: Option<Videolauf>,
    /// Laufendes Kamera-Sammeln („Sammeln 10 s“), sonst `None`.
    sammeln: Option<Sammellauf>,
    /// Zeitpunkt des letzten `ui`-Aufrufs: bleibt der Tab länger als eine
    /// Sekunde weg, gilt ein laufendes Sammeln als abgebrochen (die JamApp
    /// ruft `ui` nur für den sichtbaren Tab; ein globaler Schritt wie beim
    /// Video steht hier nicht zur Verfügung).
    sammeln_letzte_ui: Instant,
    /// Abstände je Gruppe (2/4/8) — jede Gruppe wird ein Takt/Pattern.
    gruppe_groesse: usize,
    /// Gewählte Gruppe für „Anhören“ (0-basiert, auf die Gruppen geklemmt).
    gruppe_auswahl: usize,
    /// Länge der Taktgruppe in Takten (1/2/4/8).
    taktgruppe_len: usize,
    /// Gruppenindex je Takt der Taktgruppe (0-basiert); wird geklemmt, damit
    /// ein Wechsel der Gruppengröße die Folge nicht ungültig macht.
    taktfolge: Vec<usize>,
    /// Gruppenzahl, für die die Taktfolge zuletzt aufgefüllt wurde. Solange
    /// noch kein Bild erkannt ist (0), sind die Takte Platzhalter; kommen die
    /// ersten Gruppen, werden sie reihum belegt statt alle auf G1 zu bleiben.
    taktfolge_gruppen: usize,
    /// „Anhören“ läuft: die berechneten Schritte klingen in Schleife, ohne ins
    /// Pattern zu schreiben.
    anhoeren_laeuft: bool,
    /// Stopp-Flag des Anhören-Threads.
    anhoeren_stopp: Option<Arc<AtomicBool>>,
    anhoeren_thread: Option<std::thread::JoinHandle<()>>,
    /// Gerade gespielter Rasterschritt (0…Raster−1) für den Marker im Bild;
    /// Stop setzt 0. Ein Atom statt eines Mutex: die Zeichenfunktion liest je
    /// Frame, der Thread schreibt je Note.
    spiel_position: Arc<AtomicU16>,
    /// Herzschlag der Oberfläche: der Anhören-Thread stoppt, wenn der Laub-Tab
    /// verlassen wird (dann ruft niemand mehr `ui` auf).
    anhoeren_herz: Arc<Mutex<Instant>>,
    /// Zeile unter der Steuerung („ins Raster geschrieben …“).
    status: String,
    /// Zeile der Erkennung („13 Blätter, Median-Abstand 47 px“).
    erkennung: String,
}

impl Default for LaubPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LaubPanel {
    fn drop(&mut self) {
        // Beim Verlassen der App darf der Anhören-Thread nicht weiterklingen.
        self.anhoeren_stoppen(false);
    }
}

impl LaubPanel {
    pub fn new() -> Self {
        Self {
            texture: None,
            textur_offen: None,
            anzeige_masse: (0, 0),
            textur_live: false,
            vorschau_live: true,
            vorschau_nummer: 0,
            vorschau_bildzeit: None,
            standbild_rgba: None,
            bild: None,
            blaetter: Vec::new(),
            abstaende: Vec::new(),
            hintergrund: (128, 128, 128),
            auto_hintergrund: true,
            toleranz: TOLERANZ_STANDARD,
            genre: 0,
            raster: 128,
            relativ: true,
            geschwindigkeit: jamcore::laub::GEHTEMPO_M_S,
            schrittlaenge: jamcore::laub::SCHRITTLANGE_M,
            px_pro_meter: PX_PRO_METER,
            bpm: jamcore::laub::BPM_VORGABE,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            videolauf: None,
            sammeln: None,
            sammeln_letzte_ui: Instant::now(),
            gruppe_groesse: GRUPPEN_VORGABE,
            gruppe_auswahl: 0,
            taktgruppe_len: TAKTGRUPPE_VORGABE,
            taktfolge: Vec::new(),
            taktfolge_gruppen: 0,
            anhoeren_laeuft: false,
            anhoeren_stopp: None,
            anhoeren_thread: None,
            spiel_position: Arc::new(AtomicU16::new(0)),
            anhoeren_herz: Arc::new(Mutex::new(Instant::now())),
            status: "Kamera an + „Foto aufnehmen“, „Bild laden …“ oder „Video laden …“."
                .to_string(),
            erkennung: String::new(),
        }
    }

    /// Das eine Tempo der App (JamApp, jedes Frame) — der absolute Modus
    /// rechnet damit.
    pub fn set_tempo(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(20.0, 300.0);
    }

    /// Ein Bild von der Platte laden (Datei-Browser). Wie im Bild-Tab wird die
    /// lange Kante auf [`BILD_KANTE`] begrenzt und das Bild in den
    /// Kamera-Framepuffer gelegt — so sehen Bild-Tab und Laub-Tab dasselbe
    /// Standbild.
    pub fn laden(&mut self, pfad: &str) {
        self.sammeln_stoppen(false);
        if let Ok(app) = crate::current_app() {
            camera::stop(&app);
            crate::video::stop(&app);
        }
        match image::open(pfad) {
            Ok(bild) => {
                let bild = if bild.width().max(bild.height()) > BILD_KANTE {
                    bild.resize(BILD_KANTE, BILD_KANTE, image::imageops::FilterType::Triangle)
                } else {
                    bild
                };
                let (w, h) = (bild.width() as usize, bild.height() as usize);
                let rgba = bild.to_rgba8().into_raw();
                camera::ruhebild_setzen(w, h, rgba.clone());
                self.bild_uebernehmen(w, h, rgba);
                self.status = format!("Bild geladen: {w}x{h}");
                log::info!("laub: Bild geladen: {pfad} ({w}x{h})");
            }
            Err(e) => {
                self.status = format!("Bild nicht lesbar: {e}");
                log::info!("laub: Bild nicht lesbar: {pfad}: {e}");
            }
        }
    }

    /// Ein Video laden (Datei-Browser): die Dekodierung startet, die Bilder
    /// sammelt [`LaubPanel::video_schritt`] ein. Kehrt sofort zurück — die
    /// Analyse läuft über die nächsten Anzeigeframes.
    pub fn video_laden(&mut self, pfad: &str) {
        self.video_stoppen();
        self.sammeln_stoppen(false);
        if let Ok(app) = crate::current_app() {
            camera::stop(&app);
        }
        let ergebnis = crate::current_app()
            .map_err(|e| e.to_string())
            .and_then(|app| crate::video::start(&app, pfad, VIDEO_FPS));
        match ergebnis {
            Ok(()) => {
                let name = pfad.rsplit('/').next().unwrap_or(pfad);
                self.videolauf = Some(Videolauf {
                    pfad: pfad.to_string(),
                    start: std::time::Instant::now(),
                    letzte_nummer: camera::stand().0,
                    letzte_bildzeit: Instant::now(),
                    letzte_pruefung: None,
                    geprueft: 0,
                    bester: None,
                });
                self.status = format!(
                    "Video läuft: {name} — höchstens alle {VIDEO_ABTAST_MS} ms ein Bild, \
                     bis zu {VIDEO_FRAMES_MAX} insgesamt …"
                );
                log::info!(
                    "laub: Video startet: {pfad} ({VIDEO_FPS} fps, Abtastung alle \
                     {VIDEO_ABTAST_MS} ms, bis zu {VIDEO_FRAMES_MAX} Bilder, Frist {VIDEO_FRIST_S:.0} s)"
                );
            }
            Err(e) => {
                self.status = format!("Video: {e}");
                log::info!("laub: Video nicht startbar: {pfad}: {e}");
            }
        }
    }

    /// Läuft gerade ein Video-Prüfen?
    pub fn video_laeuft(&self) -> bool {
        self.videolauf.is_some()
    }

    /// „Video abbrechen“: laufendes Prüfen verwerfen, Dekoder stoppen.
    pub fn video_abbrechen(&mut self) {
        if self.video_laeuft() {
            self.video_stoppen();
            self.status = "Video abgebrochen".to_string();
            log::info!("laub: Video abgebrochen");
        }
    }

    /// Den Dekoder stoppen und das laufende Prüfen vergessen (ohne Status).
    fn video_stoppen(&mut self) {
        if self.videolauf.take().is_some() {
            if let Ok(app) = crate::current_app() {
                crate::video::stop(&app);
            }
        }
    }

    /// Ein Schritt des Video-Prüfens: **höchstens eines alle
    /// [`VIDEO_ABTAST_MS`] Wanduhr** ansehen, Blätter zählen, das beste
    /// behalten. So verteilen sich die Proben über die ganze Videodauer statt
    /// nur über die ersten zwei Sekunden; nach [`VIDEO_FRAMES_MAX`] Bildern,
    /// [`VIDEO_FRIST_S`] Sekunden oder [`VIDEO_ENDE_OHNE_BILD_S`] ohne neues
    /// Dekodierbild wird abgeschlossen. Die JamApp ruft das je Anzeigeframe
    /// auf — auch außerhalb des Laub-Tabs, damit der Dekoder nicht im
    /// Hintergrund weiterläuft.
    ///
    /// Warum Wanduhr und nicht `stichproben_zeiten` aus dem Kern: die
    /// Java-Brücke (`crate::video`) liefert keine Videodauer, und der Dekoder
    /// läuft in Schleife. Die 400-ms-Drosselung braucht beides nicht und
    /// tastet ein Echtzeit-Video trotzdem gleichmäßig ab (15 Dekodier-fps
    /// genügen für 2,5 Proben je Sekunde).
    pub fn video_schritt(&mut self, ctx: &egui::Context) {
        let Some(mut lauf) = self.videolauf.take() else {
            return;
        };
        let (nummer, _, _) = camera::stand();
        let neues_bild = nummer != lauf.letzte_nummer;
        if neues_bild {
            lauf.letzte_nummer = nummer;
            lauf.letzte_bildzeit = Instant::now();
        }
        let faellig = lauf
            .letzte_pruefung
            .map(|zeit| zeit.elapsed() >= Duration::from_millis(VIDEO_ABTAST_MS))
            .unwrap_or(true);
        if neues_bild && faellig {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                // Auch ein erfolgloses Vorbereiten (kaputte Maße) zählt als
                // Abtastung: sonst würde dasselbe Bild im nächsten Frame
                // erneut verarbeitet.
                lauf.letzte_pruefung = Some(Instant::now());
                if let Some((bw, bh, brgba, blaetter)) = self.videobild_pruefen(w, h, rgba) {
                    lauf.geprueft += 1;
                    let besser = lauf
                        .bester
                        .as_ref()
                        .map(|b| blaetter > b.blaetter)
                        .unwrap_or(true);
                    if besser {
                        lauf.bester = Some(BestesBild {
                            index: lauf.geprueft,
                            w: bw,
                            h: bh,
                            rgba: brgba,
                            blaetter,
                        });
                    }
                    let sekunden = lauf.start.elapsed().as_secs_f32();
                    let bisher = lauf
                        .bester
                        .as_ref()
                        .map(|b| b.blaetter)
                        .unwrap_or(blaetter);
                    self.status = format!(
                        "Bild {} · {:.1} s geprüft · {} Blätter (bestes {})",
                        lauf.geprueft, sekunden, blaetter, bisher
                    );
                }
            }
        }
        let sekunden = lauf.start.elapsed().as_secs_f64();
        let ohne_bild = lauf.letzte_bildzeit.elapsed().as_secs_f64();
        if video_fertig(lauf.geprueft, sekunden, ohne_bild) {
            self.video_beenden(lauf, ctx);
        } else {
            self.videolauf = Some(lauf);
            // Ohne Nutzereingabe zeichnet egui nicht neu — der nächste Schritt
            // braucht aber einen neuen Durchlauf (Abtastung 400 ms).
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    /// Das Video-Prüfen abschließen: Dekoder stoppen und das beste Bild als
    /// Analysebild übernehmen (Marker, Abstände, Rendern wie beim Foto).
    fn video_beenden(&mut self, lauf: Videolauf, ctx: &egui::Context) {
        if let Ok(app) = crate::current_app() {
            crate::video::stop(&app);
        }
        let Some(best) = lauf.bester else {
            let meldung = match crate::current_app() {
                Ok(app) => crate::video::java_meldung(&app),
                Err(_) => String::new(),
            };
            self.status = if meldung.is_empty() {
                "Video ohne Bilder — Format prüfen.".to_string()
            } else {
                format!("Video: {meldung}")
            };
            log::info!("laub: Video ohne Bilder: {} ({meldung})", lauf.pfad);
            ctx.request_repaint();
            return;
        };
        // Wie beim geladenen Bild: das übernommene Bild liegt auch im
        // Kamera-Framepuffer, damit der Bild-Tab dasselbe Standbild sieht.
        camera::ruhebild_setzen(best.w, best.h, best.rgba.clone());
        self.bild_uebernehmen(best.w, best.h, best.rgba);
        let sekunden = lauf.start.elapsed().as_secs_f32();
        self.status = format!(
            "Video geladen: bester Frame {} · {:.1} s geprüft · {} Blätter ({} Bilder)",
            best.index, sekunden, best.blaetter, lauf.geprueft
        );
        log::info!(
            "laub: Video geladen: {} ({}x{}, {} Bilder ueber {:.1} s, bester Frame {}, {} Blaetter)",
            lauf.pfad,
            best.w,
            best.h,
            lauf.geprueft,
            sekunden,
            best.index,
            best.blaetter
        );
        ctx.request_repaint();
    }

    /// Ein Videobild für die Analyse vorbereiten: auf die lange Kante
    /// [`BILD_KANTE`] begrenzen (wie „Bild laden …“), Blätter zählen.
    ///
    /// Hintergrund für das Zählen: im Automatik-Modus die häufigste Farbe des
    /// Bildes selbst — genau die, die [`LaubPanel::auto_hintergrund_waehlen`]
    /// für das gewählte Bild nimmt; sonst die vom Nutzer getippte Farbe.
    fn videobild_pruefen(
        &self,
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    ) -> Option<(usize, usize, Vec<u8>, usize)> {
        if w == 0 || h == 0 || rgba.len() != w * h * 4 {
            return None;
        }
        let bild = verkleinern(image::RgbaImage::from_raw(w as u32, h as u32, rgba)?);
        let (bw, bh) = (bild.width() as usize, bild.height() as usize);
        let rgba = bild.into_raw();
        let rgb = rgb_aus_rgba(&rgba);
        let hintergrund = if self.auto_hintergrund {
            haeufigste_farbe(bw, bh, &rgb).unwrap_or(self.hintergrund)
        } else {
            self.hintergrund
        };
        let n =
            jamcore::laub::blaetter(&rgb, bw as i32, bh as i32, hintergrund, self.toleranz).len();
        Some((bw, bh, rgba, n))
    }

    /// Foto: den letzten Kameraframe als Standbild übernehmen.
    fn foto_aufnehmen(&mut self) {
        match camera::letztes_bild() {
            Some((w, h, rgba)) => {
                self.bild_uebernehmen(w, h, rgba);
                self.status = format!("Foto aufgenommen ({w}x{h})");
                log::info!("laub: Foto aufgenommen ({w}x{h})");
            }
            None => {
                self.status = "Kein Kamerabild — erst „Kamera an“.".to_string();
            }
        }
    }

    /// „Sammeln 10 s“: alle [`SAMMELN_ABSTAND_MS`] ein Kamerabild auswerten
    /// (ohne es anzuzeigen), das mit den meisten Blättern gewinnt. Warum nicht
    /// wie das Foto nur ein Bild: Laub bewegt sich im Wind und die Kamera
    /// leicht — über zehn Sekunden ist mit Sicherheit ein Bild dabei, auf dem
    /// die Blätter frei liegen.
    fn sammeln_starten(&mut self) {
        if self.sammeln.is_some() {
            return;
        }
        // Ein laufendes Video würde denselben Framepuffer füllen: erst
        // stoppen, sonst sammelt die Kamera Videobilder ein.
        self.video_stoppen();
        self.sammeln = Some(Sammellauf {
            start: Instant::now(),
            letzte_nummer: camera::stand().0,
            letzte_probe: None,
            geprueft: 0,
            bester: None,
        });
        self.sammeln_letzte_ui = Instant::now();
        self.status = format!(
            "Sammeln 0/{SAMMELN_PROBEN} … (alle {SAMMELN_ABSTAND_MS} ms ein Bild, \
             {SAMMELN_DAUER_S:.0} s)"
        );
        log::info!(
            "laub: Sammeln startet ({SAMMELN_PROBEN} Bilder ueber {SAMMELN_DAUER_S:.0} s, \
             alle {SAMMELN_ABSTAND_MS} ms)"
        );
    }

    /// Ein Schritt des Kamera-Sammelns. Wird aus [`LaubPanel::ui`] gerufen und
    /// hört deshalb beim Tab-Wechsel auf (siehe `sammeln_letzte_ui`).
    fn sammeln_schritt(&mut self) {
        let Some(mut lauf) = self.sammeln.take() else {
            return;
        };
        let (nummer, _, _) = camera::stand();
        let faellig = lauf
            .letzte_probe
            .map(|zeit| zeit.elapsed() >= Duration::from_millis(SAMMELN_ABSTAND_MS))
            .unwrap_or(true);
        if nummer != lauf.letzte_nummer && faellig {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                lauf.letzte_nummer = nummer;
                lauf.letzte_probe = Some(Instant::now());
                if let Some((bw, bh, brgba, blaetter)) = self.videobild_pruefen(w, h, rgba) {
                    lauf.geprueft += 1;
                    let besser = lauf
                        .bester
                        .as_ref()
                        .map(|b| blaetter > b.blaetter)
                        .unwrap_or(true);
                    if besser {
                        lauf.bester = Some(BestesBild {
                            index: lauf.geprueft,
                            w: bw,
                            h: bh,
                            rgba: brgba,
                            blaetter,
                        });
                    }
                    let bisher = lauf
                        .bester
                        .as_ref()
                        .map(|b| b.blaetter)
                        .unwrap_or(blaetter);
                    self.status = format!(
                        "Sammeln {}/{} … ({} Blätter, bestes {})",
                        lauf.geprueft, SAMMELN_PROBEN, blaetter, bisher
                    );
                }
            }
        }
        let sekunden = lauf.start.elapsed().as_secs_f32();
        // Erst nach vollen zehn Sekunden abschließen (die letzte Probe fällt
        // bei 9,5 s); steht die Kamera, endet das Sammeln nach 11 s mit dem,
        // was da ist, statt ewig zu warten.
        let fertig = (lauf.geprueft >= SAMMELN_PROBEN && sekunden >= SAMMELN_DAUER_S)
            || sekunden >= SAMMELN_DAUER_S + 1.0;
        if fertig {
            self.sammeln_beenden(lauf);
        } else {
            self.sammeln = Some(lauf);
        }
    }

    /// Sammeln abschließen: das beste Bild durch dieselbe Kette wie Foto und
    /// Video schicken (Hintergrund, Blätter, Abstände, Textur).
    fn sammeln_beenden(&mut self, lauf: Sammellauf) {
        let sekunden = lauf.start.elapsed().as_secs_f32();
        match lauf.bester {
            Some(best) => {
                camera::ruhebild_setzen(best.w, best.h, best.rgba.clone());
                self.bild_uebernehmen(best.w, best.h, best.rgba);
                self.status = format!(
                    "Sammeln fertig: bestes von {} Bildern, {} Blätter ({:.1} s)",
                    lauf.geprueft, best.blaetter, sekunden
                );
                log::info!(
                    "laub: Sammeln: {} Bilder ueber {:.1} s, bestes mit {} Blaettern",
                    lauf.geprueft,
                    sekunden,
                    best.blaetter
                );
            }
            None => {
                self.status = "Sammeln: kein Kamerabild — erst „Kamera an“.".to_string();
                log::info!(
                    "laub: Sammeln ohne Bilder ({:.1} s) — Kamera pruefen",
                    sekunden
                );
            }
        }
    }

    /// „Sammeln abbrechen“ bzw. Tab-Wechsel: verwerfen, ohne das Bild zu
    /// übernehmen.
    fn sammeln_stoppen(&mut self, loggen: bool) -> bool {
        let war_an = self.sammeln.take().is_some();
        if war_an {
            self.status = "Sammeln abgebrochen".to_string();
            if loggen {
                log::info!("laub: Sammeln abgebrochen");
            }
        }
        war_an
    }

    /// Ein RGBA-Standbild übernehmen: RGB8 für die Erkennung ablegen, die
    /// Textur beim nächsten `ui` hochladen, Hintergrund (falls automatisch)
    /// und Blätter neu rechnen.
    fn bild_uebernehmen(&mut self, w: usize, h: usize, rgba: Vec<u8>) {
        if w == 0 || h == 0 || rgba.len() != w * h * 4 {
            self.status = "Bild hat unerwartete Maße".to_string();
            return;
        }
        let rgb = rgb_aus_rgba(&rgba);
        self.standbild_rgba = Some((w, h, rgba.clone()));
        self.bild = Some((w, h, rgb));
        self.textur_offen = Some((w, h, rgba));
        // Ein übernommenes Bild ist ein Standbild: Die Live-Vorschau würde es
        // sonst beim nächsten Kameraframe sofort wieder überschreiben.
        self.vorschau_setzen(false);
        if self.auto_hintergrund {
            self.auto_hintergrund_waehlen();
        }
        self.erkennen(true);
    }

    /// Häufigste Farbe des Bildes als Hintergrund (der Weg): ein grobes
    /// Histogramm (jeder 4. Punkt, je Kanal auf 8 Stufen) — der Weg ist auf
    /// einem Weg-Foto die größte Fläche. Ein Tipp ins Bild ersetzt sie.
    fn auto_hintergrund_waehlen(&mut self) {
        let Some((w, h, rgb)) = self.bild.as_ref() else {
            return;
        };
        if let Some(farbe) = haeufigste_farbe(*w, *h, rgb) {
            self.hintergrund = farbe;
            let (r, g, b) = farbe;
            log::info!("laub: Hintergrund automatisch RGB({r}, {g}, {b})");
        }
    }

    /// Blätter im Standbild finden, in Wegrichtung sortieren und die Abstände
    /// (Pixel) zwischen aufeinanderfolgenden Blättern rechnen. `loggen` ist
    /// beim Ziehen der Toleranz aus (sonst eine Log-Zeile je Frame).
    fn erkennen(&mut self, loggen: bool) {
        let Some((w, h, rgb)) = self.bild.as_ref() else {
            self.blaetter.clear();
            self.abstaende.clear();
            self.erkennung = "Noch kein Bild".to_string();
            return;
        };
        let mut bl = jamcore::laub::blaetter(
            rgb,
            *w as i32,
            *h as i32,
            self.hintergrund,
            self.toleranz,
        );
        jamcore::laub::sortiere_wegrichtung(&mut bl);
        self.abstaende = abstaende_px(&bl);
        self.blaetter = bl;
        let (anzeige, log_text) = erkennungstexte(self.blaetter.len(), median(&self.abstaende));
        self.erkennung = anzeige;
        if loggen {
            log::info!("laub: {log_text}");
        }
    }

    /// Farbe im analysierten Bild (RGB) an der Bildposition.
    fn farbe_an(&self, x: usize, y: usize) -> Option<(u8, u8, u8)> {
        let (w, h, rgb) = self.bild.as_ref()?;
        if x >= *w || y >= *h {
            return None;
        }
        let i = (y * *w + x) * 3;
        rgb.get(i..i + 3).map(|p| (p[0], p[1], p[2]))
    }

    /// Der eingestellte Abstandsmodus für den Kern.
    fn modus(&self) -> Abstandsmodus {
        if self.relativ {
            Abstandsmodus::Relativ
        } else {
            Abstandsmodus::Absolut {
                geschwindigkeit_m_s: self.geschwindigkeit,
                schrittlaenge_m: self.schrittlaenge,
            }
        }
    }

    /// Die Abstände in der Einheit des Modus: relativ die Pixel (nur die
    /// Verhältnisse zählen), absolut Meter (Pixel geteilt durch den Maßstab).
    fn eingabe_abstaenden(&self, abstaende: &[f32]) -> Vec<f32> {
        match self.modus() {
            Abstandsmodus::Relativ => abstaende.to_vec(),
            Abstandsmodus::Absolut { .. } => abstaende
                .iter()
                .map(|px| px / self.px_pro_meter.max(1.0))
                .collect(),
        }
    }

    /// Die erkannten Abstände in Gruppen von [`LaubPanel::gruppe_groesse`]
    /// Abständen, samt Startindex in [`LaubPanel::abstaende`]. Zwei
    /// aufeinanderfolgende Abstände gehören zu zwei aufeinanderfolgenden
    /// Blättern — der Startindex ist deshalb zugleich der Index des **ersten
    /// Blattes** der Gruppe in [`LaubPanel::blaetter`]; nur so bleibt die
    /// Blattfarbe je Schritt richtig.
    fn gruppen(&self) -> Vec<(usize, Vec<f32>)> {
        let mut start = 0usize;
        jamcore::laub::gruppiere_abstaende(&self.abstaende, self.gruppe_groesse)
            .into_iter()
            .map(|gruppe| {
                let index = start;
                start += gruppe.len();
                (index, gruppe)
            })
            .collect()
    }

    /// Die Zahl der Schritte je Gruppe (für die Anzeige „G1: 4 Schritte …“) —
    /// dieselbe Kernrechnung, die auch beim Rendern läuft.
    fn gruppen_schritte(&self) -> Vec<usize> {
        let Some(g) = genre(GENRES[self.genre.min(GENRES.len() - 1)]) else {
            return Vec::new();
        };
        let modus = self.modus();
        self.gruppen()
            .iter()
            .map(|(_, abstaende)| {
                let eingabe = self.eingabe_abstaenden(abstaende);
                schritte_aus_abstaenden_bpm(&eingabe, self.raster, modus, &g, self.bpm).len()
            })
            .collect()
    }

    /// Die berechneten Schritte **eines Takts** samt Farbe je Schritt —
    /// dieselbe Kette für „als Rhythmus rendern“, „Anhören“ und die Gruppen,
    /// aber ohne Schreiben ins Pattern. `Err` trägt den Statustext für den
    /// Nutzer. `abstaende` ist ein Ausschnitt aus [`LaubPanel::abstaende`]
    /// (bei einer Gruppe), `blatt_start` der Index des zugehörigen ersten
    /// Blattes.
    fn takt_aus_abstaenden(
        &self,
        abstaende: &[f32],
        blatt_start: usize,
    ) -> Result<Vec<(u32, u8)>, String> {
        let Some(g) = genre(GENRES[self.genre.min(GENRES.len() - 1)]) else {
            return Err("Unbekanntes Genre".to_string());
        };
        if abstaende.is_empty() {
            return Err("Keine Abstände — erst Blätter erkennen.".to_string());
        }
        let modus = self.modus();
        let eingabe = self.eingabe_abstaenden(abstaende);
        let schritte = schritte_aus_abstaenden_bpm(&eingabe, self.raster, modus, &g, self.bpm);
        log::info!(
            "laub: Genre {}, Raster {}, {} -> {} Schritte",
            g.name,
            self.raster,
            if self.relativ { "relativ" } else { "absolut" },
            schritte.len()
        );
        if schritte.is_empty() {
            return Err("Keine Schritte — Abstände, Genre und Raster prüfen.".to_string());
        }
        let blaetter = self.blaetter.get(blatt_start..).unwrap_or(&[]);
        let farben = farben_je_schritt(
            blaetter,
            &eingabe,
            self.raster,
            modus,
            &g,
            self.bpm,
            &schritte,
        );
        Ok(schritte
            .iter()
            .map(|s| {
                (
                    *s,
                    farben
                        .get(s)
                        .copied()
                        .unwrap_or(jamcore::laub::FARBE_ROT),
                )
            })
            .collect())
    }

    /// Der ganze Weg als ein Takt (bisheriges Verhalten).
    fn takt_berechnen(&self) -> Result<Vec<(u32, u8)>, String> {
        self.takt_aus_abstaenden(&self.abstaende, 0)
    }

    /// Eine Gruppe als ein Takt (für Anhören und „Gruppen → Patterns“).
    fn gruppe_berechnen(&self, index: usize) -> Result<Vec<(u32, u8)>, String> {
        let gruppen = self.gruppen();
        let Some((start, abstaende)) = gruppen.get(index) else {
            return Err("Keine Gruppe — erst Blätter erkennen.".to_string());
        };
        self.takt_aus_abstaenden(abstaende, *start)
    }

    /// Die Taktfolge auf die vorhandenen Gruppen klemmen und auf die gewählte
    /// Länge bringen.
    ///
    /// Neue Takte beginnen reihum mit der nächsten Gruppe (`G1, G2, …`), damit
    /// eine frische Taktgruppe nicht viermal denselben Takt spielt; vorhandene
    /// Takte bleiben beim Verlängern stehen. Wird die Gruppenzahl kleiner
    /// (andere Gruppengröße), werden zu große Indizes geklemmt statt verworfen.
    fn taktfolge_anpassen(&mut self) {
        let anzahl = self.gruppen().len();
        self.taktgruppe_len = self.taktgruppe_len.clamp(1, GRUPPEN_MAX);
        self.taktfolge.truncate(self.taktgruppe_len);
        while self.taktfolge.len() < self.taktgruppe_len {
            let i = self.taktfolge.len();
            self.taktfolge.push(if anzahl > 0 { i % anzahl } else { 0 });
        }
        // Vor dem ersten erkannten Bild gibt es keine Gruppen; die Takte sind
        // dann Platzhalter (0). Sobald Gruppen da sind, reihum belegen —
        // sonst bliebe die Folge bei „viermal G1“ stehen.
        if self.taktfolge_gruppen == 0 && anzahl > 0 {
            for (i, index) in self.taktfolge.iter_mut().enumerate() {
                *index = i % anzahl;
            }
        }
        self.taktfolge_gruppen = anzahl;
        for index in &mut self.taktfolge {
            *index = if anzahl > 0 {
                (*index).min(anzahl - 1)
            } else {
                0
            };
        }
    }

    /// Die Gruppenindizes der Taktgruppe (0-basiert, geklemmt). Leer, solange
    /// es keine Gruppen gibt.
    fn taktgruppe_indizes(&self) -> Vec<usize> {
        let anzahl = self.gruppen().len();
        if anzahl == 0 {
            return Vec::new();
        }
        self.taktfolge
            .iter()
            .take(self.taktgruppe_len)
            .map(|index| (*index).min(anzahl - 1))
            .collect()
    }

    /// Die Takte der Taktgruppe in Folge-Reihenfolge (`(Schritt, Farbe)` je
    /// Takt). `T1: G2` heißt: Takt 1 spielt die Abstände von Gruppe 2; Gruppen
    /// dürfen sich wiederholen. Die Abbildung der Indizes macht der Kern
    /// ([`jamcore::laub::taktfolge_gruppen`]), damit Klemmen und Leerfälle dort
    /// getestet sind.
    fn taktgruppe_takte(&self) -> Result<Vec<Vec<(u32, u8)>>, String> {
        let gruppen = self.gruppen();
        if gruppen.is_empty() {
            return Err("Keine Abstände — erst Blätter erkennen.".to_string());
        }
        let indizes = self.taktgruppe_indizes();
        if indizes.is_empty() {
            return Err("Keine Taktgruppe — Länge wählen.".to_string());
        }
        let abstaende: Vec<Vec<f32>> = gruppen.iter().map(|(_, d)| d.clone()).collect();
        let gewaehlt = jamcore::laub::taktfolge_gruppen(&abstaende, &indizes);
        gewaehlt
            .iter()
            .zip(indizes.iter())
            .map(|(abstaende, index)| self.takt_aus_abstaenden(abstaende, gruppen[*index].0))
            .collect()
    }

    /// „Taktgruppe → Patterns“: die Taktfolge hintereinander in das aktuelle
    /// Pattern schreiben (Takt 1 ab Zeile 0, Takt 2 ab Zeile `feinheit`, …).
    /// Passt sie nicht in einen Slot, geht es im nächsten weiter (Ring 8) —
    /// die Aufteilung macht das Rhythmus-Panel
    /// ([`crate::rhythm::RhythmPanel::laub_taktgruppe_setzen`]).
    fn taktgruppe_zu_patterns(&mut self, rhythm: &mut crate::rhythm::RhythmPanel) {
        let takte = match self.taktgruppe_takte() {
            Ok(takte) => takte,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let ziele = rhythm.laub_taktgruppe_setzen(&takte, self.raster);
        let noten: usize = ziele.iter().map(|(_, n)| *n).sum();
        let mut slots: Vec<usize> = Vec::new();
        for (slot, _) in &ziele {
            if !slots.contains(slot) {
                slots.push(*slot);
            }
        }
        let slot_text: Vec<String> = slots.iter().map(|slot| (slot + 1).to_string()).collect();
        self.status = format!(
            "Taktgruppe -> Patterns: {} Takte, {noten} Noten (Slots {})",
            takte.len(),
            slot_text.join(", ")
        );
        log::info!("laub: {}", self.status);
    }

    /// „als Rhythmus rendern“: die Schrittnummern des Kerns ins aktuelle
    /// Rhythmus-Raster schreiben (Farbe je Schritt → Instrument).
    fn rendern(&mut self, rhythm: &mut crate::rhythm::RhythmPanel) {
        let takt = match self.takt_berechnen() {
            Ok(takt) => takt,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let kanaele = kanaele_aus_takt(&takt);
        let noten = rhythm.laub_setzen(&takt, self.raster);
        self.status = format!("ins Raster geschrieben ({noten} Noten auf {kanaele} Kanaelen)");
        log::info!(
            "laub: ins Raster geschrieben ({noten} Noten auf {kanaele} Kanaelen, \
             {} Schritte je Takt)",
            takt.len()
        );
    }

    /// „Gruppen → Patterns“: jede Gruppe wird ein Takt in einem eigenen
    /// Pattern-Slot (Gruppe 1 in den aktuellen, dann der Reihe nach weiter,
    /// nach Slot 8 zurück auf Slot 1). Die Noten verteilen sich wie beim
    /// Rendern auf die Farbkanäle.
    fn gruppen_zu_patterns(&mut self, rhythm: &mut crate::rhythm::RhythmPanel) {
        let gruppen = self.gruppen();
        if gruppen.is_empty() {
            self.status = "Keine Abstände — erst Blätter erkennen.".to_string();
            return;
        }
        let mut takte = Vec::new();
        for index in 0..gruppen.len().min(GRUPPEN_MAX) {
            match self.gruppe_berechnen(index) {
                Ok(takt) => takte.push(takt),
                Err(e) => {
                    self.status = format!("G{}: {e}", index + 1);
                    return;
                }
            }
        }
        let kanaele = takte
            .iter()
            .flat_map(|takt| takt.iter())
            .map(|(_, instrument)| (*instrument).min(63))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let ziele = rhythm.laub_gruppen_setzen(&takte, self.raster);
        let teile: Vec<String> = ziele
            .iter()
            .enumerate()
            .map(|(i, (slot, noten))| format!("G{} {noten} Noten (Slot {})", i + 1, slot + 1))
            .collect();
        self.status = format!(
            "Gruppen -> Patterns: {} — auf {kanaele} Kanaelen",
            teile.join(", ")
        );
        log::info!(
            "laub: Gruppen -> Patterns: {} — auf {kanaele} Kanaelen",
            teile.join(", ")
        );
    }

    /// „Anhören“: dieselbe Kette wie [`LaubPanel::rendern`], aber nur hören —
    /// es wird **nichts** ins Pattern geschrieben. Gespielt wird eine Zeile je
    /// `60/bpm/(feinheit/4)` Sekunden (also im Raster des Rhythmus-Tabs), in
    /// Schleife bis „Stop“, Tab-Wechsel oder Drop.
    fn anhoeren_starten(&mut self, rhythm: &crate::rhythm::RhythmPanel) {
        if self.anhoeren_laeuft {
            return;
        }
        // Immer die gewählte Gruppe hören: bei weniger Abständen als die
        // Gruppengröße ist Gruppe 1 der ganze Weg (wie bisher).
        let gruppe = self.gruppe_auswahl.min(self.gruppen().len().saturating_sub(1));
        let takt = match self.gruppe_berechnen(gruppe) {
            Ok(takt) => takt,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let feinheit = rhythm.feinheit().max(1);
        let bpm = rhythm.bpm().max(20.0);
        let (_, welle, _) = rhythm.klang_zustand();
        // Nur die Noten eines Takts (Startzeile 0, Länge ein Takt): die
        // Schleife wiederholt den Takt, genau wie das Pattern. `anhoer_noten`
        // behält zusätzlich den Rasterschritt — daraus kommt der Marker im
        // Bild, den die Zeilen allein nicht mehr hergeben (mehrere Schritte
        // können auf derselben Zeile liegen).
        let noten = anhoer_noten(&takt, self.raster, feinheit);
        if noten.is_empty() {
            self.status = "Anhören: keine Noten im Takt.".to_string();
            return;
        }
        // Gehört wird zum analysierten Standbild: In der Live-Vorschau wären
        // die markierten Blätter nicht zu sehen, denn sie gehören zum
        // Standbild. Das Standbild wird dafür wieder angezeigt.
        if self.vorschau_live && self.standbild_rgba.is_some() {
            self.vorschau_setzen(false);
            self.textur_offen = self.standbild_rgba.clone();
        }
        let anzahl = takt.len();
        let stopp = Arc::new(AtomicBool::new(false));
        let herz = Arc::clone(&self.anhoeren_herz);
        if let Ok(mut herz) = herz.lock() {
            *herz = Instant::now();
        }
        let stopp_c = Arc::clone(&stopp);
        let position = Arc::clone(&self.spiel_position);
        position.store(0, Ordering::Relaxed);
        let position_c = Arc::clone(&position);
        let ergebnis = std::thread::Builder::new()
            .name("jam-laub-anhoeren".to_string())
            .spawn(move || {
                let schrittzeit = 60.0 / bpm / (feinheit as f32 / 4.0);
                let taktzeit = schrittzeit * feinheit as f32;
                'aussen: loop {
                    let start = Instant::now();
                    for (zeile, farbe, schritt) in &noten {
                        let ziel = *zeile as f32 * schrittzeit;
                        if !warte_bis(&stopp_c, &herz, start, ziel) {
                            break 'aussen;
                        }
                        // Marker im Bild: der Schritt, der jetzt klingt.
                        position_c.store((*schritt).min(u16::MAX as u32) as u16, Ordering::Relaxed);
                        crate::klang::spielen_welle(crate::rhythm::laub_pitch(*farbe), welle);
                    }
                    if !warte_bis(&stopp_c, &herz, start, taktzeit) {
                        break 'aussen;
                    }
                }
            });
        match ergebnis {
            Ok(thread) => {
                self.anhoeren_thread = Some(thread);
                self.anhoeren_stopp = Some(stopp);
                self.anhoeren_laeuft = true;
                self.status = format!(
                    "Anhören G{} ({} Schritte, 1/{feinheit}, {bpm:.0} BPM)",
                    gruppe + 1,
                    anzahl
                );
                log::info!(
                    "laub: Anhören G{} ({anzahl} Schritte, 1/{feinheit}, {bpm:.0} BPM)",
                    gruppe + 1
                );
            }
            Err(e) => self.status = format!("Anhören-Thread: {e}"),
        }
    }

    /// Anhören stoppen (Flag setzen, Thread einsammeln).
    fn anhoeren_stoppen(&mut self, loggen: bool) {
        let war_an = self.anhoeren_laeuft;
        // Marker im Bild auf 0: Ohne Anhören gibt es keinen „gerade
        // gespielten“ Schritt.
        self.spiel_position.store(0, Ordering::Relaxed);
        if let Some(stopp) = self.anhoeren_stopp.take() {
            stopp.store(true, Ordering::Relaxed);
        }
        if let Some(thread) = self.anhoeren_thread.take() {
            let _ = thread.join();
        }
        self.anhoeren_laeuft = false;
        if war_an {
            self.status = "Anhören gestoppt".to_string();
            if loggen {
                log::info!("laub: Anhören gestoppt");
            }
        }
    }

    /// Vom Anhören-Thread selbst beendet (Tab-Wechsel): aufräumen und melden.
    fn anhoeren_aufraeumen(&mut self) {
        if !self.anhoeren_laeuft {
            return;
        }
        if self
            .anhoeren_thread
            .as_ref()
            .map(|thread| thread.is_finished())
            .unwrap_or(false)
        {
            self.anhoeren_stoppen(true);
        }
    }

    /// Die Textur auf ein fertiges RGBA-Bild setzen.
    fn textur_setzen(&mut self, ctx: &egui::Context, w: usize, h: usize, rgba: &[u8]) {
        let bild = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba);
        match self.texture.as_mut() {
            Some(texture) => texture.set(bild, egui::TextureOptions::LINEAR),
            None => {
                self.texture =
                    Some(ctx.load_texture("laubbild", bild, egui::TextureOptions::LINEAR))
            }
        }
        self.anzeige_masse = (w, h);
    }

    /// Vorschau-Modus umschalten: live (neueste Kamerabilder) oder Standbild.
    ///
    /// Beim Zurückschalten auf live wird die Framenummer zurückgesetzt, damit
    /// das aktuelle Kamerabild sofort in die Textur kommt; auf „Standbild“
    /// liegt das eingefrorene Bild schon in [`LaubPanel::standbild_rgba`].
    fn vorschau_setzen(&mut self, live: bool) {
        if self.vorschau_live == live {
            return;
        }
        self.vorschau_live = live;
        self.vorschau_nummer = 0;
        if live {
            log::info!("laub: Vorschau live");
            self.status = "Vorschau live".to_string();
        } else {
            log::info!("laub: Standbild");
            self.status = "Standbild".to_string();
        }
    }

    /// Das analysierte Standbild in die Textur legen (Rückkehr aus der
    /// Vorschau, z. B. wenn die Kamera aus ist).
    fn standbild_zeigen(&mut self, ctx: &egui::Context) {
        self.textur_live = false;
        let Some((w, h, rgba)) = self.standbild_rgba.clone() else {
            return;
        };
        self.textur_setzen(ctx, w, h, &rgba);
    }

    /// Die Live-Vorschau fortschreiben.
    ///
    /// Kopiert wird nur bei **neuer** Framenummer: `camera::letztes_bild`
    /// klont das ganze Bild (bei 640×480 rund 1,2 MB), und die Kamera liefert
    /// ~30 Bilder je Sekunde. Kommt über [`VORSCHAU_STILL_MS`] kein neues Bild
    /// mehr, gilt die Kamera als aus und das Standbild kehrt zurück — so
    /// verhält sich der Tab auch, wenn „Kamera aus“ in einem anderen Tab
    /// gedrückt wurde.
    fn vorschau_aktualisieren(&mut self, ctx: &egui::Context) {
        let (nummer, _, _) = camera::stand();
        let neu = nummer != 0 && nummer != self.vorschau_nummer;
        if neu {
            self.vorschau_bildzeit = Some(Instant::now());
        }
        if !self.vorschau_live {
            return;
        }
        if neu {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                self.textur_setzen(ctx, w, h, &rgba);
                self.textur_live = true;
                self.vorschau_nummer = nummer;
            }
        }
        let still = self
            .vorschau_bildzeit
            .map(|zeit| zeit.elapsed() >= Duration::from_millis(VORSCHAU_STILL_MS))
            .unwrap_or(true);
        if self.textur_live && still {
            // Vorschau bleibt eingeschaltet: startet die Kamera erneut (etwa
            // im Bild-Tab), kommt das nächste Bild von selbst wieder an.
            self.standbild_zeigen(ctx);
        }
        if self.textur_live {
            ctx.request_repaint_after(Duration::from_millis(VORSCHAU_MS));
        }
    }

    /// Der Tab: Kamera-Zeile, Bild mit Markern, Erkennung und Steuerung.
    ///
    /// Das Video-Prüfen ruft die JamApp je Frame ([`LaubPanel::video_schritt`],
    /// auch außerhalb des Tabs) — hier kommt nur der fertige Zustand an.
    pub fn ui(&mut self, ui: &mut egui::Ui, rhythm: &mut crate::rhythm::RhythmPanel) -> Aktion {
        // Herzschlag für den Anhören-Thread: bleibt `ui` aus (Tab-Wechsel),
        // stoppt er sich selbst; ein fertiger Thread wird hier eingesammelt.
        if let Ok(mut herz) = self.anhoeren_herz.lock() {
            *herz = Instant::now();
        }
        self.anhoeren_aufraeumen();
        // Ein laufendes Sammeln überlebt den Tab-Wechsel nicht: `ui` wird nur
        // für den sichtbaren Tab gerufen, eine Lücke von über einer Sekunde
        // heißt also „Tab war weg“ (oder die App pausiert).
        if self.sammeln.is_some()
            && self.sammeln_letzte_ui.elapsed() > Duration::from_secs(1)
        {
            self.sammeln_stoppen(false);
            self.status = "Sammeln abgebrochen (Tab gewechselt)".to_string();
            log::info!("laub: Sammeln abgebrochen (Tab gewechselt)");
        }
        self.sammeln_letzte_ui = Instant::now();
        self.sammeln_schritt();
        if self.sammeln.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if self.anhoeren_laeuft {
            // Ohne Anzeige-Frames käme kein Herzschlag mehr — den Tab also
            // während des Hörens wachhalten.
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let aktion = self.kopfzeile(ui);
        // Erst nach der Kopfzeile hochladen: „Foto aufnehmen“ legt dort ein
        // neues Standbild in `textur_offen`, und die Vorschau darunter darf es
        // nicht mit einem alten Livebild überholen.
        if let Some((w, h, rgba)) = self.textur_offen.take() {
            self.textur_setzen(ui.ctx(), w, h, &rgba);
            self.textur_live = false;
        }
        self.vorschau_aktualisieren(ui.ctx());
        self.bild_zeichnen(ui);
        self.steuerung(ui, rhythm);
        aktion
    }

    /// Kamera und Bildquelle — die Zeile bleibt immer erreichbar (wie im
    /// Bild-Tab).
    fn kopfzeile(&mut self, ui: &mut egui::Ui) -> Aktion {
        let mut aktion = Aktion::Keine;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Kamera an").clicked() {
                let ergebnis = match crate::current_app() {
                    Ok(app) => camera::start(&app),
                    Err(_) => Err("keine Activity".to_string()),
                };
                self.status = match ergebnis {
                    Ok(()) => {
                        self.vorschau_setzen(true);
                        "Kamera startet …".to_string()
                    }
                    Err(e) => format!("Kamera: {e}"),
                };
            }
            if ui.button("Foto aufnehmen").clicked() {
                self.sammeln_stoppen(false);
                self.foto_aufnehmen();
            }
            // „Live“ holt die Vorschau zurück, ohne ein neues Foto zu machen:
            // Das Foto bleibt als Analysebild erhalten, nur die Anzeige folgt
            // wieder der Kamera.
            if ui
                .add_enabled(!self.vorschau_live, egui::Button::new("Live"))
                .clicked()
            {
                self.vorschau_setzen(true);
            }
            if ui
                .button(if self.sammeln.is_some() {
                    "Sammeln abbrechen"
                } else {
                    "Sammeln 10 s"
                })
                .clicked()
            {
                if self.sammeln.is_some() {
                    self.sammeln_stoppen(true);
                } else {
                    self.sammeln_starten();
                }
            }
            if ui.button("Bild laden …").clicked() {
                aktion = Aktion::BrowserBild;
            }
            if ui.button("Video laden …").clicked() {
                aktion = Aktion::BrowserVideo;
            }
            if self.video_laeuft() {
                if ui.button("Video abbrechen").clicked() {
                    self.video_abbrechen();
                }
                ui.spinner();
            }
            let (frames, breite, hoehe) = camera::stand();
            if frames > 0 {
                ui.label(
                    RichText::new(format!("Kamera: {breite}x{hoehe}"))
                        .small()
                        .color(Color32::GRAY),
                );
            }
            ui.label(
                RichText::new(if self.vorschau_live {
                    "Vorschau: live"
                } else {
                    "Vorschau: Standbild"
                })
                .small()
                .color(if self.vorschau_live {
                    Color32::from_rgb(120, 220, 120)
                } else {
                    Color32::GRAY
                }),
            );
        });
        // Zoom: Regler + „ganzes Bild“ (Ziehen im Bild verschiebt).
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Zoom").small().color(Color32::GRAY));
            ui.add(
                egui::Slider::new(&mut self.zoom, 1.0..=8.0)
                    .logarithmic(true)
                    .show_value(false),
            );
            ui.label(
                RichText::new(format!("{:.0} %", self.zoom * 100.0))
                    .small()
                    .color(Color32::GRAY),
            );
            if ui.button("ganzes Bild").clicked() {
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
            }
        });
        aktion
    }

    /// Das Bild mit den Blättern: Kreise in Wegrichtung nummeriert, der Weg als
    /// Linie; ein Tipp ins Bild wählt die Hintergrundfarbe.
    ///
    /// In der Live-Vorschau bleibt die Analyse-Ebene weg: Die Blätter gehören
    /// zum eingefrorenen Bild und lägen über einem anderen Kamerabild falsch.
    fn bild_zeichnen(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = self.texture.clone() else {
            ui.colored_label(
                Color32::GRAY,
                "Noch kein Bild: „Kamera an“ + „Foto aufnehmen“, „Bild laden …“ oder \
                 „Video laden …“.",
            );
            return;
        };
        let (w, h) = self.anzeige_masse;
        if w == 0 || h == 0 {
            return;
        }
        let (w, h) = (w as f32, h as f32);
        let platz = ui.available_width().max(80.0);
        let flaeche = ui.available_height().max(120.0);
        let basis = (platz / w).min(flaeche / h);
        let skala = basis * self.zoom;
        let bild_b = (w * skala).max(1.0);
        let bild_h = (h * skala).max(1.0);
        let sicht = bild_h.min(flaeche);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(platz, sicht), egui::Sense::click_and_drag());
        // Verschiebung begrenzen: das Bild bleibt im Kasten (wie im Bild-Tab).
        let max_x = ((bild_b - rect.width()) / 2.0).max(0.0);
        let max_y = ((bild_h - rect.height()) / 2.0).max(0.0);
        self.pan.x = self.pan.x.clamp(-max_x, max_x);
        self.pan.y = self.pan.y.clamp(-max_y, max_y);
        if response.dragged() {
            self.pan += response.drag_delta();
        }
        let bild_rect = egui::Rect::from_center_size(
            rect.center() + self.pan,
            egui::vec2(bild_b, bild_h),
        );
        // Marker nur über dem Standbild: In der Live-Vorschau zeigen die
        // Blattpositionen des Standbilds auf ein anderes Bild.
        let still = !self.textur_live;
        let spiel = if still {
            self.spielende_blaetter()
        } else {
            None
        };
        let spiel_position = self.spiel_position.load(Ordering::Relaxed) as u32;
        {
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 12, 14));
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), bild_rect, uv, Color32::WHITE);

            if still {
                // Blätter: Kreis (Radius aus der Fläche) mit Nummer in
                // Wegrichtung; die Linie zwischen den Mittelpunkten ist der
                // Weg. Der gerade gehörte Schritt bekommt einen weißen Ring.
                let gelb = Color32::from_rgb(255, 215, 60);
                let punkt = |b: &Blatt| {
                    egui::pos2(
                        bild_rect.min.x + b.x * skala,
                        bild_rect.min.y + b.y * skala,
                    )
                };
                if self.blaetter.len() >= 2 {
                    let punkte: Vec<egui::Pos2> = self.blaetter.iter().map(punkt).collect();
                    painter.add(egui::Shape::line(
                        punkte,
                        Stroke::new(
                            1.5,
                            Color32::from_rgba_unmultiplied(255, 215, 60, 140),
                        ),
                    ));
                }
                for (i, b) in self.blaetter.iter().enumerate() {
                    let p = punkt(b);
                    let r = ((b.flaeche as f32 / std::f32::consts::PI).sqrt() * skala)
                        .clamp(6.0, 40.0);
                    let spielt = spiel
                        .as_ref()
                        .and_then(|(start, positionen)| {
                            i.checked_sub(*start)
                                .and_then(|k| positionen.get(k))
                                .map(|position| *position == spiel_position)
                        })
                        .unwrap_or(false);
                    let (farbe, dicke) = if spielt {
                        (Color32::WHITE, 3.0)
                    } else {
                        (gelb, 2.0)
                    };
                    // Weiß auf hellem Himmel wäre unsichtbar: ein dunkler
                    // Rand macht den Marker überall lesbar.
                    if spielt {
                        painter.circle_stroke(
                            p,
                            r + 1.0,
                            Stroke::new(4.0, Color32::from_black_alpha(150)),
                        );
                    }
                    painter.circle_stroke(p, r, Stroke::new(dicke, farbe));
                    if spielt {
                        painter.text(
                            p + egui::vec2(r + 7.0, -3.0),
                            Align2::LEFT_BOTTOM,
                            format!("{}", i + 1),
                            FontId::proportional(13.0),
                            Color32::from_black_alpha(200),
                        );
                    }
                    painter.text(
                        p + egui::vec2(r + 6.0, -4.0),
                        Align2::LEFT_BOTTOM,
                        format!("{}", i + 1),
                        FontId::proportional(13.0),
                        farbe,
                    );
                }
            }
        }

        // Schrittleiste unter dem Bild: Raster 0…N−1 mit Marker am gerade
        // gespielten Schritt.
        if self.anhoeren_laeuft {
            self.schrittleiste(ui, platz);
        }

        // Tipp ins Bild: die Farbe an der Stelle wird der Hintergrund (wie die
        // Farbwahl im Bild-Tab), danach neu erkennen. Ziehen verschiebt nur
        // (Zoom), deshalb zählt hier der reine Klick. In der Live-Vorschau
        // gäbe es keine Analysefarbe — dort also nicht.
        if still && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    let x = ((pos.x - bild_rect.min.x) / skala).floor();
                    let y = ((pos.y - bild_rect.min.y) / skala).floor();
                    if x >= 0.0 && y >= 0.0 {
                        if let Some(farbe) = self.farbe_an(x as usize, y as usize) {
                            if self.hintergrund != farbe {
                                self.hintergrund = farbe;
                                self.auto_hintergrund = false;
                                log::info!(
                                    "laub: Hintergrund RGB({}, {}, {}) getippt",
                                    farbe.0,
                                    farbe.1,
                                    farbe.2
                                );
                                self.erkennen(true);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Kleine Schrittleiste unter dem Bild: Raster 0…N−1, senkrechter Marker
    /// am gerade gespielten Schritt. Nur während des Anhörens — sonst hätte
    /// sie keine Bedeutung und würde nur Platz kosten.
    fn schrittleiste(&self, ui: &mut egui::Ui, breite: f32) {
        if self.raster == 0 {
            return;
        }
        let (rect, _) = ui.allocate_exact_size(egui::vec2(breite, 18.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect.shrink(1.0), 3.0, Color32::from_rgb(24, 24, 30));
        // Viertelmarken als Orientierung (0, 1/4, …).
        for i in 0..=4 {
            let x = rect.min.x + 2.0 + (rect.width() - 4.0) * i as f32 / 4.0;
            painter.line_segment(
                [
                    egui::pos2(x, rect.max.y - 5.0),
                    egui::pos2(x, rect.max.y - 1.0),
                ],
                Stroke::new(1.0, Color32::from_gray(90)),
            );
        }
        let position = self
            .spiel_position
            .load(Ordering::Relaxed)
            .min(self.raster.saturating_sub(1));
        let x = rect.min.x + 2.0 + (rect.width() - 4.0) * position as f32 / self.raster as f32;
        painter.line_segment(
            [
                egui::pos2(x, rect.min.y + 1.0),
                egui::pos2(x, rect.max.y - 1.0),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );
        painter.text(
            egui::pos2(rect.min.x + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            "0",
            FontId::proportional(9.0),
            Color32::from_gray(120),
        );
        painter.text(
            egui::pos2(rect.max.x - 4.0, rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{}", self.raster - 1),
            FontId::proportional(9.0),
            Color32::from_gray(120),
        );
    }

    /// Die Blätter der gerade gehörten Gruppe samt Rasterposition:
    /// `(erstes Blatt, Positionen)`, wobei `Positionen[k]` zu Blatt
    /// `erstes + k` gehört (eine Gruppe aus n Abständen hat n+1 Blätter —
    /// das erste Blatt vor dem ersten Abstand kommt dazu).
    ///
    /// Warum dieselbe Spiegelrechnung wie beim Färben: Der Kern liefert nur
    /// die Menge der Schritte; welche Blätter auf einem Schritt liegen, folgt
    /// aus ihren Positionen im Takt.
    fn spielende_blaetter(&self) -> Option<(usize, Vec<u32>)> {
        if !self.anhoeren_laeuft {
            return None;
        }
        let gruppen = self.gruppen();
        if gruppen.is_empty() {
            return None;
        }
        let index = self.gruppe_auswahl.min(gruppen.len() - 1);
        let (start, abstaende) = &gruppen[index];
        let g = genre(GENRES[self.genre.min(GENRES.len() - 1)])?;
        let eingabe = self.eingabe_abstaenden(abstaende);
        let positionen = spiegel_positionen(&eingabe, self.raster, self.modus(), &g, self.bpm);
        Some((*start, positionen))
    }

    /// Erkennung (Hintergrund, Toleranz, Abstände), Steuerung (Genre, Raster,
    /// Modus, BPM) und „als Rhythmus rendern“.
    fn steuerung(&mut self, ui: &mut egui::Ui, rhythm: &mut crate::rhythm::RhythmPanel) {
        ui.separator();
        // Die Taktfolge an die aktuelle Gruppenzahl und -länge anpassen: Die
        // Gruppen können sich durch die Gruppengröße ändern, die Folge bleibt
        // dabei gültig (Indizes werden geklemmt, neue Takte aufgefüllt).
        self.taktfolge_anpassen();
        ui.horizontal_wrapped(|ui| {
            ui.label("Hintergrund");
            let (r, g, b) = self.hintergrund;
            let (feld, _) = ui.allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(feld, 2.0, Color32::from_rgb(r, g, b));
            ui.label(
                RichText::new(format!("RGB({r}, {g}, {b}) — ins Bild tippen"))
                    .small()
                    .color(Color32::GRAY),
            );
            ui.label("Toleranz");
            if ui
                .add(egui::Slider::new(&mut self.toleranz, 1..=120))
                .changed()
            {
                self.erkennen(false);
            }
            if ui.button("Neu erkennen").clicked() {
                self.erkennen(true);
            }
            if ui.button("Hintergrund automatisch").clicked() {
                self.auto_hintergrund_waehlen();
                self.erkennen(true);
            }
        });
        if !self.erkennung.is_empty() {
            ui.label(&self.erkennung);
        }
        if !self.abstaende.is_empty() {
            let zahlen: Vec<String> = self.abstaende.iter().map(|d| format!("{d:.0}")).collect();
            ui.label(
                RichText::new(format!("Abstände (px): {}", zahlen.join(", ")))
                    .small()
                    .color(Color32::GRAY),
            );
        }
        // Gruppen: je N Abstände ein Takt. Das teilt einen langen Weg auf
        // mehrere Patterns auf, statt einen immer dichteren Einzeltakt zu
        // bauen.
        ui.horizontal_wrapped(|ui| {
            ui.label("Gruppen");
            for wert in [2usize, 4, 8] {
                if ui
                    .selectable_label(self.gruppe_groesse == wert, wert.to_string())
                    .clicked()
                {
                    self.gruppe_groesse = wert;
                    self.gruppe_auswahl = self
                        .gruppe_auswahl
                        .min(self.gruppen().len().saturating_sub(1));
                    self.taktfolge_anpassen();
                    log::info!("laub: Gruppengroesse {wert} Abstaende");
                }
            }
            ui.label("Abstände");
        });
        if !self.abstaende.is_empty() {
            let schritte = self.gruppen_schritte();
            let text = schritte
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    if i == 0 {
                        format!("G1: {n} Schritte")
                    } else {
                        format!("G{}: {n}", i + 1)
                    }
                })
                .collect::<Vec<_>>()
                .join(" · ");
            ui.label(RichText::new(text).small().color(Color32::GRAY));
        }
        // Taktgruppe: eine Folge von Takten (z. B. G1 G2 G1 G3). Ein Takt ist
        // eine Gruppe aus der Zeile darüber; die Folge ordnet sie den
        // Taktpositionen zu. Die Knöpfe bleiben bedienbar, damit sich die
        // Folge nachträglich umsortieren lässt — ein Tipp schaltet zur
        // nächsten Gruppe weiter (G1→G2→…→Gmax→G1).
        if !self.abstaende.is_empty() {
            let anzahl = self.gruppen().len();
            let schritte = self.gruppen_schritte();
            ui.horizontal_wrapped(|ui| {
                ui.label("Taktgruppe");
                for wert in TAKTGRUPPE_WAHL {
                    if ui
                        .selectable_label(self.taktgruppe_len == wert, wert.to_string())
                        .clicked()
                    {
                        self.taktgruppe_len = wert;
                        self.taktfolge_anpassen();
                        log::info!("laub: Taktgruppe {wert} Takte");
                    }
                }
                ui.label("Takte");
            });
            if anzahl > 0 {
                ui.horizontal_wrapped(|ui| {
                    for i in 0..self.taktfolge.len().min(self.taktgruppe_len) {
                        let index = self.taktfolge[i].min(anzahl - 1);
                        let n = schritte.get(index).copied().unwrap_or(0);
                        if ui
                            .button(format!("T{}: G{} ({n} Schritte)", i + 1, index + 1))
                            .clicked()
                        {
                            let naechste = (index + 1) % anzahl;
                            self.taktfolge[i] = naechste;
                            log::info!("laub: Takt {} -> G{}", i + 1, naechste + 1);
                        }
                    }
                });
                // Die Taktfolge hintereinander in das aktuelle Pattern: Takt 1
                // ab Zeile 0, Takt 2 ab Zeile `feinheit`; was nicht mehr
                // hineinpasst, geht im nächsten Slot weiter (Ring 8).
                if ui
                    .add_enabled(
                        !self.taktfolge.is_empty(),
                        egui::Button::new("Taktgruppe -> Patterns"),
                    )
                    .clicked()
                {
                    self.taktgruppe_zu_patterns(rhythm);
                }
            }
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Genre:");
            for (i, name) in GENRES.iter().enumerate() {
                if ui.selectable_label(self.genre == i, *name).clicked() {
                    self.genre = i;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Raster:");
            for raster in rasterwahl() {
                if ui
                    .selectable_label(self.raster == raster, raster.to_string())
                    .clicked()
                {
                    self.raster = raster;
                }
            }
            ui.label(
                RichText::new("(Schritte je Takt)")
                    .small()
                    .color(Color32::GRAY),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Modus:");
            if ui.selectable_label(self.relativ, "relativ").clicked() {
                self.relativ = true;
            }
            if ui.selectable_label(!self.relativ, "absolut").clicked() {
                self.relativ = false;
            }
            if !self.relativ {
                ui.label("Gehen");
                ui.add(
                    egui::DragValue::new(&mut self.geschwindigkeit)
                        .range(0.2..=3.0)
                        .speed(0.05)
                        .suffix(" m/s"),
                );
                ui.label("Schritt");
                ui.add(
                    egui::DragValue::new(&mut self.schrittlaenge)
                        .range(0.3..=1.5)
                        .speed(0.05)
                        .suffix(" m"),
                );
                ui.label("Maßstab");
                ui.add(
                    egui::DragValue::new(&mut self.px_pro_meter)
                        .range(20.0..=2000.0)
                        .speed(5.0)
                        .suffix(" px/m"),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("BPM {:.0} (App-Tempo)", self.bpm));
            // „2 Patterns“: Laub-Rendern auf zwei Slots verteilen. Der Schalter
            // liegt hier direkt neben dem Rendern, weil er nur diesen Weg
            // betrifft (Euklid/Rampe schreiben weiter in einen Slot).
            let mut zwei = rhythm.zwei_patterns();
            if ui.checkbox(&mut zwei, "2 Patterns").changed() {
                rhythm.set_zwei_patterns(zwei);
                self.status = if zwei {
                    "Laub-Rendern: 2 Patterns (erste Hälfte, Rest)".to_string()
                } else {
                    "Laub-Rendern: 1 Pattern".to_string()
                };
                log::info!("laub: 2 Patterns = {zwei}");
            }
        });
        // Gruppen-Umschalter direkt über den Aktionen: „Anhören“ spielt die
        // gewählte Gruppe, damit man vor dem Schreiben hört, was in den Slot
        // kommt. Höchstens acht — mehr Pattern-Slots gibt es nicht.
        let gruppen_anzahl = self.gruppen().len().min(GRUPPEN_MAX);
        if gruppen_anzahl > 1 {
            self.gruppe_auswahl = self.gruppe_auswahl.min(gruppen_anzahl - 1);
            ui.horizontal_wrapped(|ui| {
                ui.label("Anhören:");
                for i in 0..gruppen_anzahl {
                    if ui
                        .selectable_label(self.gruppe_auswahl == i, format!("G{}", i + 1))
                        .clicked()
                    {
                        self.gruppe_auswahl = i;
                        // Läuft schon eine Vorschau, auf die neue Gruppe
                        // umschalten statt die alte weiterklingen zu lassen.
                        if self.anhoeren_laeuft {
                            self.anhoeren_stoppen(false);
                            self.anhoeren_starten(rhythm);
                        }
                    }
                }
            });
        }
        // Eigene Zeile für die beiden Aktionen: nebeneinander passen sie mit
        // dem BPM-Text und der Checkbox auf schmalen Geräten nicht mehr.
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.abstaende.is_empty(),
                    egui::Button::new("als Rhythmus rendern"),
                )
                .clicked()
            {
                self.rendern(rhythm);
            }
            // Jede Gruppe ein Takt in einem eigenen Pattern-Slot: aus einem
            // langen Weg werden mehrere kurze Patterns statt eines dichten.
            // ASCII-Pfeil, weil die egui-Standardschrift „→“ als leeres
            // Kästchen zeichnet (im Gerätetest gesehen).
            if ui
                .add_enabled(
                    !self.abstaende.is_empty(),
                    egui::Button::new("Gruppen -> Patterns"),
                )
                .clicked()
            {
                self.gruppen_zu_patterns(rhythm);
            }
            // Live hören, ohne ins Pattern zu schreiben — so lässt sich vor
            // dem Rendern entscheiden, ob der Rhythmus passt.
            let anhoeren_text = if self.anhoeren_laeuft {
                "Stop"
            } else {
                "Anhören"
            };
            if ui
                .add_enabled(
                    !self.abstaende.is_empty() || self.anhoeren_laeuft,
                    egui::Button::new(anhoeren_text),
                )
                .clicked()
            {
                if self.anhoeren_laeuft {
                    self.anhoeren_stoppen(true);
                } else {
                    self.anhoeren_starten(rhythm);
                }
            }
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }
}

/// Zahl der verschiedenen Farbkanäle im Takt — die Note trägt ihren Kanal als
/// `instrument.min(63)` (Farbklasse 0…3 → Kanal 1–4 im Tracker). Für Status
/// und Log („auf 3 Kanaelen“).
fn kanaele_aus_takt(takt: &[(u32, u8)]) -> usize {
    let mut kanaele: Vec<u8> = takt.iter().map(|(_, farbe)| (*farbe).min(63)).collect();
    kanaele.sort_unstable();
    kanaele.dedup();
    kanaele.len()
}

/// Noten eines Takts fürs Anhören: `(Pattern-Zeile, Farbe, Rasterschritt)`.
///
/// Dieselbe Abbildung wie `jamcore::laub::raster_auf_zeilen` (erste Note je
/// Zeile gewinnt, nach Zeile sortiert), aber der Rasterschritt bleibt
/// erhalten: Nur so kann die Anzeige die Blätter genau dieses Schritts
/// markieren. Bei feinem Raster fallen mehrere Schritte auf eine Zeile — dann
/// klingt und markiert der erste, wie beim Schreiben ins Pattern.
fn anhoer_noten(takt: &[(u32, u8)], raster: u16, feinheit: u16) -> Vec<(u16, u8, u32)> {
    if raster == 0 || feinheit == 0 {
        return Vec::new();
    }
    let mut zeilen: BTreeMap<u16, (u8, u32)> = BTreeMap::new();
    for (schritt, farbe) in takt {
        let zeile = (*schritt as u64 * feinheit as u64 / raster as u64) as u16;
        if zeile >= feinheit {
            continue;
        }
        zeilen.entry(zeile).or_insert((*farbe, *schritt));
    }
    zeilen
        .into_iter()
        .map(|(zeile, (farbe, schritt))| (zeile, farbe, schritt))
        .collect()
}

/// Wartet bis `ziel_s` Sekunden nach `start`; `false`, wenn gestoppt wurde
/// oder der Laub-Tab verlassen ist (Herzschlag älter als 500 ms).
///
/// In kleinen Schritten, damit „Stop“ und Tab-Wechsel nicht erst am Ende eines
/// Takts wirken.
fn warte_bis(
    stopp: &AtomicBool,
    herz: &Mutex<Instant>,
    start: Instant,
    ziel_s: f32,
) -> bool {
    loop {
        if stopp.load(Ordering::Relaxed) {
            return false;
        }
        if let Ok(herz) = herz.lock() {
            if herz.elapsed().as_millis() > 500 {
                return false;
            }
        }
        let rest = ziel_s - start.elapsed().as_secs_f32();
        if rest <= 0.0 {
            return true;
        }
        std::thread::sleep(Duration::from_secs_f32(rest.min(0.02)));
    }
}

/// Ist das Video-Prüfen fertig? Nach [`VIDEO_FRAMES_MAX`] Bildern; ohne ein
/// einziges Bild nach [`VIDEO_START_FRIST_S`] Sekunden (hängender Dekoder);
/// nach [`VIDEO_ENDE_OHNE_BILD_S`] ohne neues Dekodierbild (Dateiende oder
/// hängender Dekoder); sonst spätestens nach [`VIDEO_FRIST_S`] Sekunden
/// (Notbremse — das beste bisherige Bild wird dann trotzdem übernommen).
fn video_fertig(geprueft: usize, sekunden: f64, ohne_bild_s: f64) -> bool {
    geprueft >= VIDEO_FRAMES_MAX
        || (geprueft == 0 && sekunden >= VIDEO_START_FRIST_S as f64)
        || (geprueft > 0 && ohne_bild_s >= VIDEO_ENDE_OHNE_BILD_S)
        || sekunden >= VIDEO_FRIST_S
}

/// Häufigste Farbe eines RGB8-Bildes: grobes Histogramm (jeder 4. Punkt, je
/// Kanal auf 8 Stufen), Rückgabe ist der Mittelwert der Siegerzelle.
///
/// Auf einem Weg-Foto ist das der Weg — dieselbe Rechnung nutzen die
/// automatische Hintergrundwahl und das Video-Prüfen.
fn haeufigste_farbe(w: usize, h: usize, rgb: &[u8]) -> Option<(u8, u8, u8)> {
    if w == 0 || h == 0 || rgb.len() < w * h * 3 {
        return None;
    }
    let mut zaehler: HashMap<u32, (u64, u64, u64, u64)> = HashMap::new();
    for y in (0..h).step_by(4) {
        for x in (0..w).step_by(4) {
            let i = (y * w + x) * 3;
            let (r, g, b) = (rgb[i], rgb[i + 1], rgb[i + 2]);
            let schluessel = ((r >> 3) as u32) << 10 | ((g >> 3) as u32) << 5 | (b >> 3) as u32;
            let e = zaehler.entry(schluessel).or_insert((0, 0, 0, 0));
            e.0 += 1;
            e.1 += r as u64;
            e.2 += g as u64;
            e.3 += b as u64;
        }
    }
    zaehler
        .into_iter()
        .filter(|(_, e)| e.0 > 0)
        .max_by_key(|(_, e)| e.0)
        .map(|(_, (n, r, g, b))| ((r / n) as u8, (g / n) as u8, (b / n) as u8))
}

/// RGB8 aus RGBA8: Kamera und Videodekoder liefern RGBA, der Kern rechnet RGB.
fn rgb_aus_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for p in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&p[..3]);
    }
    rgb
}

/// Ein RGBA-Bild auf die lange Kante [`BILD_KANTE`] begrenzen (Triangle, wie
/// `image::DynamicImage::resize` in [`LaubPanel::laden`]); kleinere Bilder
/// bleiben unverändert. Videobilder kommen in Dekodiergröße (z. B. 1920×1080)
/// — die Analyse rechnet wie bei Foto und Bild auf höchstens 640 Punkten.
fn verkleinern(bild: image::RgbaImage) -> image::RgbaImage {
    let (w, h) = (bild.width(), bild.height());
    if w.max(h) <= BILD_KANTE {
        return bild;
    }
    let skala = BILD_KANTE as f32 / w.max(h) as f32;
    let nw = ((w as f32 * skala).round() as u32).max(1);
    let nh = ((h as f32 * skala).round() as u32).max(1);
    image::imageops::resize(&bild, nw, nh, image::imageops::FilterType::Triangle)
}

/// Abstände zwischen aufeinanderfolgenden Blättern (euklidisch, Pixel) — die
/// Blätter stehen in Wegrichtung, damit ist es der Weg von Blatt zu Blatt.
fn abstaende_px(blaetter: &[Blatt]) -> Vec<f32> {
    blaetter
        .windows(2)
        .map(|w| {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            (dx * dx + dy * dy).sqrt()
        })
        .collect()
}

/// Median einer Zahlenreihe (bei gerader Anzahl das Mittel der beiden
/// mittleren Werte); ohne Werte `None`.
fn median(werte: &[f32]) -> Option<f32> {
    if werte.is_empty() {
        return None;
    }
    let mut sortiert = werte.to_vec();
    sortiert.sort_by(f32::total_cmp);
    let mitte = sortiert.len() / 2;
    Some(if sortiert.len() % 2 == 0 {
        (sortiert[mitte - 1] + sortiert[mitte]) / 2.0
    } else {
        sortiert[mitte]
    })
}

/// Erkennungs-Text: Anzeige (mit Umlaut) und Log (ASCII, wie die übrigen
/// Log-Zeilen: „laub: 13 Blaetter, Median 47 px“).
fn erkennungstexte(n: usize, med: Option<f32>) -> (String, String) {
    match (n, med) {
        (0, _) => ("0 Blätter".to_string(), "0 Blaetter".to_string()),
        (n, None) => (
            format!("{n} Blätter (nur eines: kein Abstand)"),
            format!("{n} Blaetter (nur eines)"),
        ),
        (n, Some(m)) => (
            format!("{n} Blätter, Median-Abstand {m:.0} px"),
            format!("{n} Blaetter, Median {m:.0} px"),
        ),
    }
}

/// Farbe (Instrument) je Schritt: die erste Blattfarbe auf dem Schritt gewinnt.
///
/// `schritte` ist das Ergebnis des Kerns (Menge, sortiert ohne Doppelte); der
/// Vergleich mit dem Spiegel läuft nur als Warnung — geschrieben wird immer
/// die Menge des Kerns, damit Kern und Raster nicht auseinanderlaufen.
fn farben_je_schritt(
    blaetter: &[Blatt],
    d: &[f32],
    raster: u16,
    modus: Abstandsmodus,
    g: &Genre,
    bpm: f32,
    schritte: &[u32],
) -> HashMap<u32, u8> {
    let mut karte: HashMap<u32, u8> = HashMap::new();
    for (position, blatt) in spiegel_positionen(d, raster, modus, g, bpm)
        .into_iter()
        .zip(blaetter.iter())
    {
        karte.entry(position).or_insert_with(|| farbklasse(blatt));
    }
    let mut menge: Vec<u32> = karte.keys().copied().collect();
    menge.sort_unstable();
    if menge != schritte {
        log::warn!(
            "laub: Spiegelrechnung weicht vom Kern ab ({menge:?} statt {schritte:?}) — \
             Farbe je Schritt kann verschoben sein"
        );
    }
    karte
}

/// Position jedes Blattes im Takt — ein **Spiegel** von
/// `jamcore::laub::schritte_aus_abstaenden_bpm`.
///
/// Nötig, weil der Kern nur die Menge der Schrittnummern zurückgibt (sortiert
/// und ohne Doppelte) — damit ginge die Blattfarbe je Schritt verloren. Die
/// Rechnung ist bewusst Zeile für Zeile der des Kerns nachgebaut (gleiche
/// Filter, gleiche Rundung); [`farben_je_schritt`] vergleicht das Ergebnis
/// zur Sicherheit mit dem Kern und warnt bei Abweichung.
fn spiegel_positionen(
    d: &[f32],
    raster: u16,
    modus: Abstandsmodus,
    g: &Genre,
    bpm: f32,
) -> Vec<u32> {
    if raster == 0 || g.vokabular.is_empty() {
        return Vec::new();
    }
    let abstaende: Vec<f32> = d
        .iter()
        .copied()
        .filter(|wert| wert.is_finite() && *wert >= 0.0)
        .collect();
    if abstaende.is_empty() {
        return Vec::new();
    }
    let vokabeln: Vec<u32> = g
        .vokabular
        .iter()
        .map(|v| vokabel_skaliert(*v, raster))
        .collect();
    let grundschritt = vokabeln[0] as f32;
    let roh: Vec<f32> = match modus {
        Abstandsmodus::Relativ => {
            let kleinster = abstaende
                .iter()
                .copied()
                .filter(|wert| *wert > 0.0)
                .fold(f32::INFINITY, f32::min);
            let bezug = if kleinster.is_finite() { kleinster } else { 1.0 };
            abstaende
                .iter()
                .map(|wert| (wert / bezug) * grundschritt)
                .collect()
        }
        Abstandsmodus::Absolut {
            geschwindigkeit_m_s,
            schrittlaenge_m,
        } => {
            let tempo = if geschwindigkeit_m_s.is_finite() && geschwindigkeit_m_s > 0.0 {
                geschwindigkeit_m_s
            } else {
                jamcore::laub::GEHTEMPO_M_S
            };
            let schritt = if schrittlaenge_m.is_finite() && schrittlaenge_m > 0.0 {
                schrittlaenge_m
            } else {
                jamcore::laub::SCHRITTLANGE_M
            };
            let bpm = if bpm.is_finite() && bpm > 0.0 {
                bpm
            } else {
                jamcore::laub::BPM_VORGABE
            };
            abstaende
                .iter()
                .map(|meter| {
                    let sekunden = meter.max(schritt) / tempo;
                    sekunden * bpm * raster as f32 / 240.0
                })
                .collect()
        }
    };
    let raster = raster as u64;
    let mut raus: Vec<u32> = Vec::with_capacity(roh.len() + 1);
    raus.push(0);
    let mut position: u64 = 0;
    for interval in roh {
        let schritt = runde_auf_vokabular(interval, &vokabeln) as u64;
        position = (position + schritt) % raster;
        raus.push(position as u32);
    }
    raus
}

/// Vokabel in Schritten des gewählten Rasters — Spiegel von
/// `jamcore::laub::vokabel_skaliert` (dort nicht `pub`): `v * raster / 96`,
/// gerundet und mindestens 1.
fn vokabel_skaliert(v: u32, raster_pro_takt: u16) -> u32 {
    let skaliert = (v as u64 * raster_pro_takt as u64 + 48) / 96;
    skaliert.max(1) as u32
}

/// Rohintervall auf das Genre-Vokabular runden — Spiegel von
/// `jamcore::laub::runde_auf_vokabular` (dort nicht `pub`): das nächste
/// Vielfache `k * v` (`k ≥ 1`), bei Gleichstand gewinnt der kürzere Wert und
/// die frühere Vokabel.
fn runde_auf_vokabular(roh: f32, vokabeln: &[u32]) -> u32 {
    let mut best: Option<(f32, u32)> = None;
    for v in vokabeln {
        let v = *v as f32;
        let k = (roh / v - 0.5).ceil().max(1.0);
        let wert = v * k;
        let abstand = (wert - roh).abs();
        if best.map(|(bisher, _)| abstand < bisher).unwrap_or(true) {
            best = Some((abstand, wert.round().max(1.0) as u32));
        }
    }
    best.map(|(_, wert)| wert).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blatt mit Farbe für die Farbtests.
    fn blatt(x: f32, y: f32, r: u8, g: u8, b: u8) -> Blatt {
        Blatt {
            x,
            y,
            flaeche: 9,
            r,
            g,
            b,
        }
    }

    #[test]
    fn abstaende_sind_die_wege_zwischen_den_blaetern() {
        let bl = vec![
            blatt(0.0, 0.0, 0, 0, 0),
            blatt(3.0, 4.0, 0, 0, 0),
            blatt(3.0, 14.0, 0, 0, 0),
        ];
        assert_eq!(abstaende_px(&bl), vec![5.0, 10.0], "3-4-5 und 10");
        assert!(abstaende_px(&[]).is_empty());
        assert!(abstaende_px(&bl[..1]).is_empty(), "ein Blatt: kein Abstand");
    }

    #[test]
    fn median_ist_die_mitte() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[5.0]), Some(5.0));
        assert_eq!(median(&[9.0, 1.0, 5.0]), Some(5.0), "ungerade");
        assert_eq!(median(&[9.0, 1.0, 5.0, 3.0]), Some(4.0), "gerade");
    }

    #[test]
    fn erkennungstexte_nennen_zahl_und_median() {
        let (anzeige, log_text) = erkennungstexte(13, Some(47.4));
        assert_eq!(anzeige, "13 Blätter, Median-Abstand 47 px");
        assert_eq!(log_text, "13 Blaetter, Median 47 px");
        assert_eq!(erkennungstexte(0, None).0, "0 Blätter");
        assert!(erkennungstexte(1, None).0.contains("nur eines"));
    }

    /// Der Spiegel muss dieselbe Menge liefern wie der Kern — sonst wäre die
    /// Farbe je Schritt falsch. Geprüft mit den Kern-Testwerten aus
    /// `core/src/laub.rs` (relativ und absolut).
    #[test]
    fn spiegel_trifft_den_kern() {
        let techno = genre("Techno").expect("Techno");
        let groove = genre("Groove").expect("Groove");
        let breakbeat = genre("Breakbeat").expect("Breakbeat");
        let faelle: Vec<(Vec<f32>, u16, Abstandsmodus, Genre, f32)> = vec![
            (
                vec![2.0, 4.0, 8.0],
                96,
                Abstandsmodus::Relativ,
                techno,
                120.0,
            ),
            (vec![1.0, 2.4, 3.9], 96, Abstandsmodus::Relativ, groove, 120.0),
            (vec![1.0, 1.5], 128, Abstandsmodus::Relativ, breakbeat, 120.0),
            (vec![1.2, 1.8, 1.2, 2.4, 1.2], 96, Abstandsmodus::gehen(), techno, 120.0),
            (vec![1.2], 96, Abstandsmodus::gehen(), techno, 60.0),
            (
                vec![1.0, 1.0, 1.0],
                192,
                Abstandsmodus::Relativ,
                techno,
                128.0,
            ),
        ];
        for (d, raster, modus, g, bpm) in faelle {
            let kern = schritte_aus_abstaenden_bpm(&d, raster, modus, &g, bpm);
            let mut spiegel = spiegel_positionen(&d, raster, modus, &g, bpm);
            spiegel.sort_unstable();
            spiegel.dedup();
            assert_eq!(spiegel, kern, "Spiegel ≠ Kern für {d:?} @ {raster}");
        }
    }

    /// Ein synthetisches RGBA-Bild: grauer Weg, zwei rote Blätter.
    fn video_testbild(w: usize, h: usize) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let blatt = (2..6).contains(&x) && (2..6).contains(&y)
                    || (10..13).contains(&x) && (10..13).contains(&y);
                let p: [u8; 4] = if blatt {
                    [200, 40, 30, 255]
                } else {
                    [128, 128, 128, 255]
                };
                rgba.extend_from_slice(&p);
            }
        }
        rgba
    }

    #[test]
    fn haeufigste_farbe_findet_den_weg() {
        // 16x16: 12 graue Spalten, 4 rote — der Weg ist die größte Fläche.
        let (w, h) = (16usize, 16usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for _y in 0..h {
            for x in 0..w {
                let p: [u8; 3] = if x < 12 {
                    [128, 128, 128]
                } else {
                    [200, 30, 20]
                };
                rgb.extend_from_slice(&p);
            }
        }
        assert_eq!(haeufigste_farbe(w, h, &rgb), Some((128, 128, 128)));
        assert_eq!(haeufigste_farbe(0, 0, &[]), None);
        assert_eq!(haeufigste_farbe(4, 4, &[0, 0, 0]), None, "zu kurz");
    }

    #[test]
    fn rgb_aus_rgba_laesst_alpha_weg() {
        let rgba = vec![1, 2, 3, 255, 4, 5, 6, 0];
        assert_eq!(rgb_aus_rgba(&rgba), vec![1, 2, 3, 4, 5, 6]);
        assert!(rgb_aus_rgba(&[]).is_empty());
    }

    #[test]
    fn verkleinern_begrenzt_die_lange_kante() {
        let gross = image::RgbaImage::from_pixel(1280, 640, image::Rgba([1, 2, 3, 255]));
        let klein = verkleinern(gross);
        assert_eq!((klein.width(), klein.height()), (640, 320), "lange Kante 640");
        let schon_klein = image::RgbaImage::from_pixel(100, 50, image::Rgba([1, 2, 3, 255]));
        let gleich = verkleinern(schon_klein);
        assert_eq!((gleich.width(), gleich.height()), (100, 50), "klein bleibt");
    }

    #[test]
    fn video_fertig_nach_bildern_frist_oder_stillstand() {
        assert!(!video_fertig(0, 0.0, 0.0), "frisch gestartet");
        assert!(!video_fertig(79, 44.0, 0.0), "kurz vor der Obergrenze");
        assert!(video_fertig(80, 6.0, 0.0), "80 Bilder reichen");
        assert!(video_fertig(0, 10.0, 10.0), "kein Bild: hängender Dekoder");
        assert!(!video_fertig(1, 10.0, 0.0), "ein Bild: weiter prüfen");
        assert!(video_fertig(1, 10.0, 1.5), "kein neues Bild mehr");
        assert!(!video_fertig(1, 10.0, 1.4), "noch nicht lange genug still");
        assert!(video_fertig(1, 46.0, 0.0), "Notbremse greift");
    }

    /// Ein Videobild wird wie ein Foto vorbereitet: verkleinert und mit der
    /// Blattzahl des Kerns zurückgegeben.
    #[test]
    fn videobild_pruefen_zaehlt_die_blaetter() {
        let panel = LaubPanel::new();
        let (w, h) = (16usize, 16usize);
        let (bw, bh, _, n) = panel
            .videobild_pruefen(w, h, video_testbild(w, h))
            .expect("Bild");
        assert_eq!((bw, bh), (w, h), "kleines Bild bleibt");
        assert_eq!(n, 2, "zwei Blätter");
        assert!(panel.videobild_pruefen(0, 0, Vec::new()).is_none());
        assert!(
            panel.videobild_pruefen(4, 4, vec![0; 8]).is_none(),
            "falsche Länge"
        );
    }

    #[test]
    fn gruppen_teilen_den_weg_und_kennen_ihr_erstes_blatt() {
        let mut panel = LaubPanel::new();
        panel.abstaende = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        panel.gruppe_groesse = 2;
        let gruppen = panel.gruppen();
        assert_eq!(gruppen.len(), 3, "2 + 2 + 1 Abstände");
        assert_eq!(gruppen[0], (0, vec![1.0, 2.0]));
        assert_eq!(gruppen[1], (2, vec![3.0, 4.0]), "Blattstart = Abstände davor");
        assert_eq!(gruppen[2], (4, vec![5.0]));
        panel.gruppe_groesse = 8;
        assert_eq!(panel.gruppen().len(), 1, "weniger Abstände als die Gruppe");
    }

    #[test]
    fn gruppen_takte_behalten_die_blattfarbe() {
        let mut panel = LaubPanel::new();
        panel.abstaende = vec![1.0];
        panel.gruppe_groesse = 1;
        panel.blaetter = vec![
            blatt(0.0, 0.0, 200, 40, 30),  // rot -> 0
            blatt(1.0, 1.0, 230, 200, 50), // gelb -> 1
        ];
        assert_eq!(panel.gruppe_berechnen(0).unwrap(), vec![(0, 0)], "G1 rot");
        assert_eq!(panel.gruppe_berechnen(1).unwrap(), vec![(0, 1)], "G2 gelb");
    }

    #[test]
    fn kanaele_zaehlen_die_farbklassen() {
        assert_eq!(kanaele_aus_takt(&[(0, 0), (4, 0)]), 1);
        assert_eq!(kanaele_aus_takt(&[(0, 0), (4, 2), (8, 1)]), 3);
        assert_eq!(kanaele_aus_takt(&[]), 0);
    }

    #[test]
    fn farben_nehmen_die_erste_blattfarbe_je_schritt() {
        // Zwei Abstände 1/1 (Techno, Raster 96): Schritte 0, 24, 48.
        let blaetter = vec![
            blatt(0.0, 0.0, 200, 40, 30),  // rot -> 0
            blatt(1.0, 1.0, 230, 200, 50), // gelb -> 1
            blatt(2.0, 2.0, 60, 150, 50),  // grün -> 3
        ];
        let techno = genre("Techno").expect("Techno");
        let d = vec![1.0, 1.0];
        let kern = schritte_aus_abstaenden_bpm(&d, 96, Abstandsmodus::Relativ, &techno, 120.0);
        let farben = farben_je_schritt(
            &blaetter,
            &d,
            96,
            Abstandsmodus::Relativ,
            &techno,
            120.0,
            &kern,
        );
        assert_eq!(farben.get(&0), Some(&0), "rot");
        assert_eq!(farben.get(&24), Some(&1), "gelb");
        assert_eq!(farben.get(&48), Some(&3), "grün");
    }

    #[test]
    fn taktfolge_passt_sich_der_gruppenzahl_an() {
        let mut panel = LaubPanel::new();
        panel.abstaende = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        panel.gruppe_groesse = 2; // drei Gruppen
        panel.taktgruppe_len = 4;
        panel.taktfolge_anpassen();
        assert_eq!(panel.taktfolge, vec![0, 1, 2, 0], "neue Takte reihum");
        panel.taktfolge[1] = 2;
        panel.taktgruppe_len = 2;
        panel.taktfolge_anpassen();
        assert_eq!(panel.taktfolge, vec![0, 2], "beim Kürzen bleibt der Anfang");
        panel.gruppe_groesse = 8; // eine Gruppe
        panel.taktfolge_anpassen();
        assert_eq!(
            panel.taktfolge,
            vec![0, 0],
            "zu große Indizes werden auf die letzte Gruppe geklemmt"
        );
        assert_eq!(panel.taktgruppe_indizes(), vec![0, 0]);
    }

    #[test]
    fn taktfolge_fuellt_nach_dem_ersten_bild_reihum() {
        // Vor dem ersten Bild gibt es keine Gruppen: Die Takte sind
        // Platzhalter. Kommen die Gruppen, darf daraus nicht viermal G1
        // werden — die Folge wird reihum belegt.
        let mut panel = LaubPanel::new();
        panel.taktgruppe_len = 4;
        panel.taktfolge_anpassen();
        assert_eq!(panel.taktfolge, vec![0, 0, 0, 0], "noch kein Bild");
        panel.abstaende = vec![1.0; 8];
        panel.gruppe_groesse = 2; // vier Gruppen
        panel.taktfolge_anpassen();
        assert_eq!(panel.taktfolge, vec![0, 1, 2, 3], "G1 G2 G3 G4");
    }

    #[test]
    fn taktgruppe_wiederholt_gruppen_in_folge() {
        let mut panel = LaubPanel::new();
        panel.abstaende = vec![1.0, 1.0, 1.0, 1.0];
        panel.gruppe_groesse = 2; // zwei Gruppen
        panel.taktgruppe_len = 3;
        panel.taktfolge = vec![1, 0, 1];
        assert_eq!(panel.taktgruppe_indizes(), vec![1, 0, 1], "G2 G1 G2");
        let takte = panel.taktgruppe_takte().expect("drei Takte");
        assert_eq!(takte.len(), 3, "je Takt ein Eintrag");
        assert_eq!(takte[0], takte[2], "dieselbe Gruppe, derselbe Takt");
        // Ohne Abstände gibt es keine Taktgruppe (Meldung statt Panik).
        panel.abstaende.clear();
        assert!(panel.taktgruppe_takte().is_err());
    }

    #[test]
    fn anhoer_noten_behalten_den_rasterschritt() {
        // Raster 128 auf 16 Zeilen: 0…7 liegen auf Zeile 0 (der erste
        // gewinnt), 64 wird Zeile 8, 127 Zeile 15.
        let takt = [(0, 0), (4, 1), (64, 2), (127, 3)];
        assert_eq!(
            anhoer_noten(&takt, 128, 16),
            vec![(0, 0, 0), (8, 2, 64), (15, 3, 127)]
        );
        assert!(anhoer_noten(&takt, 0, 16).is_empty(), "Raster 0");
        assert!(anhoer_noten(&takt, 128, 0).is_empty(), "Feinheit 0");
    }
}
