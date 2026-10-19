//! Laub-Tab (Plan `docs/plans/2026-10-18-laub-rhythmus.md`, Step 3):
//! Foto aufnehmen, Bild **oder Video** laden → Blätter finden → Abstände
//! anzeigen → Genre, Raster und Modus wählen → „als Rhythmus rendern“ schreibt
//! die Schrittnummern ins Rhythmus-Raster.
//!
//! Das Rechnen liegt im Kern (`jamcore::laub`, Step 1+2); hier stehen
//! Aufnahme, Anzeige, Bedienung und der Weg ins Raster
//! ([`crate::rhythm::RhythmPanel::laub_setzen`]). Das Standbild liegt als RGB8
//! in [`LaubPanel::bild`] (die Kamera liefert RGBA), die Anzeige als Textur.
//!
//! # Video („Video laden …“)
//!
//! Der Videodekoder (`crate::video`, Java-Brücke `VideoBridge`) liefert seine
//! Bilder in denselben Framepuffer wie die Kamera. Statt zu warten, bis das
//! Video durch ist, sieht [`LaubPanel::video_schritt`] (von der JamApp je
//! Anzeigeframe gerufen, auch außerhalb des Tabs) **bis zu
//! [`VIDEO_FRAMES_MAX`] Bilder** an und behält das mit den **meisten
//! Blättern**. Danach wird der Dekoder gestoppt und dieses Bild läuft durch
//! dieselbe Kette wie ein Foto.
//!
//! Warum der beste **einzelne** Frame und keine Sammlung über die Zeit: beim
//! Filmen bewegt sich die Kamera, dasselbe Blatt liegt in verschiedenen Bildern
//! an verschiedenen Bildpositionen — ein Vereinigen in Bildkoordinaten würde
//! es mehrfach zählen. Ein einzelnes Bild hält die Abstände ehrlich.
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

use std::collections::HashMap;

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

/// Wie viele Videobilder höchstens geprüft werden (bei [`VIDEO_FPS`] ≈ 2 s
/// Dekodierzeit; die Auswertung je Bild kommt hinzu — auf dem Pixel 10 Pro
/// dauerten 30 Bilder eines 1080p-Videos gemessen ≈ 6 s).
const VIDEO_FRAMES_MAX: usize = 30;

/// Bildrate beim Video-Prüfen: 15 fps (30 Bilder in ≈ 2 s Dekodierzeit).
const VIDEO_FPS: u32 = 15;

/// Frist ohne ein einziges Bild (Sekunden): kommt in dieser Zeit kein Frame,
/// gilt der Dekoder als hängend und das Prüfen endet mit Meldung.
const VIDEO_START_FRIST_S: u64 = 10;

/// Obergrenze für das ganze Prüfen (Sekunden) — nur eine Notbremse. Die 30
/// Bilder sollen normal durchlaufen: gemessen auf dem Pixel 10 Pro dauerten
/// sie ≈ 6 s (1080p, 360x640 analysiert) bzw. ≈ 13 s (4K, 2160x3840).
const VIDEO_FRIST_S: u64 = 40;

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
    /// Zuletzt ausgewertete Framenummer aus [`camera::stand`].
    letzte_nummer: u64,
    /// Wie viele Bilder geprüft wurden.
    geprueft: usize,
    /// Das bisher beste Bild (die meisten Blätter).
    bester: Option<BestesBild>,
}

/// Das bisher beste Videobild samt Zählung.
struct BestesBild {
    /// Wie vieltes geprüftes Bild (1-basiert) — „bester Frame 17/30“.
    index: usize,
    w: usize,
    h: usize,
    /// Das Bild in Analysegröße (lange Kante höchstens [`BILD_KANTE`]).
    rgba: Vec<u8>,
    /// Gefundene Blätter in diesem Bild.
    blaetter: usize,
}

