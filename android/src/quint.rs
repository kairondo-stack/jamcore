//! Quintenzirkel als Touch-Scheibe (S7d-b).
//!
//! Wie im Desktop-rrn5: zwölf Segmente, ein Tipp toggelt ein Segment und
//! sendet beim Hinzufügen sofort `note_on` (Port 7000, gemappt auf die
//! OSC-Basis). Die Auswahl ergibt die Notenklassen der Harmonie
//! (`jamcore::quint::note_classes`).

use eframe::egui::{self, Color32, Pos2, Shape, Stroke};

use jamcore::osc::{OscOut, Renoise};
use jamcore::quint::{
    akkord_aus_stufen, note_classes, segment_angles, segment_index_at, toggle_stufe, Akkord, CIRCLE,
};
use log::info;

const SEGMENTE: usize = 12;
const R_INNEN: f32 = 55.0;
const R_AUSSEN: f32 = 130.0;

pub struct QuintPanel {
    ausgewaehlt: Vec<usize>,
    /// Gemerkter Akkord aus den Segmenten (None = keiner gemerkt).
    akkord: Option<Akkord>,
    /// x-Bereich der Bildspalten als Anteile 0..1 (`von ≤ bis`) — derselbe
    /// Wert wie in Bild/B2 (`BildPanel::bereich`, dort `scan_links`/`scan_rechts`).
    x_von: f32,
    x_bis: f32,
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
            akkord: None,
            x_von: 0.0,
            x_bis: 1.0,
            host: jamcore::osc::DEFAULT_HOST.to_string(),
            port: jamcore::osc::DEFAULT_BASE_PORT.to_string(),
            status: "Segment tippen: Note klingt und kommt in die Harmonie.".to_string(),
        }
    }

    pub fn set_target(&mut self, host: &str, port: &str) {
        self.host = host.trim().to_string();
        self.port = port.trim().to_string();
    }

    /// x-Bereich der Bildspalten als Anteile (0.0..=1.0), immer `von ≤ bis`.
    /// Die App reicht den Wert an Bild und B2 durch, damit alle drei
    /// denselben Bereich zeigen (wie `BildPanel::bereich()`).
    pub fn x_bereich(&self) -> (f32, f32) {
        let von = if self.x_von.is_finite() {
            self.x_von.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bis = if self.x_bis.is_finite() {
            self.x_bis.clamp(0.0, 1.0)
        } else {
            1.0
        };
        (von.min(bis), von.max(bis))
    }

    /// x-Bereich setzen (Anteile 0..1); klemmt und dreht vertauschte
    /// Grenzen, damit immer `von ≤ bis` gilt.
    pub fn setze_x_bereich(&mut self, von: f32, bis: f32) {
        let von = if von.is_finite() {
            von.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bis = if bis.is_finite() {
            bis.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.x_von = von.min(bis);
        self.x_bis = von.max(bis);
    }

    /// Der gemerkte Akkord (Stufen + Name) für die App.
    pub fn akkord(&self) -> Option<&Akkord> {
        self.akkord.as_ref()
    }

    /// Aktuelle Segmentauswahl als Akkord merken; den Namen liefert die
    /// Kerntabelle (`akkord_aus_stufen`, unbekannt = "?").
    fn akkord_merken(&mut self) {
        let mut stufen: Vec<u8> = Vec::new();
        for &i in &self.ausgewaehlt {
            toggle_stufe(&mut stufen, i as u8);
        }
        match akkord_aus_stufen(&stufen) {
            Some(akkord) => {
                self.status = format!(
                    "Akkord gemerkt: {} (Stufen {:?})",
                    akkord.name, akkord.stufen
                );
                info!("quint: Akkord gemerkt: {} {:?}", akkord.name, akkord.stufen);
                self.akkord = Some(akkord);
            }
            None => self.status = "Keine Segmente ausgewählt.".to_string(),
        }
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
            // Stufen des gemerkten Akkords bleiben im Rad markiert (gold),
            // auch wenn die aktuelle Auswahl sich ändert.
            let im_akkord = self
                .akkord
                .as_ref()
                .is_some_and(|a| a.stufen.contains(&(i as u8)));
            let farbe = if im_akkord {
                Color32::from_rgb(255, 170, 0)
            } else if ausgewaehlt {
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
            let text_farbe = if ausgewaehlt || im_akkord {
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
        ui.horizontal_wrapped(|ui| {
            if !self.ausgewaehlt.is_empty() && ui.button("Auswahl löschen").clicked() {
                self.ausgewaehlt.clear();
                self.status = "Harmonie geleert.".to_string();
            }
            // Die aktuell ausgewählten Segmente als Akkord merken.
            if !self.ausgewaehlt.is_empty() && ui.button("als Akkord merken").clicked() {
                self.akkord_merken();
            }
        });
        if let Some(akkord) = self.akkord().cloned() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Akkord: {} (Stufen {:?})",
                        akkord.name, akkord.stufen
                    ))
                    .small(),
                );
                if ui.button("Akkord löschen").clicked() {
                    self.akkord = None;
                    self.status = "Akkord gelöscht.".to_string();
                }
            });
        }
        let klassen = note_classes(&self.ausgewaehlt);
        ui.label(format!("Notenklassen: {klassen:?}"));

        // x-Bereich der Bildspalten als Anteile 0..100 % — derselbe Wert in
        // Kreis, Bild und B2 (das Durchreichen macht die App in `lib.rs`).
        ui.separator();
        ui.label(
            egui::RichText::new("x-Bereich der Bildspalten (Kreis, Bild, B2 — ein Wert)")
                .small()
                .color(Color32::DARK_GRAY),
        );
        let (von, bis) = self.x_bereich();
        let mut x_von = (von * 100.0).round();
        let mut x_bis = (bis * 100.0).round();
        let mut geaendert = false;
        ui.horizontal(|ui| {
            ui.label("x von");
            geaendert |= ui
                .add(egui::Slider::new(&mut x_von, 0.0..=x_bis).suffix(" %"))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("x bis");
            geaendert |= ui
                .add(egui::Slider::new(&mut x_bis, x_von..=100.0).suffix(" %"))
                .changed();
        });
        if geaendert {
            self.setze_x_bereich(x_von / 100.0, x_bis / 100.0);
            let (von, bis) = self.x_bereich();
            self.status = format!("x-Bereich {:.0}–{:.0} %", von * 100.0, bis * 100.0);
        }
        ui.horizontal(|ui| {
            if ui.button("ganzes Bild").clicked() {
                self.setze_x_bereich(0.0, 1.0);
                self.status = "x-Bereich: ganzes Bild.".to_string();
            }
            let (von, bis) = self.x_bereich();
            ui.label(
                egui::RichText::new(format!("x {:.0}–{:.0} %", von * 100.0, bis * 100.0)).small(),
            );
        });

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
