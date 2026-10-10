//! Bild-Screen: Kamera-Vorschau (S7c) und Farbauswahl per Finger (S7d).
//!
//! Semantik wie im Desktop-rrn5: Linke Maustaste gedrückt heißt "einfrieren
//! und malen". Am Handy ist der gedrückte Finger der Linksklick: Während der
//! Berührung steht das Bild, die Farbe unter dem Finger wird gewählt, und beim
//! Ziehen wandert die Auswahl. Alle Pixel in Toleranzweite der Auswahl werden
//! markiert (Anfang des "Malens"); Zähler und Trefferzahl stehen darunter.
//! Die Anzeige folgt der Kamera, solange keine Auswahl steht; „Auswahl
//! löschen" gibt sie wieder frei.

use eframe::egui::{self, Color32, TextureOptions};

use jamcore::grid::{Note, Pattern};
use jamcore::livescan::{
    build_palette, note_volume, note_volume_farbe, pixel_matches, scale_rgb, scan_column,
    ScanFilter,
};
use jamcore::osc::{OscOut, Renoise};
use jamcore::scanmask::{Aufloesung, ScanMaske};

use crate::camera;
use crate::video;

const TOLERANZ_STANDARD: i32 = 24;
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 4.0;
/// Höhe der Maskenleiste oben im Bild in Punkten.
const LEISTE_H: f32 = 18.0;
/// Der Bildteil nimmt höchstens so viel Höhe, damit die Regler unten sichtbar
/// bleiben (Querformat) — ohne dass man scrollen muss.
const BILD_ANTEIL_H: f32 = 0.55;
/// Im Querformat liegt links das Bild, rechts die Steuerung in zwei Spalten —
/// sonst frisst die GUI die Höhe und das Bild wird winzig. Das Bild bekommt
/// dort gut die Hälfte der Fläche.
const BILD_ANTEIL_H_QUER: f32 = 0.62;

///
/// Gibt es als freie Funktion, weil sie an zwei Stellen gebraucht wird.
fn gesperrte_spalten(maske: &ScanMaske, schritte: usize) -> usize {
    maske
        .abschnitte(schritte)
        .iter()
        .filter(|(_, _, alpha)| *alpha <= 0.0)
        .map(|(von, bis, _)| bis - von)
        .sum()
}

/// Ist die Datei ein Standbild (Endungen wie im Desktop-rrn5,
/// `filter_image_files`: png/jpg/jpeg/bmp).
fn ist_bild(pfad: &str) -> bool {
    std::path::Path::new(pfad)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            ["png", "jpg", "jpeg", "bmp"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        })
        .unwrap_or(false)
}

pub struct ImagePanel {
    texture: Option<egui::TextureHandle>,
    textur_masse: (usize, usize),
    letzte_nummer: u64,
    /// Bild, das während der Berührung steht.
    eingefroren: Option<(usize, usize, Vec<u8>)>,
    /// Auswahlfarbe (Linksklick im Original).
    sel: Option<(u8, u8, u8)>,
    /// Hintergrundfarbe, die der Scan überspringt.
    bg: (u8, u8, u8),
    /// Toleranz je Kanal (wie rrange/grange/brange im Desktop).
    toleranz: (i32, i32, i32),
    treffer: u64,
    status: String,
    /// noteadjust-Modus des Scans: 0 = Farbe (`note_volume_farbe`), 1-8 =
    /// Lage im Bild (Desktop-rrn5, `event_note_volume`).
    note_modus: u8,
    /// Malen (Merken): Farben beim Ziehen in den Merker uebernehmen.
    malen_an: bool,
    /// Scan gegen die gemerkten Farben statt gegen die Einzelauswahl.
    palette_scan: bool,
    /// Merker: gemalte Pixel in Originalfarbe (RGBA).
    mal_puffer: Vec<u8>,
    mal_masse: (usize, usize),
    /// Radieren: Treffer aus dem Merker loeschen statt merken.
    radieren: bool,
    /// Takt-Maske (rrn5 `biialpha`): Spalten an/ab, ohne die Zeilen mit alpha 0.
    maske_an: bool,
    maske: ScanMaske,
    /// Vom Nutzer gewaehlte Schrittzahl (X-Aufloesung = Patternlaenge);
    /// der Zoom skaliert nur die Anzeige.
    schritte_wahl: u16,
    /// Mindestanteil durcher Spalten beim Herabstufen der Maske, in Prozent.
    /// 0 = streng (nichts verschmieren), 100 = eine genuegt.
    maske_anteil: u32,
    /// Takt-Anteil a/b, aus dem die Schrittzahl entsteht (1/1 = 16).
    bruch_a: u16,
    bruch_b: u16,
    /// Taktdauer des Sweeps in Takten (Fader mit /2 x2 /3 x3), f64 wie im Kern.
    pub takte: f64,
    /// Zoom des Bildes als Anteil der verfuegbaren Breite.
    bild_zoom: f32,
    /// Scan-Bereich als Anteile der Bildbreite (rrn5: Alt+D/Alt+C).
    scan_links: f32,
    scan_rechts: f32,
    /// Verschiebung des Bildes im Kasten in Punkten (drei Finger).
    bild_versatz: (f32, f32),
    /// Mehrfinger (zwei Finger Zoom, drei Finger verschieben).
    finger: crate::touch::Mehrfinger,
    /// Harmonics-Filter (Akkord/Notenmengen) wie im Desktop-rrn5.
    harmonie: jamcore::harmonie::Harmonie,
    /// Scan-Treffer ins Rhythmus-Pattern schreiben (Desktop: `L`/bpmo3).
    scan_aufnahme: bool,
    /// Aus der Masken-Folge berechnete Zeitpunkte (fuer Status/Log).
    tipp_zeiten: Vec<f32>,
    /// Klangquelle des Bild-Scans — gespiegelt aus dem Rhythmus-Tab, damit ein
    /// Schalter fuer Bild und Pattern gilt.
    klangquelle: crate::rhythm::Klangquelle,
    welle: jamcore::klang::Welle,
    modul_quelle: Option<crate::player::ModulQuelle>,
    /// x-Spalte des letzten Bildtippens: dort setzt der Harmoniekreis seine
    /// Grenze (Desktop `notessetx[c] = x`).
    tipp_spalte: Option<i32>,
    /// Breite des letzten Kamerabildes in Pixeln.
    bild_w: usize,
    tipp_status: String,
    /// Wie viele Treffer dabei aufgenommen wurden.
    aufgenommen: u64,
    /// Schrittzahl beim letzten Zeichnen (fuer das Nachziehen der Maske).
    letzte_schritte: u16,
    /// Start- und Endpunkt des Pattern-Ausschnitts der X-Auflösung (Schritte).
    abschnitt_von: u16,
    abschnitt_bis: u16,
    /// Wert, den der Finger gerade in die Maske malt (sonst flackert es).
    maske_pinsel: Option<bool>,
    /// Live-Scan (S7e): Sweep ueber die Spalten, Treffer als OSC-Noten.
    scan_an: bool,
    scan_bpm: f32,
    scan_x: i32,
    scan_puffer: Option<(usize, usize, Vec<u8>)>,
    scan_naechste_spalte: std::time::Instant,
    sweep_noten: u64,
    sweep_gezeichnet: u64,
    /// Treffer, die die Bremse (Noten pro Sekunde / Spalte) wegschmeissen hat.
    /// Sichtbar, damit „es klingt nicht" nicht an einer stillen Bremse liegt.
    sweep_doppelt: u64,
    host: String,
    port: String,
    /// Videos aus dem App-Ordner videos/ (S8).
    videos: Vec<String>,
    video_auswahl: Option<String>,
    video_fps: u32,
    video_laeuft: bool,
    video_pruefung: std::time::Instant,
}