pub struct LaubPanel {
    /// Anzeige-Textur des Standbilds.
    texture: Option<egui::TextureHandle>,
    /// Neues Bild für die Textur, bis der nächste `ui`-Aufruf es hochlädt.
    textur_offen: Option<(usize, usize, Vec<u8>)>,
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
    /// Laufendes Video-Prüfen („Video laden …“), sonst `None`.
    videolauf: Option<Videolauf>,
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

impl LaubPanel {
    pub fn new() -> Self {
        Self {
            texture: None,
            textur_offen: None,
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
            videolauf: None,
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
                    geprueft: 0,
                    bester: None,
                });
                self.status = format!(
                    "Video läuft: {name} — bis zu {VIDEO_FRAMES_MAX} Bilder werden geprüft …"
                );
                log::info!(
                    "laub: Video startet: {pfad} ({VIDEO_FPS} fps, bis zu {VIDEO_FRAMES_MAX} Bilder)"
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

    /// Ein Schritt des Video-Prüfens: **ein** neues Bild ansehen, Blätter
    /// zählen, das beste behalten; nach [`VIDEO_FRAMES_MAX`] Bildern (oder
    /// wenn gar nichts kommt bzw. [`VIDEO_FRIST_S`] abgelaufen ist)
    /// abschließen. Die JamApp ruft das je Anzeigeframe auf — auch außerhalb
    /// des Laub-Tabs, damit der Dekoder nicht im Hintergrund weiterläuft.
    pub fn video_schritt(&mut self, ctx: &egui::Context) {
        let Some(mut lauf) = self.videolauf.take() else {
            return;
        };
        let (nummer, _, _) = camera::stand();
        if nummer != lauf.letzte_nummer {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                // Nach dem Lesen noch einmal fragen: kommt während der
                // Auswertung ein neues Bild, wird es übersprungen statt
                // doppelt gezählt.
                lauf.letzte_nummer = camera::stand().0.max(nummer);
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
                    self.status = format!(
                        "Video prüft Bild {}/{} … ({blaetter} Blätter)",
                        lauf.geprueft, VIDEO_FRAMES_MAX
                    );
                }
            }
        }
        if lauf.geprueft >= VIDEO_FRAMES_MAX
            || (lauf.geprueft == 0
                && lauf.start.elapsed() >= std::time::Duration::from_secs(VIDEO_START_FRIST_S))
            || lauf.start.elapsed() >= std::time::Duration::from_secs(VIDEO_FRIST_S)
        {
            self.video_beenden(lauf, ctx);
        } else {
            self.videolauf = Some(lauf);
            // Ohne Nutzereingabe zeichnet egui nicht neu — der nächste Schritt
            // braucht aber einen neuen Durchlauf.
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
        self.status = format!(
            "Video geladen, bester Frame {}/{} ({} Blätter)",
            best.index, lauf.geprueft, best.blaetter
        );
        log::info!(
            "laub: Video geladen: {} ({}x{}, {} Frames geprueft, bester Frame {}, {} Blaetter)",
            lauf.pfad,
            best.w,
            best.h,
            lauf.geprueft,
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

    /// Ein RGBA-Standbild übernehmen: RGB8 für die Erkennung ablegen, die
    /// Textur beim nächsten `ui` hochladen, Hintergrund (falls automatisch)
    /// und Blätter neu rechnen.
    fn bild_uebernehmen(&mut self, w: usize, h: usize, rgba: Vec<u8>) {
        if w == 0 || h == 0 || rgba.len() != w * h * 4 {
            self.status = "Bild hat unerwartete Maße".to_string();
            return;
        }
        let rgb = rgb_aus_rgba(&rgba);
        self.bild = Some((w, h, rgb));
        self.textur_offen = Some((w, h, rgba));
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
    fn eingabe_abstaende(&self) -> Vec<f32> {
        match self.modus() {
            Abstandsmodus::Relativ => self.abstaende.clone(),
            Abstandsmodus::Absolut { .. } => self
                .abstaende
                .iter()
                .map(|px| px / self.px_pro_meter.max(1.0))
                .collect(),
        }
    }

    /// „als Rhythmus rendern“: die Schrittnummern des Kerns ins aktuelle
    /// Rhythmus-Raster schreiben (Farbe je Schritt → Instrument).
    fn rendern(&mut self, rhythm: &mut crate::rhythm::RhythmPanel) {
        let Some(g) = genre(GENRES[self.genre.min(GENRES.len() - 1)]) else {
            self.status = "Unbekanntes Genre".to_string();
            return;
        };
        if self.abstaende.is_empty() {
            self.status = "Keine Abstände — erst Blätter erkennen.".to_string();
            return;
        }
        let modus = self.modus();
        let eingabe = self.eingabe_abstaende();
        let schritte = schritte_aus_abstaenden_bpm(&eingabe, self.raster, modus, &g, self.bpm);
        let modus_text = if self.relativ { "relativ" } else { "absolut" };
        log::info!(
            "laub: Genre {}, Raster {}, {} -> {} Schritte",
            g.name,
            self.raster,
            modus_text,
            schritte.len()
        );
        if schritte.is_empty() {
            self.status = "Keine Schritte — Abstände, Genre und Raster prüfen.".to_string();
            return;
        }
        let farben = farben_je_schritt(
            &self.blaetter,
            &eingabe,
            self.raster,
            modus,
            &g,
            self.bpm,
            &schritte,
        );
        let takt: Vec<(u32, u8)> = schritte
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
            .collect();
        let noten = rhythm.laub_setzen(&takt, self.raster);
        self.status = format!("ins Raster geschrieben ({noten} Noten)");
        log::info!(
            "laub: ins Raster geschrieben ({noten} Noten, {} Schritte je Takt)",
            schritte.len()
        );
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
    }

    /// Der Tab: Kamera-Zeile, Bild mit Markern, Erkennung und Steuerung.
    ///
    /// Das Video-Prüfen ruft die JamApp je Frame ([`LaubPanel::video_schritt`],
    /// auch außerhalb des Tabs) — hier kommt nur der fertige Zustand an.
    pub fn ui(&mut self, ui: &mut egui::Ui, rhythm: &mut crate::rhythm::RhythmPanel) -> Aktion {
        if let Some((w, h, rgba)) = self.textur_offen.take() {
            self.textur_setzen(ui.ctx(), w, h, &rgba);
        }
        let aktion = self.kopfzeile(ui);
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
                    Ok(()) => "Kamera startet …".to_string(),
                    Err(e) => format!("Kamera: {e}"),
                };
            }
            if ui.button("Foto aufnehmen").clicked() {
                self.foto_aufnehmen();
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
        });
        aktion
    }

