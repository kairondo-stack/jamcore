//! Takt-Maske: Tastenfeld, jeder Tastendruck ein Schritt.
//!
//! Tippen auf eine Taste belegt den nächsten Schritt. Ein Wisch zählt **nur**
//! die Taste, bei der er anfängt, und die, bei der er aufhört. Eine gehaltene
//! Taste plus eine zweite ergibt zwei Schritte hintereinander (jeder neu
//! aufgesetzte Finger ist ein Tastendruck).
//!
//! Zwei Feldarten:
//! - **Je Wert**: eine Taste je Wert, 0..=wert_max (z.B. 0..128 = 129 Tasten);
//!   die Spaltenzahl bestimmt die Anordnung.
//! - **Verteilt**: festes Feld (Spalten x Reihen), die Werte gleichmäßig von
//!   0 (Pause) bis wert_max verteilt.

use eframe::egui::{self, Color32, RichText};
use jamcore::grid::{Note, Pattern};
use log::info;

use crate::touch::Mehrfinger;

/// Regenbogenfarbe fuer den Touch-Verlauf (0.0 = rot, 1.0 = violett).
fn regenbogen(anteil: f32) -> Color32 {
    egui::ecolor::Hsva::new(anteil.clamp(0.0, 1.0) * 0.8, 0.85, 0.95, 1.0).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feldart {
    /// Eine Taste je Wert (gut zum gezielten Anschlagen).
    JeWert,
    /// Festes Feld, Werte gleichmäßig verteilt (kompakt).
    Verteilt,
}

pub struct MaskePanel {
    feldart: Feldart,
    spalten: usize,
    reihen: usize,
    wert_max: i32,
    status: String,
    /// Getastete Felder in Druckreihenfolge (Index und Mitte) fuer den Regenbogen.
    spur: Vec<(usize, egui::Pos2)>,
    /// Der zuletzt gesetzte Schritt, damit „Taste halten + andere" den
    /// **nächsten** Schritt belegt.
    naechster_schritt: u16,
    /// Mehrfinger: jeder neu aufgesetzte Finger ist ein weiterer Tastendruck.
    finger: Mehrfinger,
    /// Erst- und Endtaste je Finger (Fingernummer -> (Start, Ende)).
    wisch: std::collections::HashMap<u64, (Option<usize>, Option<usize>)>,
    /// Tastendruecke dieses Bildes (Reihenfolge), ausgeleert von
    /// [`MaskePanel::presse_entnehmen`]. Der Bild-Scan zaehlt daraus seine
    /// Tippfolge: so wird aus dem Feldtippen die Maske (wie KeyEventRender.txt).
    presse: Vec<usize>,
}

impl Default for MaskePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl MaskePanel {
    pub fn new() -> Self {
        Self {
            feldart: Feldart::JeWert,
            spalten: 8,
            reihen: 4,
            wert_max: 32,
            status: "Taste tippen = nächster Schritt; halten + andere Taste = danach.".to_string(),
            spur: Vec::new(),
            naechster_schritt: 0,
            finger: Mehrfinger::neu(),
            wisch: std::collections::HashMap::new(),
            presse: Vec::new(),
        }
    }

    /// Die seit dem letzten Aufruf gedrueckten Tasten (Index je Druck), in
    /// Druckreihenfolge. Der Aufrufer zaehlt daraus die Tippfolge.
    pub fn presse_entnehmen(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.presse)
    }

    fn tasten_anzahl(&self) -> usize {
        match self.feldart {
            Feldart::JeWert => (self.wert_max.max(1) + 1) as usize,
            Feldart::Verteilt => (self.spalten * self.reihen).max(2),
        }
    }

    /// Reihen des Feldes (bei "Je Wert" aus Spalten und Tastenanzahl).
    fn reihen(&self) -> usize {
        match self.feldart {
            Feldart::JeWert => self.tasten_anzahl().div_ceil(self.spalten.max(1)),
            Feldart::Verteilt => self.reihen.max(2),
        }
    }

    /// Wert einer Taste.
    fn wert_der_taste(&self, index: usize) -> i32 {
        match self.feldart {
            Feldart::JeWert => (index as i32).min(self.wert_max),
            Feldart::Verteilt => {
                let n = self.tasten_anzahl();
                if n <= 1 {
                    return 0;
                }
                (index as i32 * self.wert_max / (n as i32 - 1)).clamp(0, 127)
            }
        }
    }

    /// Taste unter einer Position im Feld (None außerhalb).
    /// Mitte einer Taste im Feld.
    fn zentrum(&self, rect: egui::Rect, index: usize) -> egui::Pos2 {
        let reihen = self.reihen();
        let zell_w = rect.width() / self.spalten.max(1) as f32;
        let zell_h = rect.height() / reihen.max(1) as f32;
        let spalte = (index % self.spalten) as f32;
        let reihe = (index / self.spalten) as f32;
        egui::pos2(
            rect.left() + (spalte + 0.5) * zell_w,
            rect.top() + (reihe + 0.5) * zell_h,
        )
    }

    fn taste_unter(&self, rect: egui::Rect, pos: egui::Pos2) -> Option<usize> {
        if !rect.contains(pos) {
            return None;
        }
        let rel = pos - rect.min;
        let reihen = self.reihen();
        let spalte = ((rel.x / rect.width()) * self.spalten as f32) as usize;
        let reihe = ((rel.y / rect.height()) * reihen as f32) as usize;
        if spalte >= self.spalten || reihe >= reihen {
            return None;
        }
        let index = reihe * self.spalten + spalte;
        (index < self.tasten_anzahl()).then_some(index)
    }

    /// Setzt einen Schritt mit dem Wert der Taste und rückt den Schrittzähler vor.
    fn druck_schritt(&mut self, taste: usize, pos: egui::Pos2, pattern: &mut Pattern) {
        let wert = self.wert_der_taste(taste);
        let schritt = self.naechster_schritt % pattern.steps_per_cycle().max(1);
        self.naechster_schritt = self.naechster_schritt.wrapping_add(1);
        self.spur.push((taste, pos));
        if wert > 0 {
            pattern.insert(Note {
                step: schritt,
                instrument: 0,
                pitch: wert,
                glide: false,
            });
            info!(
                "druck: taste {} -> schritt {} (wert {wert})",
                taste + 1,
                schritt + 1
            );
            self.status = format!(
                "Taste {} -> Schritt {} (Wert {wert})",
                taste + 1,
                schritt + 1
            );
        } else {
            info!(
                "druck: taste {} -> schritt {} (pause)",
                taste + 1,
                schritt + 1
            );
            self.status = format!("Taste {} ist Pause (Schritt {}).", taste + 1, schritt + 1);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, pattern: &mut Pattern) {
        ui.heading("Takt-Maske");
        ui.horizontal(|ui| {
            ui.label("Feld");
            if ui
                .selectable_label(self.feldart == Feldart::JeWert, "je Wert")
                .clicked()
            {
                self.feldart = Feldart::JeWert;
            }
            if ui
                .selectable_label(self.feldart == Feldart::Verteilt, "verteilt")
                .clicked()
            {
                self.feldart = Feldart::Verteilt;
            }
            if ui.button("leeren").clicked() {
                pattern.clear();
                self.status = "Raster geleert.".to_string();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Spalten");
            if ui.button("-").clicked() {
                self.spalten = self.spalten.saturating_sub(1).max(2);
            }
            ui.label(format!("{}", self.spalten));
            if ui.button("+").clicked() {
                self.spalten = (self.spalten + 1).min(16);
            }
            if self.feldart == Feldart::Verteilt {
                ui.label("Reihen");
                if ui.button("-").clicked() {
                    self.reihen = self.reihen.saturating_sub(1).max(2);
                }
                ui.label(format!("{}", self.reihen));
                if ui.button("+").clicked() {
                    self.reihen = (self.reihen + 1).min(8);
                }
            }
            ui.label("Wert max");
            if ui.button("-").clicked() {
                self.wert_max = (self.wert_max - 8).max(8);
            }
            ui.label(format!("{}", self.wert_max));
            if ui.button("+").clicked() {
                self.wert_max = (self.wert_max + 8).min(128);
            }
        });
        ui.label(
            RichText::new(format!(
                "{} Tasten (0..{})",
                self.tasten_anzahl(),
                self.wert_max
            ))
            .small()
            .color(Color32::GRAY),
        );

        // Das Tastenfeld: ein Painter fürs Ganze (so kommen die Wischproben mit).
        let breite = ui.available_width();
        let reihen = self.reihen();
        let hoehe = (breite * reihen as f32 / self.spalten.max(1) as f32).min(430.0);
        let (response, painter) =
            ui.allocate_painter(egui::vec2(breite, hoehe), egui::Sense::click_and_drag());
        let rect = response.rect;
        let zell_w = rect.width() / self.spalten.max(1) as f32;
        let zell_h = rect.height() / reihen.max(1) as f32;
        let schrift = (zell_h * 0.45).clamp(9.0, 14.0);
        let aktiv = if response.is_pointer_button_down_on() {
            response
                .interact_pointer_pos()
                .and_then(|pos| self.taste_unter(rect, pos))
        } else {
            None
        };
        for i in 0..self.tasten_anzahl() {
            let spalte = (i % self.spalten) as f32;
            let reihe = (i / self.spalten) as f32;
            let zelle = egui::Rect::from_min_size(
                egui::pos2(
                    rect.left() + spalte * zell_w + 1.0,
                    rect.top() + reihe * zell_h + 1.0,
                ),
                egui::vec2(zell_w - 2.0, zell_h - 2.0),
            );
            let wert = self.wert_der_taste(i);
            // Getouchte Tasten im Regenbogen nach ihrer Reihenfolge.
            let spur_index = self.spur.iter().position(|(t, _)| *t == i);
            let farbe = if let Some(k) = spur_index {
                let anteil = if self.spur.len() <= 1 {
                    0.0
                } else {
                    k as f32 / (self.spur.len() - 1) as f32
                };
                regenbogen(anteil)
            } else if aktiv == Some(i) {
                Color32::from_rgb(0, 200, 255)
            } else if wert == 0 {
                Color32::from_rgb(45, 48, 58)
            } else {
                Color32::from_rgb(60, 70, 90)
            };
            painter.rect_filled(zelle, 3.0, farbe);
            painter.text(
                zelle.center(),
                egui::Align2::CENTER_CENTER,
                format!("{wert}"),
                egui::FontId::proportional(schrift),
                Color32::from_rgb(230, 230, 235),
            );
        }

        // Wischweg als Linien zwischen den getouchten Tasten.
        for (k, fenster) in self.spur.windows(2).enumerate() {
            let anteil = k as f32 / (self.spur.len().max(2) - 1) as f32;
            painter.line_segment(
                [fenster[0].1, fenster[1].1],
                egui::Stroke::new(3.0, regenbogen(anteil)),
            );
        }

        // Alles laeuft ueber die Touch-Ereignisse: so zaehlt ein Finger genau einmal.
        // Aufsetzen = erster Schritt, Loslassen = der Schritt, bei dem der Wisch
        // endet. Damit ist „Taste halten und eine andere druecken" automatisch
        // „naechster Schritt".
        let rahmen = self.finger.schritt(ui);
        for (id, pos) in rahmen.starts {
            let taste = self.taste_unter(rect, pos);
            self.wisch.insert(id, (taste, taste));
            // Aufsetzen = erster Schritt, und genau dann faengt auch die
            // Tippfolge an (der C++-Recorder startet seinen Timer beim ersten
            // Tastendruck). Deshalb wird hier schon geschrieben und nicht erst
            // beim Loslassen — sonst kaeme der erste Schritt zu spaet.
            if let Some(taste) = taste {
                self.spur.clear();
                self.druck_schritt(taste, pos, pattern);
                self.presse.push(taste);
            }
        }
        for (id, pos) in rahmen.zuege {
            let treffer = self.taste_unter(rect, pos);
            if let Some(gespeichert) = self.wisch.get_mut(&id) {
                gespeichert.1 = treffer.or(gespeichert.1);
            }
        }
        for (id, _) in rahmen.endes {
            let Some((start, ende)) = self.wisch.remove(&id) else {
                continue;
            };
            let Some(start) = start else {
                continue;
            };
            // Der Wisch zaehlt als zweiter Schritt nur, wenn er wirklich
            // gewandert ist; ein Tippen auf dieselbe Taste bleibt ein Schritt.
            let gewandert = ende.filter(|ende| *ende != start);
            if let Some(ende) = gewandert {
                let pos = self.zentrum(rect, ende);
                self.druck_schritt(ende, pos, pattern);
                self.presse.push(ende);
            }
        }

        // Schrittanzeige: die gesetzten Schritte mit ihrem Wert.
        let schritte = pattern.steps_per_cycle().max(2);
        if !pattern.is_empty() {
            let (leiste, painter) =
                ui.allocate_painter(egui::vec2(breite, 44.0), egui::Sense::hover());
            let rect = leiste.rect;
            let mitte = rect.center().y + 6.0;
            painter.line_segment(
                [
                    egui::pos2(rect.left(), mitte),
                    egui::pos2(rect.right(), mitte),
                ],
                egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
            );
            for s in 0..schritte {
                let x = rect.left() + rect.width() * s as f32 / (schritte - 1) as f32;
                painter.line_segment(
                    [egui::pos2(x, mitte - 3.0), egui::pos2(x, mitte + 3.0)],
                    egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
                );
            }
            for note in pattern.notes() {
                let x = rect.left() + rect.width() * note.step as f32 / (schritte - 1) as f32;
                painter.circle_filled(egui::pos2(x, mitte), 6.0, Color32::from_rgb(90, 200, 255));
                painter.text(
                    egui::pos2(x, mitte - 16.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", note.pitch),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(225, 225, 230),
                );
            }
        }

        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        ui.label(format!(
            "Nächster Schritt: {}",
            self.naechster_schritt.wrapping_add(1)
        ));
        ui.label(
            RichText::new(
                "Jede Taste belegt den nächsten Schritt. Ein Wisch zählt nur die Taste, \
                 bei der er anfängt, und die, bei der er aufhört; eine gehaltene Taste \
                 plus eine zweite ergibt zwei Schritte hintereinander.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
    }
}