impl Default for ImagePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ImagePanel {
    pub fn new() -> Self {
        Self {
            texture: None,
            textur_masse: (0, 0),
            letzte_nummer: 0,
            eingefroren: None,
            sel: None,
            // Gleiche Quelle wie das Pattern (Rhythmus-Tab spiegelt sie
            // herein); ohne sie waere der Bild-Scan stumm.
            klangquelle: crate::rhythm::Klangquelle::Welle,
            welle: jamcore::klang::Welle::Sinus,
            modul_quelle: None,
            tipp_spalte: None,
            bild_w: 1,
            bg: (0, 0, 0),
            toleranz: (TOLERANZ_STANDARD, TOLERANZ_STANDARD, TOLERANZ_STANDARD),
            treffer: 0,
            status: "Kamera starten, dann ins Bild tippen (halten = einfrieren).".to_string(),
            note_modus: 0,
            malen_an: false,
            palette_scan: false,
            mal_puffer: Vec::new(),
            mal_masse: (0, 0),
            radieren: false,
            maske_an: false,
            maske: ScanMaske::neu(16, Aufloesung::Sync),
            schritte_wahl: 16,
            maske_anteil: 50,
            bruch_a: 1,
            bruch_b: 1,
            takte: jamcore::takte::VORGABE,
            bild_zoom: 1.0,
            scan_links: 0.0,
            scan_rechts: 1.0,
            bild_versatz: (0.0, 0.0),
            finger: crate::touch::Mehrfinger::neu(),
            harmonie: jamcore::harmonie::Harmonie::default(),
            scan_aufnahme: false,
            tipp_zeiten: Vec::new(),
            tipp_status: String::new(),
            aufgenommen: 0,
            letzte_schritte: 0,
            abschnitt_von: 0,
            abschnitt_bis: u16::MAX,
            maske_pinsel: None,
            scan_an: false,
            scan_bpm: 120.0,
            scan_x: 0,
            scan_puffer: None,
            scan_naechste_spalte: std::time::Instant::now(),
            sweep_noten: 0,
            sweep_gezeichnet: 0,
            sweep_doppelt: 0,
            host: jamcore::osc::DEFAULT_HOST.to_string(),
            port: jamcore::osc::DEFAULT_BASE_PORT.to_string(),
            videos: Vec::new(),
            video_auswahl: None,
            video_fps: 20,
            video_laeuft: false,
            video_pruefung: std::time::Instant::now(),
        }
    }