    /// Das Standbild mit den Blättern: Kreise in Wegrichtung nummeriert, der
    /// Weg als Linie; ein Tipp ins Bild wählt die Hintergrundfarbe.
    fn bild_zeichnen(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = self.texture.clone() else {
            ui.colored_label(
                Color32::GRAY,
                "Noch kein Bild: „Kamera an“ + „Foto aufnehmen“, „Bild laden …“ oder \
                 „Video laden …“.",
            );
            return;
        };
        let Some((w, h, _)) = self.bild.as_ref() else {
            return;
        };
        let (w, h) = (*w, *h);
        let platz = ui.available_width().max(80.0);
        let flaeche = ui.available_height().max(120.0);
        let skala = (platz / w as f32).min(flaeche / h as f32);
        let bild_b = (w as f32 * skala).max(1.0);
        let bild_h = (h as f32 * skala).max(1.0);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(platz, bild_h), egui::Sense::click());
        let bild_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(bild_b, bild_h));
        {
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 12, 14));
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), bild_rect, uv, Color32::WHITE);

            // Blätter: Kreis (Radius aus der Fläche) mit Nummer in Wegrichtung;
            // die Linie zwischen den Mittelpunkten ist der Weg.
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
                painter.circle_stroke(p, r, Stroke::new(2.0, gelb));
                painter.text(
                    p + egui::vec2(r + 6.0, -4.0),
                    Align2::LEFT_BOTTOM,
                    format!("{}", i + 1),
                    FontId::proportional(13.0),
                    gelb,
                );
            }
        }

        // Tipp ins Bild: die Farbe an der Stelle wird der Hintergrund (wie die
        // Farbwahl im Bild-Tab), danach neu erkennen.
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

    /// Erkennung (Hintergrund, Toleranz, Abstände), Steuerung (Genre, Raster,
    /// Modus, BPM) und „als Rhythmus rendern“.
    fn steuerung(&mut self, ui: &mut egui::Ui, rhythm: &mut crate::rhythm::RhythmPanel) {
        ui.separator();
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
            if ui
                .add_enabled(
                    !self.abstaende.is_empty(),
                    egui::Button::new("als Rhythmus rendern"),
                )
                .clicked()
            {
                self.rendern(rhythm);
            }
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }
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
}
