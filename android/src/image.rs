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

use jamcore::livescan::{next_column, note_allowed, pixel_matches, scan_column, ScanFilter};
use jamcore::osc::{OscOut, Renoise};

use crate::camera;
use crate::video;

const TOLERANZ_STANDARD: i32 = 24;

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
    toleranz: i32,
    treffer: u64,
    status: String,
    /// Live-Scan (S7e): Sweep ueber die Spalten, Treffer als OSC-Noten.
    scan_an: bool,
    scan_takt: bool,
    scan_bpm: f32,
    scan_x: i32,
    scan_puffer: Option<(usize, usize, Vec<u8>)>,
    scan_naechste_spalte: std::time::Instant,
    scan_letzte_note: std::time::Instant,
    scan_max_noten: u32,
    sweep_noten: u64,
    sweep_gezeichnet: u64,
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
            bg: (0, 0, 0),
            toleranz: TOLERANZ_STANDARD,
            treffer: 0,
            status: "Kamera starten, dann ins Bild tippen (halten = einfrieren).".to_string(),
            scan_an: false,
            scan_takt: false,
            scan_bpm: 120.0,
            scan_x: 0,
            scan_puffer: None,
            scan_naechste_spalte: std::time::Instant::now(),
            scan_letzte_note: std::time::Instant::now(),
            scan_max_noten: 8,
            sweep_noten: 0,
            sweep_gezeichnet: 0,
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
        let ordner = std::path::PathBuf::from(basis).join("videos");
        let _ = std::fs::create_dir_all(&ordner);
        let Ok(eintraege) = std::fs::read_dir(&ordner) else {
            return;
        };
        for eintrag in eintraege.flatten() {
            let pfad = eintrag.path();
            let ist_video = pfad
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| {
                    ["mp4", "mkv", "webm", "avi", "mov", "3gp"]
                        .iter()
                        .any(|x| e.eq_ignore_ascii_case(x))
                })
                .unwrap_or(false);
            if ist_video {
                self.videos.push(pfad.display().to_string());
            }
        }
        self.videos.sort();
        if self.video_auswahl.is_none() {
            self.video_auswahl = self.videos.first().cloned();
        }
        self.status = format!("{} Videos gefunden", self.videos.len());
    }

    fn video_an(&mut self) {
        let Some(pfad) = self.video_auswahl.clone() else {
            self.status = "Kein Video ausgewählt (erst suchen).".to_string();
            return;
        };
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

    /// Tempo fuer den Takt-Scan von aussen setzen (Session-Follow).
    pub fn set_bpm(&mut self, bpm: f32) {
        self.scan_bpm = bpm.clamp(20.0, 300.0);
    }

    fn note_senden(&mut self, note: i32, volume: i32) {
        let (host, port) = (
            self.host.clone(),
            self.port.parse().unwrap_or(jamcore::osc::DEFAULT_BASE_PORT),
        );
        match OscOut::new(&host, port)
            .and_then(|out| Renoise::new(&out).note_on(7000, note, volume))
        {
            Ok(used) => log::info!("bild: Scan-Note {note} -> {host}:{used}"),
            Err(e) => self.status = format!("OSC-Fehler: {e}"),
        }
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
        let Some(sel) = self.sel else {
            return;
        };
        let Some((w, h, mut bild)) = self.eingefroren.clone().or_else(camera::letztes_bild) else {
            return;
        };
        let filter = ScanFilter {
            bg: self.bg,
            sel,
            range: (self.toleranz, self.toleranz, self.toleranz),
        };
        let mut treffer = 0u64;
        for i in 0..w * h {
            let idx = i * 4;
            if idx + 2 >= bild.len() {
                break;
            }
            let (r, g, b) = (bild[idx], bild[idx + 1], bild[idx + 2]);
            if pixel_matches(r, g, b, &filter) {
                bild[idx] = 20;
                bild[idx + 1] = 235;
                bild[idx + 2] = 60;
                treffer += 1;
            }
        }
        self.treffer = treffer;
        self.textur_setzen(ctx, w, h, &bild);
    }

    /// Ein Sweep-Schritt: Spalten scannen, Treffer als OSC-Noten senden.
    ///
    /// Wie im Desktop-rrn5: "frei" laeuft mit fester Spaltenrate, "Takt" macht
    /// einen vollen Durchlauf in einem Beat. Notenwert aus der Zeile
    /// (oben hoch, unten tief), Lautstaerke 127, Rate ueber `scan_max_noten`.
    fn scan_schritt(&mut self, ctx: &egui::Context) {
        if !self.scan_an {
            return;
        }
        let Some(sel) = self.sel else {
            return;
        };
        let Some((w, h, live)) = camera::letztes_bild() else {
            return;
        };
        let (wi, hi) = (w as i32, h as i32);
        let schritt = 2;
        let schritte = ((wi + schritt - 1) / schritt).max(1) as i64;
        let spalten_ns: i64 = if self.scan_takt {
            // Ein voller Sweep dauert einen Beat; die Spaltenzahl richtet
            // sich nach der Schrittweite, sonst waere der Sweep zu kurz.
            ((60.0 / self.scan_bpm.max(20.0) * 1.0e9) as i64 / schritte).max(1)
        } else {
            2_500_000
        };
        let budget = std::time::Duration::from_millis(10);
        let start = std::time::Instant::now();
        let filter = ScanFilter {
            bg: self.bg,
            sel,
            range: (self.toleranz, self.toleranz, self.toleranz),
        };
        let mut noten: Vec<i32> = Vec::new();
        while start.elapsed() < budget {
            let jetzt = std::time::Instant::now();
            if jetzt < self.scan_naechste_spalte {
                std::thread::sleep(self.scan_naechste_spalte - jetzt);
                continue;
            }
            if self.scan_x == 0 {
                // Sweep beginnt: Puffer leeren (nur die Treffer leuchten).
                self.scan_puffer = Some((w, h, vec![0u8; w * h * 4]));
            }
            let x = self.scan_x.clamp(0, wi - 1);
            let treffer = scan_column(&live, wi, hi, x, &filter, None);
            if let Some((_, _, puffer)) = self.scan_puffer.as_mut() {
                for (y, r, g, b) in &treffer {
                    let idx = (*y as usize) * w + x as usize;
                    let idx = idx * 4;
                    if idx + 2 < puffer.len() {
                        // "gezeichnet": Treffer in ihrer Farbe in den Puffer.
                        puffer[idx] = *r;
                        puffer[idx + 1] = *g;
                        puffer[idx + 2] = *b;
                        puffer[idx + 3] = 255;
                        // Note aus der Zeile: oben hoch, unten tief.
                        noten.push(72 - (*y * 36 / hi.max(1)));
                        self.sweep_gezeichnet += 1;
                    }
                }
            }
            for note in noten.drain(..) {
                if note_allowed(self.scan_letzte_note.elapsed(), self.scan_max_noten) {
                    self.scan_letzte_note = std::time::Instant::now();
                    self.note_senden(note, 127);
                    self.sweep_noten += 1;
                }
            }
            let next = next_column(x, 0, wi, schritt);
            if next == 0 {
                log::info!(
                    "bild: Sweep beendet, {} Noten, {} gezeichnet ({})",
                    self.sweep_noten,
                    self.sweep_gezeichnet,
                    if self.scan_takt { "Takt" } else { "frei" }
                );
                self.sweep_noten = 0;
                self.sweep_gezeichnet = 0;
                self.scan_puffer = None;
            }
            self.scan_x = next;
            self.scan_naechste_spalte += std::time::Duration::from_nanos(spalten_ns.max(1) as u64);
        }
        if let Some((w, h, puffer)) = self.scan_puffer.clone() {
            self.textur_setzen(ctx, w, h, &puffer);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // ~20 Bilder je Sekunde reichen fuer die Vorschau und sparen Strom.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));

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
                self.status = format!(
                    "Scan {} ({}), {} Noten/s",
                    if self.scan_an { "an" } else { "aus" },
                    if self.scan_takt { "Takt" } else { "frei" },
                    self.scan_max_noten
                );
            }
            if ui
                .button(if self.scan_takt { "Takt" } else { "frei" })
                .clicked()
            {
                self.scan_takt = !self.scan_takt;
            }
            ui.label("Noten/s");
            ui.add(egui::Slider::new(&mut self.scan_max_noten, 1..=30));
            if self.scan_takt {
                ui.label("BPM");
                ui.add(egui::Slider::new(&mut self.scan_bpm, 60.0..=180.0));
            }
            ui.label(format!(
                "Sweep: {} Noten, {} gezeichnet",
                self.sweep_noten, self.sweep_gezeichnet
            ));
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
            if ui.button("Videos suchen").clicked() {
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
            if ui.button("Video an").clicked() {
                self.video_an();
            }
            if ui.button("Video aus").clicked() {
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

        // Live-Textur nur, solange keine Auswahl steht und kein Scan laeuft.
        let (nummer, _, _) = camera::stand();
        if !self.scan_an && self.sel.is_none() && nummer != self.letzte_nummer {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                self.textur_setzen(ui.ctx(), w, h, &rgba);
                self.letzte_nummer = nummer;
                self.status = format!("Frame {nummer} ({w}x{h})");
            }
        }

        self.scan_schritt(ui.ctx());

        let (w, h) = self.textur_masse;
        if let Some(texture) = self.texture.clone() {
            if w > 0 && h > 0 {
                let platz = ui.available_width();
                let skala = platz / w as f32;
                let groesse = egui::vec2(platz, h as f32 * skala);
                let response = ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(texture.id(), groesse))
                        .sense(egui::Sense::click_and_drag()),
                );

                // Finger gedrueckt = Linksklick: einfrieren + Farbe waehlen.
                let gedrueckt = response.is_pointer_button_down_on();
                if gedrueckt {
                    if self.eingefroren.is_none() {
                        self.eingefroren = camera::letztes_bild();
                    }
                    if let Some(pos) = response.interact_pointer_pos() {
                        let rel = pos - response.rect.min;
                        let px = (rel.x / skala) as usize;
                        let py = (rel.y / skala) as usize;
                        if let Some(farbe) = self.farbe_an(px, py) {
                            if self.sel != Some(farbe) {
                                self.sel = Some(farbe);
                                self.markieren(ui.ctx());
                                self.status = format!(
                                    "Auswahl RGB({}, {}, {}) - {} Pixel in Toleranz",
                                    farbe.0, farbe.1, farbe.2, self.treffer
                                );
                                log::info!(
                                    "bild: Auswahl RGB({}, {}, {}), {} Pixel (Toleranz {})",
                                    farbe.0,
                                    farbe.1,
                                    farbe.2,
                                    self.treffer,
                                    self.toleranz
                                );
                            }
                        }
                    }
                } else if self.eingefroren.is_some() {
                    // Loslassen: Freeze beenden, Markierung bleibt stehen.
                    self.eingefroren = None;
                }
            }
        } else {
            ui.colored_label(Color32::GRAY, "Keine Vorschau (Kamera an?)");
        }

        ui.horizontal(|ui| {
            ui.label("Auswahl:");
            match self.sel {
                Some((r, g, b)) => {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
                    ui.label(format!("RGB({r}, {g}, {b})"));
                }
                None => {
                    ui.label("noch keine");
                }
            }
            ui.label(format!("markiert: {}", self.treffer));
            if ui.button("Auswahl löschen").clicked() {
                self.sel = None;
                self.treffer = 0;
                // Live-Anzeige wieder freigeben (naechster Frame kommt).
            }
        });
        ui.horizontal(|ui| {
            ui.label("Toleranz");
            let geaendert = ui
                .add(egui::Slider::new(&mut self.toleranz, 0..=80))
                .changed();
            if geaendert && self.sel.is_some() {
                let ctx = ui.ctx().clone();
                self.markieren(&ctx);
            }
            ui.label("Hintergrund (übersprungen):");
            let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
            let (r, g, b) = self.bg;
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_rgb(r, g, b));
        });
    }
}
