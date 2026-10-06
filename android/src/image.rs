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

use jamcore::livescan::{pixel_matches, ScanFilter};

use crate::camera;

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
        }
    }

    fn kamera_an(&mut self) {
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

        // Live-Textur nur, solange keine Auswahl steht. Eine Auswahl friert
        // die Anzeige ein (wie der gedrueckte Linksklick im Original).
        let (nummer, _, _) = camera::stand();
        if self.sel.is_none() && nummer != self.letzte_nummer {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                self.textur_setzen(ui.ctx(), w, h, &rgba);
                self.letzte_nummer = nummer;
                self.status = format!("Frame {nummer} ({w}x{h})");
            }
        }

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
