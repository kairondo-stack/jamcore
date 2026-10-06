//! Bild-Screen: Kamera-Vorschau (S7c). Auswahl, Malen und Scan folgen in S7d/S7e.

use eframe::egui::{self, Color32, TextureOptions};

use crate::camera;

pub struct ImagePanel {
    texture: Option<egui::TextureHandle>,
    letzte_nummer: u64,
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
            letzte_nummer: 0,
            status: "Kamera starten, dann erscheint die Vorschau.".to_string(),
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
        self.status = "Kamera gestoppt".to_string();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // ~20 Bilder je Sekunde reichen fuer die Vorschau und sparen Strom.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));

        let (nummer, breite, hoehe) = camera::stand();
        if nummer != self.letzte_nummer {
            if let Some((w, h, rgba)) = camera::letztes_bild() {
                let bild = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                match self.texture.as_mut() {
                    Some(texture) => texture.set(bild, TextureOptions::LINEAR),
                    None => {
                        self.texture = Some(ui.ctx().load_texture(
                            "kamerabild",
                            bild,
                            TextureOptions::LINEAR,
                        ));
                    }
                }
                self.letzte_nummer = nummer;
                self.status = format!("Frame {nummer} ({w}x{h})");
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Kamera an").clicked() {
                self.kamera_an();
            }
            if ui.button("Kamera aus").clicked() {
                self.kamera_aus();
            }
            ui.label(&self.status);
        });

        match self.texture.as_ref() {
            Some(texture) => {
                let platz = ui.available_width();
                let skala = platz / breite.max(1) as f32;
                let groesse = egui::vec2(platz, hoehe.max(1) as f32 * skala);
                ui.image(egui::load::SizedTexture::new(texture.id(), groesse));
            }
            None => {
                ui.colored_label(Color32::GRAY, "Keine Vorschau (Kamera aus?)");
            }
        }
    }
}
