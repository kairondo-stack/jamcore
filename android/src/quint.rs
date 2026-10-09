//! Quintenzirkel als Touch-Scheibe (S7d-b).
//!
//! Wie im Desktop-rrn5: zwölf Segmente, ein Tipp toggelt ein Segment und
//! sendet beim Hinzufügen sofort `note_on` (Port 7000, gemappt auf die
//! OSC-Basis). Die Auswahl ergibt die Notenklassen der Harmonie
//! (`jamcore::quint::note_classes`).

use eframe::egui::{self, Color32, Pos2, Shape, Stroke};

use jamcore::osc::{OscOut, Renoise};
use jamcore::quint::{note_classes, segment_angles, segment_index_at, CIRCLE};
use log::info;

const SEGMENTE: usize = 12;
const R_INNEN: f32 = 55.0;
const R_AUSSEN: f32 = 130.0;

pub struct QuintPanel {
    ausgewaehlt: Vec<usize>,
    host: String,
    port: String,
    status: String,
}

impl Default for QuintPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl QuintPanel {
    pub fn new() -> Self {
        Self {
            ausgewaehlt: Vec::new(),
            host: jamcore::osc::DEFAULT_HOST.to_string(),
            port: jamcore::osc::DEFAULT_BASE_PORT.to_string(),
            status: "Segment tippen: Note klingt und kommt in die Harmonie.".to_string(),
        }
    }

    pub fn set_target(&mut self, host: &str, port: &str) {
        self.host = host.trim().to_string();
        self.port = port.trim().to_string();
    }

    fn note_senden(&mut self, note: i32) {
        let ziel = (
            self.host.clone(),
            self.port.parse().unwrap_or(jamcore::osc::DEFAULT_BASE_PORT),
        );
        let ergebnis = OscOut::new(&ziel.0, ziel.1)
            .and_then(|out| Renoise::new(&out).note_on(7000, note, 127));
        match ergebnis {
            Ok(port) => info!("quint: Note {note} -> {}:{port}", ziel.0),
            Err(e) => self.status = format!("OSC-Fehler: {e}"),
        }
    }

    fn toggle(&mut self, index: usize) {
        if let Some(pos) = self.ausgewaehlt.iter().position(|&i| i == index) {
            self.ausgewaehlt.remove(pos);
        } else {
            self.ausgewaehlt.push(index);
            let note = CIRCLE[index];
            self.note_senden(note);
            info!("quint: Note {note} (Klasse {})", note % 12);
        }
        let klassen = note_classes(&self.ausgewaehlt);
        self.status = format!(
            "Auswahl: {} Segmente, Harmonie (Klassen): {klassen:?}",
            self.ausgewaehlt.len()
        );
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Vec<i32>> {
        let groesse = (R_AUSSEN * 2.0 + 20.0).min(ui.available_width());
        let (response, painter) =
            ui.allocate_painter(egui::vec2(groesse, groesse), egui::Sense::click());
        let mitte = response.rect.center();
        let skala = groesse / (R_AUSSEN * 2.0 + 20.0);
        let r_innen = R_INNEN * skala;
        let r_aussen = R_AUSSEN * skala;

        for i in 0..SEGMENTE {
            let (a0, a1) = segment_angles(i);
            let ausgewaehlt = self.ausgewaehlt.contains(&i);
            let farbe = if ausgewaehlt {
                Color32::from_rgb(0, 200, 255)
            } else {
                Color32::from_rgb(70, 70, 70)
            };
            // Annulus-Segment als Vieleck: außen a0..a1, innen zurück.
            let schritte = 12;
            let mut punkte: Vec<Pos2> = Vec::with_capacity(2 * (schritte + 1));
            for s in 0..=schritte {
                let a = a0 + (a1 - a0) * s as f32 / schritte as f32;
                punkte.push(Pos2::new(
                    mitte.x + r_aussen * a.cos(),
                    mitte.y + r_aussen * a.sin(),
                ));
            }
            for s in (0..=schritte).rev() {
                let a = a0 + (a1 - a0) * s as f32 / schritte as f32;
                punkte.push(Pos2::new(
                    mitte.x + r_innen * a.cos(),
                    mitte.y + r_innen * a.sin(),
                ));
            }
            painter.add(Shape::convex_polygon(
                punkte,
                farbe,
                Stroke::new(1.0, Color32::from_rgb(20, 20, 20)),
            ));

            // Notenname in der Segmentmitte.
            let am = (a0 + a1) / 2.0;
            let rm = (r_innen + r_aussen) / 2.0;
            let pos = Pos2::new(mitte.x + rm * am.cos(), mitte.y + rm * am.sin());
            let text_farbe = if ausgewaehlt {
                Color32::BLACK
            } else {
                Color32::from_rgb(210, 210, 210)
            };
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                format!("{}", CIRCLE[i]),
                egui::FontId::proportional(12.0),
                text_farbe,
            );
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let rx = pos.x - mitte.x;
                let ry = pos.y - mitte.y;
                if let Some(i) = segment_index_at(rx, ry, r_innen, r_aussen) {
                    self.toggle(i);
                }
            }
        }

        ui.label(&self.status);
        if !self.ausgewaehlt.is_empty() && ui.button("Auswahl löschen").clicked() {
            self.ausgewaehlt.clear();
            self.status = "Harmonie geleert.".to_string();
        }
        let klassen = note_classes(&self.ausgewaehlt);
        ui.label(format!("Notenklassen: {klassen:?}"));
        // Die Auswahl als Harmonie-Band in den Bild-Tab übernehmen
        // (Desktop: `useNotesSet`-Mengen aus `quintenzirkel_flow`).
        let mut uebernehmen = None;
        if !klassen.is_empty() && ui.button("Notenmenge (Bild-Tab)").clicked() {
            uebernehmen = Some(klassen.clone());
            self.status = format!("{klassen:?} als Notenmenge übernommen.");
        }
        ui.label(
            egui::RichText::new("Note geht beim Antippen sofort raus (OSC-Ziel aus dem OSC-Tab).")
                .small()
                .color(Color32::DARK_GRAY),
        );
        uebernehmen
    }
}
