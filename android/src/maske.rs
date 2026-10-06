//! Takt-Maske: Tastenfeld mit Wischgesten.
//!
//! Wie die Graphen-Maske des Desktop-Recorders (dort stückweise linear), hier
//! zusätzlich mit Beschleunigungsprofil: Tippen auf eine Taste schreibt ihren
//! Wert auf den ersten Schritt; Wischen über mehrere Tasten legt sie auf
//! Schritte, deren Abstände sich aus den Wischproben ergeben (linear,
//! beschleunigend oder bremsend – `jamcore::maske`).
//!
//! Zwei Feldarten:
//! - **Je Wert**: eine Taste je Wert, 0..=wert_max (z.B. 0..128 = 129 Tasten);
//!   die Spaltenzahl bestimmt die Anordnung.
//! - **Verteilt**: festes Feld (Spalten x Reihen), die Werte gleichmäßig von
//!   0 (Pause) bis wert_max verteilt.

use eframe::egui::{self, Color32, RichText};
use jamcore::grid::{Note, Pattern, STEPS_PER_CYCLE as STEP_COUNT};
use jamcore::maske::{profil_aus_geschwindigkeiten, schritt_positionen, Verteilung};
use log::info;

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
    /// Wischproben: (x, y, Zeit in Sekunden).
    proben: Vec<(f32, f32, f32)>,
    letzter_wisch: Option<(usize, Verteilung)>,
    /// Getouchte Tasten in Reihenfolge (Index und Mitte) fuer den Regenbogen.
    spur: Vec<(usize, egui::Pos2)>,
    /// Noten des letzten Wischs (Schritt, Wert) fuer die Schrittweiten-Leiste.
    wisch_noten: Vec<(u16, i32)>,
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
            status: "Taste tippen = Note; über Tasten wischen = Schritte setzen.".to_string(),
            proben: Vec::new(),
            letzter_wisch: None,
            spur: Vec::new(),
            wisch_noten: Vec::new(),
        }
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

    /// Wertet den Wisch aus und schreibt die Noten in das Pattern.
    fn wisch_auswerten(&mut self, rect: egui::Rect, pattern: &mut Pattern) {
        let proben = std::mem::take(&mut self.proben);
        let tasten: Vec<usize> = proben
            .iter()
            .filter_map(|(x, y, _)| self.taste_unter(rect, egui::pos2(*x, *y)))
            .collect();
        let mut reihenfolge: Vec<usize> = Vec::new();
        for taste in tasten {
            if reihenfolge.last() != Some(&taste) {
                reihenfolge.push(taste);
            }
        }

        if reihenfolge.is_empty() {
            self.status = "Nichts getroffen.".to_string();
            return;
        }
        // Touch-Verlauf fuer die Anzeige (Regenbogen + Linien).
        self.spur.clear();
        self.wisch_noten.clear();
        for taste in &reihenfolge {
            self.spur.push((*taste, self.zentrum(rect, *taste)));
        }
        if reihenfolge.len() == 1 {
            // Tippen: Wert auf Schritt 1 des Rasters.
            let wert = self.wert_der_taste(reihenfolge[0]);
            if wert > 0 {
                pattern.insert(Note {
                    step: 0,
                    instrument: 0,
                    pitch: wert,
                    glide: false,
                });
                self.wisch_noten.push((0, wert));
                self.status = format!(
                    "Taste {} -> Note {} auf Schritt 1",
                    reihenfolge[0] + 1,
                    wert
                );
            } else {
                self.status = format!("Taste {} ist Pause (0).", reihenfolge[0] + 1);
            }
            return;
        }

        // Wisch: Geschwindigkeitsprofil aus den Proben.
        let geschwindigkeiten: Vec<f32> = proben
            .windows(2)
            .map(|w| {
                let dx = w[1].0 - w[0].0;
                let dy = w[1].1 - w[0].1;
                let dt = (w[1].2 - w[0].2).max(1e-3);
                (dx * dx + dy * dy).sqrt() / dt
            })
            .collect();
        let profil = profil_aus_geschwindigkeiten(&geschwindigkeiten);

        // Die durchstrichenen Tasten auf die 16 Schritte legen: Abstände nach
        // Profil, Werte von den Tasten. Pausen (0) lassen Lücken.
        let anzahl = reihenfolge.len();
        let positionen = schritt_positionen(anzahl as u32, profil);
        let mut punkte: Vec<f32> = Vec::with_capacity(anzahl + 2);
        punkte.push(0.0);
        punkte.extend(positionen);
        punkte.push(1.0);
        let mut belegt = 0i32;
        let mut zuletzt = -2i32;
        for (i, taste) in reihenfolge.iter().enumerate() {
            let anteil = punkte.get(i + 1).copied().unwrap_or(1.0);
            let mut schritt = (anteil * (STEP_COUNT - 1) as f32).round() as i32;
            if schritt <= zuletzt {
                schritt = zuletzt + 1;
            }
            zuletzt = schritt;
            let wert = self.wert_der_taste(*taste);
            if wert > 0 && (0..STEP_COUNT as i32).contains(&schritt) {
                pattern.insert(Note {
                    step: schritt as u16,
                    instrument: 0,
                    pitch: wert,
                    glide: false,
                });
                self.wisch_noten.push((schritt as u16, wert));
                belegt += 1;
            }
        }
        let name = match profil {
            Verteilung::Linear => "linear",
            Verteilung::Beschleunigend => "beschleunigend",
            Verteilung::Bremsend => "bremsend",
        };
        self.letzter_wisch = Some((anzahl, profil));
        self.status = format!(
            "Wisch: {} Tasten, {} ({belegt} Noten gesetzt)",
            anzahl, name
        );
        info!(
            "maske: Wisch {} Tasten, Profil {name}, {belegt} Noten",
            anzahl
        );
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

        // Proben sammeln, während der Finger auf dem Feld ist.
        if response.is_pointer_button_down_on() {
            if let Some(pos) = response.interact_pointer_pos() {
                let jetzt = ui.input(|i| i.time) as f32;
                let neu = match self.proben.last() {
                    Some((x, y, _)) => (pos.x - x).abs() + (pos.y - y).abs() > 2.0,
                    None => true,
                };
                if neu {
                    self.proben.push((pos.x, pos.y, jetzt));
                }
            }
        }
        if response.drag_stopped() {
            self.wisch_auswerten(rect, pattern);
        } else if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let jetzt = ui.input(|i| i.time) as f32;
                // Beim reinen Tippen dieselbe Taste als einzelne Probe.
                self.proben.push((pos.x, pos.y, jetzt));
                self.wisch_auswerten(rect, pattern);
            }
        }

        // Schrittweiten-Leiste: die Wisch-Noten auf den 16 Schritten, mit den
        // Abständen dazwischen (Werte über den Punkten, +Schritte darunter).
        if !self.wisch_noten.is_empty() {
            let (leiste, painter) =
                ui.allocate_painter(egui::vec2(breite, 52.0), egui::Sense::hover());
            let rect = leiste.rect;
            let mitte = rect.center().y;
            painter.line_segment(
                [
                    egui::pos2(rect.left(), mitte),
                    egui::pos2(rect.right(), mitte),
                ],
                egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
            );
            for s in 0..STEP_COUNT {
                let x = rect.left() + rect.width() * s as f32 / (STEP_COUNT - 1) as f32;
                painter.line_segment(
                    [egui::pos2(x, mitte - 3.0), egui::pos2(x, mitte + 3.0)],
                    egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
                );
            }
            let n = self.wisch_noten.len();
            for (k, (step, wert)) in self.wisch_noten.iter().enumerate() {
                let x = rect.left() + rect.width() * *step as f32 / (STEP_COUNT - 1) as f32;
                let anteil = if n <= 1 {
                    0.0
                } else {
                    k as f32 / (n - 1) as f32
                };
                let farbe = regenbogen(anteil);
                painter.circle_filled(egui::pos2(x, mitte), 6.0, farbe);
                painter.text(
                    egui::pos2(x, mitte - 18.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{wert}"),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(225, 225, 230),
                );
                if k > 0 {
                    let vor = self.wisch_noten[k - 1].0;
                    let x_vor = rect.left() + rect.width() * vor as f32 / (STEP_COUNT - 1) as f32;
                    let diff = *step as i32 - vor as i32;
                    painter.text(
                        egui::pos2((x + x_vor) / 2.0, mitte + 18.0),
                        egui::Align2::CENTER_CENTER,
                        format!("+{diff}"),
                        egui::FontId::proportional(10.0),
                        Color32::from_rgb(150, 150, 158),
                    );
                }
            }
        }

        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        if let Some((anzahl, profil)) = self.letzter_wisch {
            let name = match profil {
                Verteilung::Linear => "linear",
                Verteilung::Beschleunigend => "beschleunigend",
                Verteilung::Bremsend => "bremsend",
            };
            ui.label(format!("Letzter Wisch: {anzahl} Tasten, {name}"));
        }
        ui.label(
            RichText::new(
                "Tippen auf eine Taste schreibt ihren Wert; Wischen über mehrere Tasten \
                 legt sie mit den Schrittabständen des Wischprofils ins Raster.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
    }
}