    /// Sucht Videodateien im App-Ordner videos/.
    fn videos_scannen(&mut self) {
        self.videos.clear();
        let Ok(app) = crate::current_app() else {
            return;
        };
        let Some(basis) = crate::insets::external_files_dir(&app) else {
            return;
        };
        // Videos aus `videos/`, Standbilder aus `bilder/` — wie am Desktop
        // (dort liest rrn5 ein Verzeichnis mit png/jpg/jpeg/bmp).
        for (ordner_name, endungen) in [
            ("videos", &["mp4", "mkv", "webm", "avi", "mov", "3gp"][..]),
            ("bilder", &["png", "jpg", "jpeg", "bmp"][..]),
        ] {
            let ordner = std::path::PathBuf::from(&basis).join(ordner_name);
            let _ = std::fs::create_dir_all(&ordner);
            let Ok(eintraege) = std::fs::read_dir(&ordner) else {
                continue;
            };
            for eintrag in eintraege.flatten() {
                let pfad = eintrag.path();
                let passt = pfad
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| endungen.iter().any(|x| e.eq_ignore_ascii_case(x)))
                    .unwrap_or(false);
                if passt {
                    self.videos.push(pfad.display().to_string());
                }
            }
        }
        self.videos.sort();
        if self.video_auswahl.is_none() {
            self.video_auswahl = self.videos.first().cloned();
        }
        self.status = format!("{} Quellen gefunden", self.videos.len());
    }

    /// Laedt ein Standbild aus `bilder/` in den Framepuffer. Grosse Bilder
    /// werden auf 640 Punkte lange Kante verkleinert (wie die Kameraauflösung).
    fn bild_an(&mut self, pfad: &str) {
        if let Ok(app) = crate::current_app() {
            camera::stop(&app);
            video::stop(&app);
        }
        self.video_laeuft = false;
        self.eingefroren = None;
        self.sel = None;
        self.scan_puffer = None;
        match image::open(pfad) {
            Ok(bild) => {
                // Seitenverhaeltnis bleibt erhalten; nur die lange Kante wird
                // auf 640 begrenzt (wie die Kameraaufloesung).
                let bild = if bild.width().max(bild.height()) > 640 {
                    bild.resize(640, 640, image::imageops::FilterType::Triangle)
                } else {
                    bild
                };
                let (breite, hoehe) = (bild.width() as usize, bild.height() as usize);
                camera::ruhebild_setzen(breite, hoehe, bild.to_rgba8().into_raw());
                self.status = format!("Bild geladen: {breite}x{hoehe}");
                log::info!("Standbild geladen: {pfad} ({breite}x{hoehe})");
            }
            Err(e) => self.status = format!("Bild nicht lesbar: {e}"),
        }
    }

    fn video_an(&mut self) {
        let Some(pfad) = self.video_auswahl.clone() else {
            self.status = "Keine Quelle ausgewählt (erst suchen).".to_string();
            return;
        };
        // Standbilder aus `bilder/` werden einmal geladen statt abgespielt.
        if ist_bild(&pfad) {
            self.bild_an(&pfad);
            return;
        }
        // Nur eine Quelle gleichzeitig.
        if let Ok(app) = crate::current_app() {
            camera::stop(&app);
        }
        self.eingefroren = None;
        self.sel = None;
        self.scan_puffer = None;
        let ergebnis = crate::current_app()
            .map_err(|e| e.to_string())
            .and_then(|app| video::start(&app, &pfad, self.video_fps));
        match ergebnis {
            Ok(()) => {
                self.video_laeuft = true;
                self.status = format!("Video läuft ({} fps)", self.video_fps);
            }
            Err(e) => self.status = format!("Video: {e}"),
        }
    }

    fn video_aus(&mut self) {
        if let Ok(app) = crate::current_app() {
            video::stop(&app);
        }
        self.video_laeuft = false;
        self.status = "Video gestoppt".to_string();
    }

    pub fn set_target(&mut self, host: &str, port: &str) {
        self.host = host.trim().to_string();
        self.port = port.trim().to_string();
    }

    /// Tempo fuer den Takt-Scan von aussen setzen (Session-Follow oder globale BPM).
    pub fn set_bpm(&mut self, bpm: f32) {
        self.scan_bpm = bpm.clamp(20.0, 300.0);
    }

    fn note_senden(&mut self, note: i32, volume: i32) {
        self.note_lokal(note, volume);
        let (host, port) = (
            self.host.clone(),
            self.port.parse().unwrap_or(jamcore::osc::DEFAULT_BASE_PORT),
        );
        // Wie im Desktop-rrn5: zur Note laeuft der MIDI-Trigger (Port 7002) mit,
        // damit Renoise auch MIDI-Instrumente anspricht.
        match OscOut::new(&host, port).and_then(|out| {
            let renoise = Renoise::new(&out);
            let used = renoise.note_on(7000, note, volume)?;
            if let Err(e) = renoise.trigger_midi(7002, volume) {
                log::warn!("MIDI-Trigger: {e}");
            }
            Ok(used)
        }) {
            // Die Werte, die wirklich rausgehen: /renoise/trigger/note_on mit
            // (track=-1, device=-1, note, volume) auf Port 7000, dazu der
            // MIDI-Trigger auf 7002. Ohne die Lautstaerke war nicht zu sehen,
            // ob der Sweep leise oder voll traf.
            Ok(used) => log::info!(
                "OSC note_on -1 -1 {note} {volume} -> {host}:{used} (MIDI-Trigger :7002)"
            ),
            Err(e) => {
                self.status = format!("OSC-Fehler: {e}");
                log::warn!("OSC note_on -1 -1 {note} {volume} -> {host} FEHLER: {e}");
            }
        }
    }

    /// Die Note auch lokal klingen lassen. Ohne das war der Bild-Scan
    /// stumm, sobald kein Renoise auf dem Laptop laeuft: er ging ausschliesslich
    /// per OSC an `127.0.0.1:7003`. Quelle ist derselbe Schalter wie im
    /// Rhythmus-Tab (Modul-Samples, sonst Tongenerator).
    fn note_lokal(&self, note: i32, volume: i32) {
        if volume <= 0 {
            return;
        }
        if self.klangquelle == crate::rhythm::Klangquelle::Modul {
            if let Some(quelle) = self.modul_quelle.as_ref() {
                if quelle
                    .note_spielen(30, note, volume as f32 / 127.0)
                    .is_some()
                {
                    return;
                }
            }
        }
        crate::klang::spielen_welle(note, self.welle);
    }

    fn kamera_an(&mut self) {
        // Nur eine Quelle gleichzeitig.
        if let Ok(app) = crate::current_app() {
            video::stop(&app);
        }
        self.video_laeuft = false;
        let ergebnis = crate::current_app()
            .map_err(|e| e.to_string())
            .and_then(|app| camera::start(&app));
        self.status = match ergebnis {
            Ok(()) => "Kamera startet …".to_string(),
            Err(e) => format!("Kamera: {e}"),
        };
    }

    fn kamera_aus(&mut self) {
        if let Ok(app) = crate::current_app() {
            camera::stop(&app);
        }
        self.eingefroren = None;
        self.sel = None;
        self.treffer = 0;
        self.status = "Kamera gestoppt".to_string();
    }

    /// Setzt die Textur auf ein fertiges RGBA-Bild.
    fn textur_setzen(&mut self, ctx: &egui::Context, w: usize, h: usize, rgba: &[u8]) {
        let bild = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba);
        match self.texture.as_mut() {
            Some(texture) => texture.set(bild, TextureOptions::LINEAR),
            None => {
                self.texture = Some(ctx.load_texture("kamerabild", bild, TextureOptions::LINEAR))
            }
        }
        self.textur_masse = (w, h);
    }

    /// Farbe an der Bildposition aus dem eingefrorenen Bild.
    fn farbe_an(&self, x: usize, y: usize) -> Option<(u8, u8, u8)> {
        let (w, h, rgba) = self
            .eingefroren
            .as_ref()
            .map(|(w, h, v)| (*w, *h, v.as_slice()))?;
        if x >= w || y >= h {
            return None;
        }
        let idx = (y * w + x) * 4;
        rgba.get(idx..idx + 3).map(|p| (p[0], p[1], p[2]))
    }

    /// Markiert alle Pixel in Toleranzweite der Auswahl und zeigt das Bild.
    fn markieren(&mut self, ctx: &egui::Context) {
        let Some((w, h, bild)) = self.eingefroren.clone().or_else(camera::letztes_bild) else {
            return;
        };
        self.markieren_bild(ctx, w, h, bild);
    }

    /// Markiert ein gegebenes Bild (auch fuer das laufende Livebild).
    fn markieren_bild(&mut self, ctx: &egui::Context, w: usize, h: usize, mut bild: Vec<u8>) {
        let Some(sel) = self.sel else {
            return;
        };
        let filter = ScanFilter {
            bg: self.bg,
            sel,
            range: self.toleranz,
        };
        let mut treffer = 0u64;
        for i in 0..w * h {
            let idx = i * 4;
            if idx + 2 >= bild.len() {
                break;
            }
            let (r, g, b) = (bild[idx], bild[idx + 1], bild[idx + 2]);
            if pixel_matches(r, g, b, &filter) {
                // Malen: die Originalfarbe in den Merker uebernehmen, bevor
                // die Anzeige gruen markiert wird.
                if self.malen_an {
                    if self.mal_masse != (w, h) {
                        self.mal_puffer = vec![0u8; w * h * 4];
                        self.mal_masse = (w, h);
                    }
                    if idx + 3 < self.mal_puffer.len() {
                        if self.radieren {
                            // Radieren: die Stelle im Merker loeschen.
                            self.mal_puffer[idx] = 0;
                            self.mal_puffer[idx + 1] = 0;
                            self.mal_puffer[idx + 2] = 0;
                            self.mal_puffer[idx + 3] = 255;
                        } else {
                            self.mal_puffer[idx] = r;
                            self.mal_puffer[idx + 1] = g;
                            self.mal_puffer[idx + 2] = b;
                            self.mal_puffer[idx + 3] = 255;
                        }
                    }
                }
                bild[idx] = 20;
                bild[idx + 1] = 235;
                bild[idx + 2] = 60;
                treffer += 1;
            }
        }
        self.treffer = treffer;
        self.textur_setzen(ctx, w, h, &bild);
    }

    /// Gewaehlte Schrittzahl: X-Aufloesung = Patternlaenge, mindestens 4.
    pub fn schritte(&self) -> u16 {
        self.schritte_wahl.max(4)
    }

    /// Die Maske aus einer aufgezeichneten Folge bauen.
    /// dem Maske-Tab (Zeitabstaende + Feldwerte als Indexnum); was hier
    /// passiert, ist exakt der Weg aus `pattern_recorder`: erst die normale
    /// Schrittweiten-Rechnung, dann die Timestamps auf die X-Aufloesung
    /// legen (KeyEventRender-Massstab).
    ///
    /// `staerke` waechelt die Modulation der Wischgeschwindigkeit auf die
    /// Krümmung; bei 0.0 bleibt die reine Rechnung uebrig.
    pub fn maske_aus_folge(&mut self, folge: &[(f32, f32, Vec<f32>)], staerke: f32) {
        if folge.is_empty() {
            return;
        }
        let tipps: Vec<jamcore::schrittweite::Tipp> = folge
            .iter()
            .map(|(zeit, indexnum, _)| jamcore::schrittweite::Tipp::new(*zeit, *indexnum))
            .collect();
        let mut zeiten = jamcore::schrittweite::timestamps(&tipps);
        // Die Krümmung kommt aus dem Wischprofil: der Referenzwert ist der
        // Mittelweg der Geste, ein gleichmaessiger Wisch ergibt daher
        // ueberall 1.0 und laesst die Timestamps unveraendert.
        let tempo: Vec<f32> = folge
            .iter()
            .flat_map(|(_, _, t)| t.iter().copied())
            .collect();
        if staerke.abs() > 1e-9 && tempo.len() >= 2 {
            let kappa = jamcore::schrittweite::kruemmung(
                &tempo,
                jamcore::schrittweite::tempo_mittel(&tempo),
            );
            zeiten = jamcore::schrittweite::modulieren(&zeiten, &kappa, staerke);
        }
        let schritte = self.schritte().max(1) as usize;
        let spalten = jamcore::schrittweite::spalten(&zeiten, schritte);
        self.maske.setze_alle(false);
        for spalte in &spalten {
            self.maske.setze(*spalte, true);
        }
        self.maske_an = true;
        self.tipp_zeiten = zeiten.clone();
        self.tipp_status = format!(
            "{} Punkte, {:.1} s",
            zeiten.len(),
            zeiten.last().copied().unwrap_or(0.0)
        );
        log::info!(
            "Maske aus Folge: {} Zeitpunkte -> {} Spalten von {schritte}",
            zeiten.len(),
            spalten.len()
        );
    }

    /// x-Spalte des letzten Bildtippens (Getter fuer den Kreis-Tab).
    /// x-Spalte des letzten Bildtippens (fuer die Harmonie-Grenze).
    pub fn aktuelle_tipp_spalte(&self) -> Option<i32> {
        self.tipp_spalte
    }

    /// Klangquelle, Wellenform und Modul aus dem Rhythmus-Tab uebernehmen, damit
    /// ein Schalter fuer Bild und Pattern gilt.
    pub fn set_klang_zustand(
        &mut self,
        quelle: crate::rhythm::Klangquelle,
        welle: jamcore::klang::Welle,
        modul: Option<crate::player::ModulQuelle>,
    ) {
        self.klangquelle = quelle;
        self.welle = welle;
        self.modul_quelle = modul;
    }

    /// Ein Harmonie-Band aus der Quintenzirkel-Auswahl anhaengen — **an der
    /// x-Stelle, die im Bild angefasst wurde**. Das ist der Ablauf aus dem
    /// Desktop-`quintenzirkel_flow`: dort wird pro Harmonie erst eine
    /// x-Grenze ins Bild geklickt (`notessetx[c] = x`), dann die Noten im
    /// Kreis gewaehlt. Vorher hat die App die Grenze selbst erfunden (halbe
    /// Bildbreite), damit war nur **ein** Band moeglich.
    ///
    /// Ohne angefasste x-Stelle (`x_spalte == None`) greift die alte
    /// Ersatzgrenze, damit der Knopf ohne vorherigen Tipp nicht stumm bleibt.
    pub fn harmonie_band(&mut self, klassen: Vec<i32>, x_spalte: Option<i32>) {
        let breite = self.schritte() as i32;
        let bis_x = match x_spalte {
            Some(x) => x.clamp(1, breite.max(1)),
            None => {
                // Ohne Klick: hinter die letzten Band(s) legen, damit das neue
                // Band nicht mitten im Bild landet.
                let letzte = self
                    .harmonie
                    .baender()
                    .last()
                    .map(|(x, _)| *x)
                    .unwrap_or(breite / 2);
                (letzte + (breite - letzte).max(1)).clamp(1, breite.max(1))
            }
        };
        let letzte_klassen = self.harmonie.sets.last().cloned().unwrap_or_default();
        self.harmonie.set_anhaengen_bei(klassen, bis_x);
        self.harmonie.notenmenge = true;
        self.status = format!(
            "Notenmenge: {} Band bis x={bis_x} (von {breite})",
            self.harmonie.sets.len()
        );
        log::info!("Harmonie-Band bis x={bis_x}: {letzte_klassen:?} (von {breite} Schritten)");
    }

    /// Geprüfter Scan-Bereich (Anteile der Breite) — wie im Kern: beide Grenzen
    /// endlich, geklemmt, rechts > links, sonst die volle Breite.
    fn bereich(&self) -> (f32, f32) {
        if !(self.scan_links.is_finite() && self.scan_rechts.is_finite()) {
            return (0.0, 1.0);
        }
        let links = self.scan_links.clamp(0.0, 1.0);
        let rechts = self.scan_rechts.clamp(0.0, 1.0);
        if rechts > links {
            (links, rechts)
        } else {
            (0.0, 1.0)
        }
    }

    /// Pfad einer Austauschdatei im App-Ordner (KeyEventRender.txt, Snapshot …).
    fn app_datei(&self, name: &str) -> Option<std::path::PathBuf> {
        let app = crate::current_app().ok()?;
        let basis = crate::insets::external_files_dir(&app)?;
        Some(std::path::PathBuf::from(basis).join(name))
    }

    /// Pfad der Austauschdatei mit dem Desktop-Recorder.
    fn keyevent_pfad(&self) -> Option<std::path::PathBuf> {
        self.app_datei("KeyEventRender.txt")
    }

    /// Letztes Kamerabild als BMP sichern (Desktop-rrn5: Taste `l` →
    /// `frame4save.bmp`).
    fn snapshot_sichern(&mut self) {
        let Some((w, h, rgba)) = camera::letztes_bild() else {
            self.status = "Kein Bild für den Snapshot.".to_string();
            return;
        };
        let Some(pfad) = self.app_datei("frame4save.bmp") else {
            self.status = "Kein App-Ordner für den Snapshot.".to_string();
            return;
        };
        let bmp = jamcore::bmp::bmp_aus_rgba(w, h, &rgba);
        match std::fs::write(&pfad, &bmp) {
            Ok(()) => {
                self.status = format!("Snapshot -> {} ({}x{})", pfad.display(), w, h);
                log::info!("Snapshot gesichert: {}", pfad.display());
            }
            Err(e) => self.status = format!("Snapshot-Fehler: {e}"),
        }
    }

    /// Maske als `KeyEventRender.txt` schreiben (Desktop: `s` `0`).
    fn keyevent_sichern(&mut self) {
        let Some(pfad) = self.keyevent_pfad() else {
            self.status = "Kein App-Ordner für die Datei.".to_string();
            return;
        };
        let text = self.maske.als_keyevent(self.schritte().max(1) as usize);
        match std::fs::write(&pfad, &text) {
            Ok(()) => {
                self.status = format!("Maske -> {} ({} Zeilen)", pfad.display(), self.schritte());
                log::info!("KeyEventRender.txt gesichert: {}", pfad.display());
            }
            Err(e) => self.status = format!("Datei-Fehler: {e}"),
        }
    }

    /// Maske aus `KeyEventRender.txt` lesen (Desktop: `space`). Spalten hinter
    /// der Datei sind gesperrt (wie im rrn5), Spalten hinter der aktuellen
    /// X-Auflösung fallen weg.
    fn keyevent_laden(&mut self) {
        let Some(pfad) = self.keyevent_pfad() else {
            self.status = "Kein App-Ordner für die Datei.".to_string();
            return;
        };
        match std::fs::read_to_string(&pfad) {
            Ok(text) => {
                let schritte = self.schritte().max(1) as usize;
                // Die Datei bestimmt die Maske; der Rest bis zur groessten
                // Aufloesung bleibt gesperrt — genau wie im rrn5, wo Spalten
                // hinter der Datei 0.0 liefern. Ohne dieses Auffuellen wuerde
                // spaeteres Wachsen (Zoom, Querformat) die Spalten oeffnen.
                self.maske = ScanMaske::aus_keyevent(&text);
                self.maske.resync(jamcore::grid::MAX_SCHRITTE as usize);
                let gesperrt = gesperrte_spalten(&self.maske, schritte);
                self.status = format!(
                    "Maske aus {} ({} Schritte, {} gesperrt)",
                    pfad.display(),
                    schritte,
                    gesperrt
                );
                log::info!("KeyEventRender.txt geladen: {}", pfad.display());
            }
            Err(e) => self.status = format!("Datei fehlt/kaputt: {e}"),
        }
    }

    /// Pattern aus dem gewählten Abschnitt der X-Auflösung: Start- und Endpunkt
    /// bestimmen, welcher Teil des Bildes ins Pattern kommt (und damit dessen
    /// Länge). Wird im Tagebuch für den Export gebraucht.
    pub fn abschnitt_pattern(&self) -> jamcore::grid::Pattern {
        let schritte = self.schritte();
        let bis = if self.abschnitt_bis == u16::MAX {
            schritte.saturating_sub(1)
        } else {
            self.abschnitt_bis.min(schritte.saturating_sub(1))
        };
        jamcore::grid::Pattern::mit_abschnitt(self.abschnitt_von, bis, schritte)
    }

    /// Schrittzahl ist die vom Nutzer gewaehlte X-Aufloesung; die Anzeige
    /// (Zoom, Breite) aendert sie nicht mehr.
    fn schrittzahl(&self, _platz: f32) -> u16 {
        self.schritte()
    }

    /// Anteil der Höhe, den das Bild im Querformat bekommt. Im Hochformat
    /// reicht [`BILD_ANTEIL_H`], im Querformat ist kaum Höhe da — dort muss das
    /// Bild kleiner sein, damit **alle** Steuerzeilen darunter ohne Scrollen
    /// sichtbar sind.
    fn bild_anteil(platz: f32, flaeche: f32) -> f32 {
        if platz > flaeche {
            BILD_ANTEIL_H_QUER
        } else {
            BILD_ANTEIL_H
        }
    }

    /// Ein Sweep-Schritt: Spalten scannen, Treffer als OSC-Noten senden.
    ///
    /// Wie im Desktop-rrn5: Der Sweep läuft über die X-Auflösung (Schritte), ein
    /// voller Durchlauf dauert so viele Takte, wie der Fader sagt (Vorgabe: ein
    /// Beat). Notenwert aus der Zeile (oben hoch, unten tief). Die Takt-Maske
    /// (rrn5 `biialpha`) sperrt einzelne Spalten; ihr Wert skaliert zusaetzlich
    /// Farbe und Lautstaerke.
    fn scan_schritt(&mut self, ctx: &egui::Context, pattern: &mut Pattern) {
        if !self.scan_an {
            return;
        }
        let Some(sel) = self.sel else {
            return;
        };
        let Some((w, h, live)) = camera::letztes_bild() else {
            return;
        };
        let schritte = self.schritte().max(1) as i32;
        let (wi, hi) = (w as i32, h as i32);
        // Bildbreite merken: der Bildtipp braucht sie, um die x-Spalte des
        // Harmoniebands zu bestimmen.
        self.bild_w = w;
        // Ein voller Durchlauf dauert die eingestellte Taktdauer.
        let schritt_ns = (jamcore::takte::sweep_sekunden(self.takte, self.scan_bpm as f64) * 1.0e9
            / schritte as f64) as i64;
        let schritt_ns = schritt_ns.clamp(200_000, 2_000_000_000) as u64;
        let budget = std::time::Duration::from_millis(10);
        let start = std::time::Instant::now();
        let filter = ScanFilter {
            bg: self.bg,
            sel,
            range: self.toleranz,
        };
        // Palette-Modus: gegen die gemerkten Farben matchen statt gegen die
        // Einzelauswahl (wie im Desktop-rrn5).
        let palette = if self.palette_scan && self.mal_masse == (w, h) {
            let p = build_palette(&self.mal_puffer, w * h, self.bg);
            (!p.is_empty()).then_some(p)
        } else {
            None
        };
        let mut noten: Vec<(i32, i32)> = Vec::new();
        while start.elapsed() < budget {
            let jetzt = std::time::Instant::now();
            if jetzt < self.scan_naechste_spalte {
                // Nicht schlafen: bei grossen Taktzeiten waere das Sekunden im
                // UI-Thread. Stattdessen neu zeichnen lassen, wenn die naechste
                // Spalte dran ist.
                ctx.request_repaint_after(self.scan_naechste_spalte - jetzt);
                break;
            }
            if self.scan_x == 0 {
                // Sweep beginnt: Puffer leeren (nur die Treffer leuchten).
                self.scan_puffer = Some((w, h, vec![0u8; w * h * 4]));
            }
            let schritt = self.scan_x.clamp(0, schritte - 1);
            // Schritt auf die Kameraspalte abbilden — im gewaehlten Bereich
            // (rrn5: Split-Grenzen links/rechts).
            let x = jamcore::livescan::spalte_im_bereich(
                schritt,
                schritte,
                self.scan_links,
                self.scan_rechts,
                wi,
            );
            // Takt-Maske (rrn5 `biialpha`): 0.0 sperrt die Spalte, sonst
            // skaliert der Wert Farbe und Lautstaerke.
            let alpha = if self.maske_an {
                self.maske.alpha(schritt, schritte as usize)
            } else {
                1.0
            };
            let treffer = if alpha > 0.0 {
                scan_column(&live, wi, hi, x, &filter, palette.as_ref())
            } else {
                Vec::new()
            };
            if let Some((_, _, puffer)) = self.scan_puffer.as_mut() {
                for (y, r, g, b) in &treffer {
                    let idx = (*y as usize) * w + x.max(0) as usize;
                    let idx = idx * 4;
                    if idx + 2 < puffer.len() {
                        // "gezeichnet": Treffer in ihrer Farbe in den Puffer,
                        // mit der Masken-Amplitude gedimmt.
                        let (dr, dg, db) = scale_rgb(*r, *g, *b, alpha);
                        puffer[idx] = dr;
                        puffer[idx + 1] = dg;
                        puffer[idx + 2] = db;
                        puffer[idx + 3] = 255;
                        // Note/Lautstaerke nach dem gewaehlten Zuordnungsmodus
                        // (wie noteadjust im Desktop-rrn5). Modus 0 nimmt die
                        // Farbe des Treffers, 1-8 die Lage im Bild.
                        let (note, vol) = if self.note_modus == 0 {
                            note_volume_farbe(*r, *g, *b, 36, 36)
                        } else {
                            note_volume(x, *y, wi, hi, self.note_modus, 36, 36)
                        };
                        // Harmonics-Filter wie im Desktop-Live-Scan: gesperrte
                        // Noten fallen ganz weg (kein Pixel, kein Ton).
                        if !self.harmonie.note_erlaubt(note, x) {
                            continue;
                        }
                        // Fuer den Bild-Follow: den Treffer ins Rhythmus-Pattern
                        // schreiben (Desktop `L`/bpmo3; dort jedes Pixel, hier
                        // eine Note je Schritt).
                        if self.scan_aufnahme {
                            let steps = pattern.steps_per_cycle().max(1) as i64;
                            let step =
                                ((x as i64 * steps) / wi.max(1) as i64).clamp(0, steps - 1) as u16;
                            pattern.insert(Note {
                                step,
                                instrument: 0,
                                pitch: note,
                                glide: false,
                            });
                            self.aufgenommen += 1;
                        }
                        noten.push((note, ((vol as f32 * alpha) as i32).clamp(0, 127)));
                        self.sweep_gezeichnet += 1;
                    }
                }
            }
            // Eine Spalte liefert viele passende Pixel, aber wenige **verschiedene**
            // Noten (gemessen: 182x Note 67, 156x 64, dann ein paar einzelne).
            // Jede Note einmal zu spielen ist der Akkord der Spalte; dieselbe
            // Note 64-mal zu wiederholen hat sie nur zugeworfen — die Warteschlange
            // lief voll und es klang wie ein Dauerton.
            //
            // Also: pro Spalte die Noten entdoppeln, dann alle spielen. Ohne
            // Deckel — die Harmonie-Auswahl ist die gewollte Menge, und nach dem
            // Entfernen der Duplikate ist sie klein genug.
            noten.sort_unstable();
            let alle = noten.len();
            noten.dedup();
            // Die Doppel zaehlen, damit im Log sichtbar bleibt, wie viel von
            // einer Spalte wirklich dieselbe Note war.
            self.sweep_doppelt += (alle - noten.len()) as u64;
            // Pro Spalte: welche Noten mit welcher Lautstaerke. Das ist die
            // Zeile, an der man im Log sieht, ob „y faellt" wirklich faellt.
            if !noten.is_empty() {
                let spalte_text: Vec<String> =
                    noten.iter().map(|(n, v)| format!("{n}/{v}")).collect();
                log::info!(
                    "Spalte {}: {} Noten {}",
                    schritt,
                    noten.len(),
                    spalte_text.join(" ")
                );
            }
            for (note, vol) in noten.drain(..) {
                self.note_senden(note, vol);
                self.sweep_noten += 1;
            }
            let naechster = schritt + 1;
            if naechster >= schritte {
                // Sweep zu Ende: Protokoll, Zaehler zuruecksetzen, und der
                // naechste Durchlauf startet wieder bei Schritt 0.
                log::info!(
                    "Sweep beendet, {} Noten, {} gezeichnet, {} doppelt, {} ({:.2} s)",
                    self.sweep_noten,
                    self.sweep_gezeichnet,
                    self.sweep_doppelt,
                    jamcore::takte::text(self.takte),
                    jamcore::takte::sweep_sekunden(self.takte, self.scan_bpm as f64)
                );
                self.sweep_noten = 0;
                self.sweep_gezeichnet = 0;
                self.sweep_doppelt = 0;
                self.scan_puffer = None;
                self.scan_x = 0;
            } else {
                self.scan_x = naechster;
            }
            self.scan_naechste_spalte += std::time::Duration::from_nanos(schritt_ns);
        }
        if let Some((w, h, puffer)) = self.scan_puffer.clone() {
            self.textur_setzen(ctx, w, h, &puffer);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, pattern: &mut Pattern) {
        // ~20 Bilder je Sekunde reichen fuer die Vorschau und sparen Strom.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        // Alles in eine ScrollArea: im Querformat ist sonst unter dem Bild
        // nichts mehr erreichbar, weil das Bild den ganzen Platz nimmt.
        egui::ScrollArea::vertical()
            .scroll_source(egui::scroll_area::ScrollSource::ALL)
            .id_salt("bild_inhalt")
            .show(ui, |ui| self.inhalt(ui, pattern));
    }

    fn inhalt(&mut self, ui: &mut egui::Ui, pattern: &mut Pattern) {
        ui.horizontal(|ui| {
            if ui.button("Kamera an").clicked() {
                self.kamera_an();
            }
            if ui.button("Kamera aus").clicked() {
                self.kamera_aus();
            }
            ui.label(&self.status);
        });
        ui.horizontal(|ui| {
            if ui
                .button(if self.scan_an { "Scan aus" } else { "Scan an" })
                .clicked()
            {
                self.scan_an = !self.scan_an;
                self.scan_puffer = None;
                self.scan_x = 0;
                // Ohne Ruecksetzen wuerde der erste Schritt sofort „aufholen“.
                self.scan_naechste_spalte = std::time::Instant::now();
                self.status = format!("Scan {}", if self.scan_an { "an" } else { "aus" });
            }
            let namen = [
                "Note: Farbe",
                "Note: Zeile",
                "Note: y steigt, Vol leise",
                "Note: y fällt, Vol laut",
                "Note: y fällt, Vol leise",
                "Note: x steigt, Vol laut",
                "Note: x steigt, Vol leise",
                "Note: x fällt, Vol laut",
                "Note: x fällt, Vol leise",
            ];
            let idx = (self.note_modus as usize).min(namen.len() - 1);
            if ui.button(namen[idx]).clicked() {
                self.note_modus = ((idx + 1) % namen.len()) as u8;
            }
            ui.label(format!(
                "Sweep: {} Noten, {} gezeichnet, {} doppelt",
                self.sweep_noten, self.sweep_gezeichnet, self.sweep_doppelt
            ));
            ui.checkbox(&mut self.scan_aufnahme, "ins Pattern");
            if self.scan_aufnahme {
                ui.label(format!("aufgenommen: {}", self.aufgenommen));
            }
            if ui.button("Snapshot").clicked() {
                self.snapshot_sichern();
            }
        });
        // Dekodierfehler aus Java einmal je Sekunde sichtbar machen.
        if self.video_laeuft && self.video_pruefung.elapsed() >= std::time::Duration::from_secs(1) {
            self.video_pruefung = std::time::Instant::now();
            if let Ok(app) = crate::current_app() {
                let meldung = video::java_meldung(&app);
                if !meldung.is_empty() {
                    self.status = format!("Video: {meldung}");
                }
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Quellen suchen").clicked() {
                self.videos_scannen();
            }
            let mut gewaehlt = self.video_auswahl.clone();
            egui::ComboBox::from_id_salt("videoliste")
                .selected_text(
                    self.video_auswahl
                        .as_deref()
                        .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                        .unwrap_or_else(|| "keine".to_string()),
                )
                .show_ui(ui, |ui| {
                    for pfad in &self.videos {
                        let name = pfad.rsplit('/').next().unwrap_or(pfad).to_string();
                        let ausgewaehlt = self.video_auswahl.as_deref() == Some(pfad.as_str());
                        if ui.selectable_label(ausgewaehlt, name).clicked() {
                            gewaehlt = Some(pfad.clone());
                        }
                    }
                });
            self.video_auswahl = gewaehlt;
            if ui.button("Quelle an").clicked() {
                self.video_an();
            }
            if ui.button("Quelle aus").clicked() {
                self.video_aus();
            }
            ui.label("fps");
            if ui
                .add(egui::Slider::new(&mut self.video_fps, 1..=30))
                .changed()
                && self.video_laeuft
            {
                if let Ok(app) = crate::current_app() {
                    video::fps_setzen(&app, self.video_fps);
                }
            }
        });

        // Live-Textur: laeuft auch mit bestehender Auswahl weiter (dann mit
        // Markierung) — kein Standbild nach dem Farbwaehlen. Nur der Scan
        // uebernimmt die Anzeige.
        let (nummer, _, _) = camera::stand();
        if !self.scan_an && nummer != self.letzte_nummer {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                if self.sel.is_some() {
                    self.markieren_bild(ui.ctx(), w, h, rgba);
                } else {
                    self.textur_setzen(ui.ctx(), w, h, &rgba);
                }
                self.letzte_nummer = nummer;
                self.status = format!("Frame {nummer} ({w}x{h})");
            }
        }

        // X-Aufloesung (Schrittzahl) aus der Anzeigebreite mal Zoom — ist
        // zugleich die Patternlaenge. Die Maske waechst mit. Die Bildbreite
        // steht **vor** dem Scan fest (Querformat: linke Spalte), sonst
        // rechnen Scan und Maskenleiste mit verschiedenen Schrittzahlen.
        let platz = ui.available_width();
        let flaeche = ui.available_height().max(160.0);
        let quer = platz > flaeche;
        let links = if quer {
            (platz * 0.46).max(240.0)
        } else {
            platz
        };
        let schritte = self.schrittzahl(links);
        if schritte != self.letzte_schritte {
            // Schrittzahl geaendert: die Maske wird **auf die neuen Schritte
            // abgebildet** (nicht nur gestreckt) — sonst liegen die alten
            // Spalten links buendeln und das Muster geht verloren.
            let alt_ende = self.letzte_schritte.saturating_sub(1);
            let war_ganz = self.letzte_schritte > 0
                && self.abschnitt_von == 0
                && (self.abschnitt_bis == u16::MAX || self.abschnitt_bis >= alt_ende);
            self.maske
                .auf_breite_mit(schritte as usize, self.maske_anteil as f32 / 100.0);
            self.letzte_schritte = schritte;
            log::info!(
                "Schritte {schritte}: Maske neu abgebildet (Mindestanteil {} %)",
                self.maske_anteil
            );
            if war_ganz {
                self.abschnitt_bis = schritte.saturating_sub(1);
            }
        }

        self.scan_schritt(ui.ctx(), pattern);

        // Im Querformat liegt das Bild links und die Steuerung rechts in zwei
        // Spalten — untereinander wuerde die GUI die Hoehe fressen und das Bild
        // winzig werden.
        if quer {
            let hoehe = flaeche;
            ui.horizontal_top(|ui| {
                ui.allocate_ui(egui::vec2(links, hoehe), |ui| {
                    self.bild_zeichnen(ui, links, hoehe, schritte);
                });
                ui.separator();
                ui.allocate_ui(egui::vec2(platz - links - 8.0, hoehe), |ui| {
                    egui::ScrollArea::vertical()
                        .scroll_source(egui::scroll_area::ScrollSource::ALL)
                        .id_salt("bild_steuerung")
                        .show(ui, |ui| {
                            // `ui.columns` teilt die Breite wirklich auf und beschneidet
                            // jede Spalte auf ihre Haelfte — mit blossen
                            // `allocate_ui`-Kasten ragen die Zeilen sonst
                            // in die Nachbarspalte hinein und ueberlappen.
                            ui.columns(2, |cols| {
                                self.steuerung_teile(&mut cols[0], 0);
                                self.steuerung_teile(&mut cols[1], 1);
                            });
                        });
                });
            });
            return;
        }

        self.bild_zeichnen(ui, platz, flaeche, schritte);
        self.steuerung(ui);
    }

    /// Wie viele Zeilen die Steuerung hat. Jede Zeile ist schmal genug für eine
    /// Spalte; im Querformat wechseln sie zwischen zwei Spalten ab.
    const STEUERUNG_ZEILEN: usize = 11;

    /// Eine Zeile der Steuerung. Jede Zeile ist schmal genug für eine Spalte;
    /// im Querformat wechseln sie sich zwischen zwei Spalten ab.
    fn steuerungs_zeile(&mut self, ui: &mut egui::Ui, zeile: usize) {
        match zeile {
            0 => self.zeile_zoom(ui),
            1 => self.zeile_takte(ui),
            2 => self.zeile_bpm(ui),
            3 => self.zeile_maske(ui),
            4 => self.zeile_schritte(ui),
            5 => self.zeile_toleranz(ui),
            6 => self.zeile_auswahl(ui),
            7 => self.zeile_malen(ui),
            8 => self.zeile_kanal(ui),
            9 => self.zeile_harmonics(ui),
            10 => self.zeile_notenmenge(ui),
            _ => {}
        }
    }

    /// Alle Regler unterhalb des Bildes (Hochformat, eine Spalte).
    fn steuerung(&mut self, ui: &mut egui::Ui) {
        for zeile in 0..Self::STEUERUNG_ZEILEN {
            self.steuerungs_zeile(ui, zeile);
        }
    }

    /// Zeilen 0, 2, 4 … in die linke, 1, 3, 5 … in die rechte Spalte.
    fn steuerung_teile(&mut self, ui: &mut egui::Ui, spalte: usize) {
        let mut zeile = spalte;
        while zeile < Self::STEUERUNG_ZEILEN {
            self.steuerungs_zeile(ui, zeile);
            zeile += 2;
        }
    }

    /// Zoom mit den Bildknöpfen (Breite/Höhe/Mitte).
    fn zeile_zoom(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Zoom");
            let mut prozent = (self.bild_zoom * 100.0).round();
            if ui
                .add(
                    egui::Slider::new(&mut prozent, 20.0..=400.0)
                        .suffix(" %")
                        .logarithmic(true),
                )
                .changed()
            {
                self.bild_zoom = (prozent / 100.0) as f32;
            }
        });
        // Scan-Bereich wie Alt+D/Alt+C im Desktop-rrn5: nur hier laeuft der Sweep.
        // Zwei schmale Zahlenfelder — Slider waeren in der halben Spalte zu breit.
        ui.horizontal(|ui| {
            ui.label("Bereich");
            let mut von = (self.scan_links * 100.0).round();
            let mut bis = (self.scan_rechts * 100.0).round();
            let a = ui.add(
                egui::DragValue::new(&mut von)
                    .range(0.0..=100.0)
                    .suffix(" %"),
            );
            let b = ui.add(
                egui::DragValue::new(&mut bis)
                    .range(0.0..=100.0)
                    .suffix(" %"),
            );
            if a.changed() || b.changed() {
                self.scan_links = (von / 100.0).clamp(0.0, 1.0);
                self.scan_rechts = (bis / 100.0).clamp(0.0, 1.0);
            }
            if ui.button("ganz").clicked() {
                self.scan_links = 0.0;
                self.scan_rechts = 1.0;
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Breite").clicked() {
                self.bild_zoom = 1.0;
                self.bild_versatz = (0.0, 0.0);
            }
            if ui.button("Höhe").clicked() {
                let (w, h) = self.textur_masse;
                let verhaeltnis = if h > 0 && w > 0 {
                    h as f32 / w as f32
                } else {
                    0.75
                };
                let platz = ui.available_width();
                let flaeche = ui.available_height().max(160.0);
                let passend =
                    flaeche * Self::bild_anteil(platz, flaeche) / (platz * verhaeltnis).max(1.0);
                self.bild_zoom = passend.clamp(ZOOM_MIN, ZOOM_MAX);
                self.bild_versatz = (0.0, 0.0);
            }
            if ui.button("Mitte").clicked() {
                self.bild_versatz = (0.0, 0.0);
            }
        });
    }

    /// Taktdauer: ÷2 ×2 ÷3 ×3 und die Dauer in Sekunden.
    fn zeile_takte(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Takte:");
            if ui.button("÷2").clicked() {
                self.takte = jamcore::takte::teile(self.takte, 2.0);
            }
            if ui.button("×2").clicked() {
                self.takte = jamcore::takte::vielfache(self.takte, 2.0);
            }
            if ui.button("÷3").clicked() {
                self.takte = jamcore::takte::teile(self.takte, 3.0);
            }
            if ui.button("×3").clicked() {
                self.takte = jamcore::takte::vielfache(self.takte, 3.0);
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "{} = {:.2} s bei {:.0} BPM",
                jamcore::takte::text(self.takte),
                jamcore::takte::sweep_sekunden(self.takte, self.scan_bpm as f64),
                self.scan_bpm
            ))
            .small(),
        );
    }

    /// BPM des Sweeps.
    fn zeile_bpm(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("BPM");
            ui.add(egui::Slider::new(&mut self.scan_bpm, 30.0..=300.0));
        });
    }

    /// Takt-Maske an/aus (rrn5 `b`).
    fn zeile_maske(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.maske_an, "Maske");
        ui.label(
            egui::RichText::new("grün = durch, rot = gesperrt")
                .small()
                .color(Color32::GRAY),
        );
        // Mindestanteil beim Herabstufen: 0 % = streng (nichts verschmieren,
        // schmale Impulse koennen weg), 100 % = eine durch genuegt. Wirkt bei
        // der naechsten Aenderung der Schrittzahl.
        ui.horizontal(|ui| {
            ui.label("Anteil");
            if ui
                .add(egui::Slider::new(&mut self.maske_anteil, 0..=100).suffix(" %"))
                .changed()
            {
                self.status = format!("Mindestanteil: {} %", self.maske_anteil);
            }
        });
    }

    /// Schrittzahl wählen (Zahlenfeld **und** Takt-Anteil a/b) und die Maske setzen.
    fn zeile_schritte(&mut self, ui: &mut egui::Ui) {
        // Zahlenfeld: direkte Schrittzahl (X-Auflösung = Patternlänge).
        ui.horizontal_wrapped(|ui| {
            ui.label("Schritte");
            // Voreinstellungen als Knöpfe: zwei Reihen, damit sie auch im Querformat in
            // die Spalten passen. Beim Wechsel wird die Maske auf die neuen
            // Schritte abgebildet (siehe `auf_breite_mit`).
            const VOREIN: [[u16; 6]; 2] = [[16, 32, 64, 128, 256, 512], [12, 24, 48, 96, 192, 384]];
            for reihe in VOREIN {
                ui.horizontal(|ui| {
                    for wert in reihe {
                        if ui
                            .selectable_label(self.schritte() == wert, wert.to_string())
                            .clicked()
                        {
                            self.schritte_wahl = wert;
                            log::info!("Schritte gewaehlt: {wert}");
                        }
                    }
                });
            }
            // Takt-Anteil a/b statt Kopfrechnen: 1/1 = ein Takt (16 Schritte).
            ui.label("Takt");
            let a = ui.add(egui::DragValue::new(&mut self.bruch_a).range(1..=64));
            ui.label("/");
            let b = ui.add(egui::DragValue::new(&mut self.bruch_b).range(1..=64));
            if a.changed() || b.changed() {
                let nenner = self.bruch_b.max(1) as f32;
                let anteil = self.bruch_a.max(1) as f32 / nenner;
                self.schritte_wahl = ((16.0 * anteil).round() as i64).clamp(4, 4096) as u16;
                log::info!(
                    "Schritte aus {}/{} Takt: {}",
                    self.bruch_a,
                    self.bruch_b,
                    self.schritte()
                );
            }
            ui.label(
                egui::RichText::new(format!("{} Schritte", self.schritte()))
                    .small()
                    .color(Color32::GRAY),
            );
        });
        // Start- und Endpunkt des Pattern-Ausschnitts: welcher Teil des Bildes
        // ins Pattern kommt — und damit dessen Länge (wie „Width?" und „Start?"
        // im Desktop-Recorder).
        let ganz = self.schritte().saturating_sub(1);
        if self.abschnitt_bis == u16::MAX {
            self.abschnitt_bis = ganz;
        }
        let mut von = self.abschnitt_von.min(ganz);
        let mut ende = self.abschnitt_bis.min(ganz).max(von);
        if ui
            .horizontal(|ui| {
                ui.label("Pattern");
                let a = ui.add(egui::DragValue::new(&mut von).range(0..=ganz));
                let b = ui.add(egui::DragValue::new(&mut ende).range(von..=ganz));
                a.changed() || b.changed()
            })
            .inner
        {
            self.abschnitt_von = von.min(ende);
            self.abschnitt_bis = ende;
            log::info!(
                "Pattern-Abschnitt {}..{}",
                self.abschnitt_von,
                self.abschnitt_bis
            );
        }
        ui.horizontal(|ui| {
            if ui.button("ganzes Bild").clicked() {
                self.abschnitt_von = 0;
                self.abschnitt_bis = ganz;
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} Schritte",
                    self.abschnitt_bis.saturating_sub(self.abschnitt_von) + 1
                ))
                .small()
                .color(Color32::GRAY),
            );
            if ui.button("alle an").clicked() {
                self.maske.setze_alle(true);
            }
            if ui.button("alle aus").clicked() {
                self.maske.setze_alle(false);
            }
        });
        // Austausch mit dem Desktop-Recorder: dieselbe Datei, die rrn5 mit
        // `space` liest und der Recorder mit `s` `0` schreibt.
        ui.horizontal(|ui| {
            if ui.button("Datei sichern").clicked() {
                self.keyevent_sichern();
            }
            if ui.button("Datei laden").clicked() {
                self.keyevent_laden();
            }
            ui.label(
                egui::RichText::new("KeyEventRender.txt")
                    .small()
                    .color(Color32::GRAY),
            );
        });
        let gesperrt = gesperrte_spalten(&self.maske, self.schritte().max(1) as usize);
        ui.label(
            egui::RichText::new(format!("{} von {} gesperrt", gesperrt, self.schritte()))
                .small()
                .color(Color32::GRAY),
        );
    }

    /// Toleranz und Hintergrundfarbe.
    fn zeile_toleranz(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut tol = self.toleranz.0.max(self.toleranz.1).max(self.toleranz.2);
            if ui
                .add(egui::Slider::new(&mut tol, 0..=80).text("Toleranz"))
                .changed()
            {
                self.toleranz = (tol, tol, tol);
                if self.sel.is_some() {
                    let ctx = ui.ctx().clone();
                    self.markieren(&ctx);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Hintergrund:");
            let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
            let (r, g, b) = self.bg;
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
        });
    }

    /// Farbauswahl aus dem Bild.
    fn zeile_auswahl(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Auswahl:");
            match self.sel {
                Some((r, g, b)) => {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
                    ui.label(egui::RichText::new(format!("{r},{g},{b}")).small());
                }
                None => {
                    ui.label(egui::RichText::new("keine").small().color(Color32::GRAY));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{} Pixel", self.treffer)).small());
            if ui.button("löschen").clicked() {
                self.sel = None;
                self.treffer = 0;
            }
        });
    }

    /// Malen, Radieren, Palette-Scan und der Merker.
    fn zeile_malen(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.malen_an, "Malen");
            ui.checkbox(&mut self.radieren, "Radieren");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.palette_scan, "Palette");
            if ui.button("Merker weg").clicked() {
                self.mal_puffer.clear();
                self.mal_masse = (0, 0);
            }
        });
        if self.mal_masse != (0, 0) {
            let farben = build_palette(
                &self.mal_puffer,
                self.mal_masse.0 * self.mal_masse.1,
                self.bg,
            );
            ui.label(
                egui::RichText::new(format!("{} Farben gemerkt", farben.len()))
                    .small()
                    .color(Color32::GRAY),
            );
        }
    }

    /// R/G/B fein (wie rrange/grange/brange im Desktop-rrn5).
    fn zeile_kanal(&mut self, ui: &mut egui::Ui) {
        for index in 0..3 {
            let (name, wert) = match index {
                0 => ("R", &mut self.toleranz.0),
                1 => ("G", &mut self.toleranz.1),
                _ => ("B", &mut self.toleranz.2),
            };
            let mut geaendert = false;
            ui.horizontal(|ui| {
                ui.label(name);
                geaendert = ui.add(egui::Slider::new(wert, 0..=80)).changed();
            });
            if geaendert && self.sel.is_some() {
                let ctx = ui.ctx().clone();
                self.markieren(&ctx);
            }
        }
    }

    /// Harmonics: Filter an/aus, Akkordart, Quinte, Transposition.
    fn zeile_harmonics(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.harmonie.an, "Harmonics");
            // Wie im Desktop: Dur gewinnt, sonst Plagal, sonst Moll.
            let moll = !self.harmonie.dur && !self.harmonie.plagal;
            if ui.selectable_label(self.harmonie.dur, "Dur").clicked() {
                self.harmonie.dur = true;
                self.harmonie.plagal = false;
            }
            if ui
                .selectable_label(!self.harmonie.dur && self.harmonie.plagal, "Plagal")
                .clicked()
            {
                self.harmonie.dur = false;
                self.harmonie.plagal = true;
            }
            if ui.selectable_label(moll, "Moll").clicked() {
                self.harmonie.dur = false;
                self.harmonie.plagal = false;
            }
            ui.checkbox(&mut self.harmonie.quinte, "Quinte");
            ui.label("Transp");
            ui.add(egui::DragValue::new(&mut self.harmonie.note_value_add).range(-11..=11));
        });
    }

    /// Notenmengen: Bänder aus der Quintenzirkel-Auswahl, mit Endspalte.
    fn zeile_notenmenge(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.harmonie.notenmenge, "Notenmenge");
            ui.label(
                egui::RichText::new("Bänder kommen über „Notenmenge (Bild-Tab)“ aus dem Kreis")
                    .small()
                    .color(Color32::GRAY),
            );
            if !self.harmonie.sets.is_empty() && ui.button("Bänder löschen").clicked() {
                self.harmonie.sets.clear();
                self.harmonie.setx.clear();
                self.harmonie.notessetcount = 0;
            }
        });
        let schritte = self.schritte().max(1) as i32;
        let mut loeschen = None;
        for i in 0..self.harmonie.sets.len() {
            ui.horizontal(|ui| {
                ui.label(format!("B{}: {:?}", i + 1, self.harmonie.sets[i]));
                ui.label("bis");
                if let Some(bis) = self.harmonie.setx.get_mut(i).and_then(|v| v.first_mut()) {
                    ui.add(egui::DragValue::new(bis).range(1..=schritte));
                }
                if ui.button("x").clicked() {
                    loeschen = Some(i);
                }
            });
        }
        if let Some(i) = loeschen {
            if i < self.harmonie.sets.len() {
                self.harmonie.sets.remove(i);
            }
            if i < self.harmonie.setx.len() {
                self.harmonie.setx.remove(i);
            }
            self.harmonie.notessetcount = self.harmonie.sets.len() as i32;
        }
    }

    /// Bild mit Zoom (zwei Finger), Verschiebung (drei Finger) und Maskenleiste.
    fn bild_zeichnen(&mut self, ui: &mut egui::Ui, platz: f32, flaeche: f32, schritte: u16) {
        // Mehrfinger: zwei Finger zoomen, drei Finger verschieben.
        if let Some((zoom, pan)) = self.finger.schritt(ui).geste {
            self.bild_zoom = (self.bild_zoom * zoom).clamp(ZOOM_MIN, ZOOM_MAX);
            self.bild_versatz.0 += pan.0;
            self.bild_versatz.1 += pan.1;
            log::info!(
                "gesten: {} Finger, Zoom {zoom:.2}, Weg ({:.0}, {:.0}), jetzt {:.0} %",
                self.finger.finger(),
                pan.0,
                pan.1,
                self.bild_zoom * 100.0
            );
        }
        let (w, h) = self.textur_masse;
        let Some(texture) = self.texture.clone() else {
            ui.colored_label(Color32::GRAY, "Keine Vorschau (Kamera an?)");
            return;
        };
        if w == 0 || h == 0 {
            ui.colored_label(Color32::GRAY, "Keine Vorschau (Kamera an?)");
            return;
        }
        let verhaeltnis = h as f32 / w as f32;
        // Bildgroesse in Punkten; bei Zoom 1 passt es in die Breite.
        let bild_b = platz * self.bild_zoom;
        let bild_h = bild_b * verhaeltnis;
        // Der Kasten ist hoechstens so hoch wie der Anteil der Flaeche, damit die
        // Regler darunter im Querformat sichtbar bleiben.
        let kasten_h = bild_h
            .min(flaeche * Self::bild_anteil(platz, flaeche))
            .max(60.0);
        let kasten = egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(platz, kasten_h));
        // Bild mittig im Kasten, plus Verschiebung.
        // Das Bild bleibt im Kasten: verschoben werden darf nur, was wirklich
        // ueber den Rand hinausragt.
        let max_vx = ((bild_b - platz) / 2.0).max(0.0);
        let max_vy = ((bild_h - kasten_h) / 2.0).max(0.0);
        self.bild_versatz.0 = self.bild_versatz.0.clamp(-max_vx, max_vx);
        self.bild_versatz.1 = self.bild_versatz.1.clamp(-max_vy, max_vy);
        let bild_rect = egui::Rect::from_min_size(
            egui::pos2(
                kasten.center().x - bild_b / 2.0 + self.bild_versatz.0,
                kasten.center().y - bild_h / 2.0 + self.bild_versatz.1,
            ),
            egui::vec2(bild_b, bild_h),
        );
        let skala = bild_b / w as f32;
        let (_, response) = ui.allocate_exact_size(kasten.size(), egui::Sense::click_and_drag());
        {
            // Auf den Kasten beschnitten: Zoom darf ueber den Rand hinausgehen.
            let painter = ui.painter().with_clip_rect(kasten);
            painter.rect_filled(kasten, 0.0, Color32::from_rgb(12, 12, 14));
            painter.image(
                texture.id(),
                bild_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let finger_unten = response.is_pointer_button_down_on()
            || response.dragged_by(egui::PointerButton::Primary);

        // Die Leiste deckt nur den Scan-Bereich ab — dort, wo der Sweep laeuft.
        let (bereich_links, bereich_rechts) = self.bereich();
        let leiste = egui::Rect::from_min_size(
            egui::pos2(
                bild_rect.min.x + bereich_links * bild_b,
                bild_rect.min.y.max(kasten.min.y),
            ),
            egui::vec2((bereich_rechts - bereich_links) * bild_b, LEISTE_H),
        );
        let sichtbar = leiste.intersect(kasten);
        if sichtbar.width() > 1.0 {
            let painter = ui.painter().with_clip_rect(sichtbar);
            painter.rect_filled(sichtbar, 0.0, Color32::from_black_alpha(150));
            for (von, bis, alpha) in self.maske.abschnitte(schritte as usize) {
                let n = schritte.max(1) as f32;
                let x0 = leiste.min.x + von as f32 / n * leiste.width();
                let x1 = leiste.min.x + bis as f32 / n * leiste.width();
                let farbe = if alpha > 0.0 {
                    Color32::from_rgba_unmultiplied(60, 205, 115, (alpha * 255.0) as u8)
                } else {
                    Color32::from_rgb(120, 40, 45)
                };
                let stueck = egui::Rect::from_min_size(
                    egui::pos2(x0, sichtbar.min.y + 1.0),
                    egui::vec2((x1 - x0).max(0.5), (sichtbar.height() - 2.0).max(2.0)),
                );
                painter.rect_filled(stueck, 0.0, farbe);
            }
        }

        if let Some(pos) = response.interact_pointer_pos() {
            if sichtbar.contains(pos) {
                // Finger in der Leiste: Spalte umschalten und beim Wischen den
                // Wert weiterschreiben; der Anteil zaehlt im Scan-Bereich.
                // Der rechte Rand gehoert noch zur letzten Spalte.
                let letzte = schritte.max(1) as f32 - 1.0;
                let anteil = (((pos.x - leiste.min.x) / leiste.width().max(1.0))
                    * schritte.max(1) as f32)
                    .floor()
                    .clamp(0.0, letzte) as i32;
                if let Some(eintrag) = self.maske.eintrag_fuer_x(anteil, schritte as usize) {
                    if self.maske_pinsel.is_none() {
                        self.maske_pinsel =
                            Some(self.maske.alpha(anteil, schritte as usize) <= 0.0);
                    }
                    let durch = self.maske_pinsel.unwrap_or(true);
                    if (self.maske.alpha(anteil, schritte as usize) > 0.0) != durch {
                        self.maske.setze(eintrag, durch);
                        self.status = format!(
                            "Maske: Schritt {anteil} {} (von {})",
                            if durch { "an" } else { "aus" },
                            schritte
                        );
                        log::info!(
                            "maske Schritt {anteil}/{} {}",
                            schritte,
                            if durch { "an" } else { "aus" }
                        );
                    }
                }
            } else if finger_unten && bild_rect.contains(pos) {
                // Finger im Bild: einfrieren + Farbe waehlen.
                if self.eingefroren.is_none() {
                    self.eingefroren = camera::letztes_bild();
                }
                let px = ((pos.x - bild_rect.min.x) / skala) as usize;
                let py = ((pos.y - bild_rect.min.y) / skala) as usize;
                if let Some(farbe) = self.farbe_an(px, py) {
                    // Die x-Stelle merken, an der gerade getippt wurde: der
                    // Harmoniekreis setzt dort seine Grenze (Desktop
                    // `notessetx[c] = x`).
                    self.tipp_spalte = Some(
                        ((px as f32 / self.bild_w.max(1) as f32) * self.schritte() as f32) as i32,
                    );
                    if self.sel != Some(farbe) {
                        self.sel = Some(farbe);
                        self.markieren(ui.ctx());
                        self.status = format!(
                            "Auswahl RGB({}, {}, {}) - {} Pixel in Toleranz",
                            farbe.0, farbe.1, farbe.2, self.treffer
                        );
                        log::info!(
                            "Auswahl RGB({}, {}, {}), {} Pixel (Toleranz {:?})",
                            farbe.0,
                            farbe.1,
                            farbe.2,
                            self.treffer,
                            self.toleranz
                        );
                    }
                }
            }
        }
        if !finger_unten {
            // Loslassen: Pinsel und Freeze beenden, die Anzeige bleibt.
            self.maske_pinsel = None;
            self.eingefroren = None;
        }
    }
}
