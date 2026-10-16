//! Lissa-Tab: Lissajous-Figur mit Phasen-Drehung im Takt.
//!
//! Die Figur kippt wie im Max-Patch (`lissa-2020optic.amxd`): der Phasenversatz
//! einer Schwingung läuft mit dem Takt — als Sägezahn (Original, harter
//! Rücksprung = Doppel-Moment), weich, oder endlos in eine Richtung
//! (links/rechts). Dazu klingen die beiden Stimmen der Figur (X = Grundton·x,
//! Y = Grundton·y); die X-Stimme bekommt den Frequenzversatz der Drehung, wie
//! im Patch. Formeln im Kern: `jamcore::lissa`.

use eframe::egui;
use jamcore::lissa::{self, Modus};
use std::f64::consts::PI;
use std::time::Instant;

/// Takte pro Zyklus, wenn nichts anderes eingestellt ist (ein Schwung = 2 Takte).
const VORGABE_TAKTE: f64 = 2.0;
/// Grundton-Vorgabe: A-3 (220 Hz) — mit 2:3 klingen 440/660 Hz.
const VORGABE_GRUNDTON: i32 = 57;

pub struct LissaPanel {
    a: u32,
    b: u32,
    umdrehungen: f64,
    /// Endlos drehen (Reglerstufe „∞") statt Faltung/weicher Umkehr?
    endlos: bool,
    /// Richtung der endlosen Drehung: false = links, true = rechts.
    richtung_rechts: bool,
    /// Faltung oder weiche Umkehr (gilt, solange nicht endlos).
    modus: Modus,
    /// Takte pro Dreh-Zyklus (jamcore::takte-Bereich).
    takte: f64,
    /// Ton an/aus (die zwei Stimmen der Figur).
    ton_an: bool,
    /// Grundton als MIDI-Note.
    grundton: i32,
    praesentation: bool,
    /// Taktzähler wird fortgeschrieben (Tempoänderungen machen keinen Sprung).
    takte_seit_start: f64,
    letzte_zeit: Instant,
}

impl Default for LissaPanel {
    fn default() -> Self {
        Self {
            a: 2,
            b: 3,
            umdrehungen: 1.0,
            endlos: false,
            richtung_rechts: true,
            modus: Modus::Weich,
            takte: VORGABE_TAKTE,
            ton_an: false,
            grundton: VORGABE_GRUNDTON,
            praesentation: false,
            takte_seit_start: 0.0,
            letzte_zeit: Instant::now(),
        }
    }
}

impl LissaPanel {
    /// Wirksamer Dreh-Modus: endlos (Richtungsknöpfe) oder Faltung/weich.
    fn wirksamer_modus(&self) -> Modus {
        if self.endlos {
            if self.richtung_rechts {
                Modus::UnendlichRechts
            } else {
                Modus::UnendlichLinks
            }
        } else {
            self.modus
        }
    }

    /// Ton abschalten (beim Verlassen des Tabs).
    pub fn stoppen(&mut self) {
        if self.ton_an {
            self.ton_an = false;
        }
        crate::klang::dauerton_setzen(None);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, bpm: f32) {
        // Takte fortschreiben: das Tempo wirkt nur auf die Rate, nicht auf die
        // Vergangenheit (sonst springt die Figur bei jedem Tempo-Wechsel).
        let jetzt = Instant::now();
        let dt = jetzt.duration_since(self.letzte_zeit).as_secs_f64();
        self.letzte_zeit = jetzt;
        self.takte_seit_start += dt * bpm.max(1.0) as f64 / 60.0;

        let modus = self.wirksamer_modus();
        let phi = lissa::phase(self.takte_seit_start, self.takte, self.umdrehungen, modus);

        // Die zwei Stimmen der Figur (X bekommt den Versatz der Drehung).
        let grund_hz = jamcore::klang::frequenz(self.grundton);
        let mut stimmen = lissa::stimmen_frequenzen(grund_hz, self.a, self.b);
        let versatz_hz = (lissa::phasen_geschwindigkeit(
            self.takte_seit_start,
            self.takte,
            self.umdrehungen,
            modus,
            bpm as f64,
        ) / (2.0 * PI)) as f32;
        stimmen[0] += versatz_hz;
        if self.ton_an {
            crate::klang::dauerton_setzen(Some(stimmen));
        } else {
            crate::klang::dauerton_setzen(None);
        }

        if self.praesentation {
            let response = self.figur(ui, phi, true);
            if response.clicked() {
                self.praesentation = false;
            }
            ui.ctx().request_repaint();
            return;
        }

        ui.heading("Lissa — Figur dreht im Takt");

        // X/Y-Stepper: 1 … 16
        ui.horizontal_wrapped(|ui| {
            ui.label("X");
            if ui.small_button("−").clicked() {
                self.a = self.a.saturating_sub(1).max(1);
            }
            ui.label(egui::RichText::new(format!("{:2}", self.a)).monospace());
            if ui.small_button("+").clicked() {
                self.a = (self.a + 1).min(16);
            }
            ui.separator();
            ui.label("Y");
            if ui.small_button("−").clicked() {
                self.b = self.b.saturating_sub(1).max(1);
            }
            ui.label(egui::RichText::new(format!("{:2}", self.b)).monospace());
            if ui.small_button("+").clicked() {
                self.b = (self.b + 1).min(16);
            }
        });

        // Presets
        ui.horizontal_wrapped(|ui| {
            for (name, a, b) in [
                ("Kreis 2:2", 2, 2),
                ("Unendlich 2:3", 2, 3),
                ("1:2", 1, 2),
                ("3:2", 3, 2),
                ("3:4", 3, 4),
                ("4:5", 4, 5),
            ] {
                if ui.small_button(name).clicked() {
                    self.a = a;
                    self.b = b;
                }
            }
        });

        // Umdrehungen (1/64 … 16) + Stufe „∞", Fader wie im Bild-Tab
        ui.horizontal_wrapped(|ui| {
            ui.label("Umdrehungen");
            if ui.small_button("÷2").clicked() {
                self.umdrehungen = lissa::teile_umdrehungen(self.umdrehungen, 2.0);
            }
            if ui.small_button("×2").clicked() {
                self.umdrehungen = lissa::vielfache_umdrehungen(self.umdrehungen, 2.0);
            }
            if ui.small_button("÷3").clicked() {
                self.umdrehungen = lissa::teile_umdrehungen(self.umdrehungen, 3.0);
            }
            if ui.small_button("×3").clicked() {
                self.umdrehungen = lissa::vielfache_umdrehungen(self.umdrehungen, 3.0);
            }
            if ui.selectable_label(self.endlos, "∞").clicked() {
                self.endlos = !self.endlos;
            }
            // Nur die Rate nennen — der ∞-Knopf zeigt den Modus, die Richtung
            // steht in der Zeile darunter (sonst bricht die Zeile um).
            ui.label(lissa::text_umdrehungen(self.umdrehungen));
        });

        // Takte pro Zyklus (1/36 … 36)
        ui.horizontal_wrapped(|ui| {
            ui.label("Takte pro Zyklus");
            if ui.small_button("÷2").clicked() {
                self.takte = jamcore::takte::teile(self.takte, 2.0);
            }
            if ui.small_button("×2").clicked() {
                self.takte = jamcore::takte::vielfache(self.takte, 2.0);
            }
            if ui.small_button("÷3").clicked() {
                self.takte = jamcore::takte::teile(self.takte, 3.0);
            }
            if ui.small_button("×3").clicked() {
                self.takte = jamcore::takte::vielfache(self.takte, 3.0);
            }
            ui.label(jamcore::takte::text(self.takte));
        });

        // Charakter + Präsentation
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(
                    !self.endlos && self.modus == Modus::Saege,
                    "Sägezahn (Original)",
                )
                .clicked()
            {
                self.endlos = false;
                self.modus = Modus::Saege;
            }
            if ui
                .selectable_label(!self.endlos && self.modus == Modus::Weich, "Weich")
                .clicked()
            {
                self.endlos = false;
                self.modus = Modus::Weich;
            }
            // Richtung der endlosen Drehung (nur wenn „∞" gewählt ist).
            if self.endlos {
                if ui.selectable_label(!self.richtung_rechts, "◀ links").clicked() {
                    self.richtung_rechts = false;
                }
                if ui.selectable_label(self.richtung_rechts, "rechts ▶").clicked() {
                    self.richtung_rechts = true;
                }
            }
            if ui.button("Präsentation").clicked() {
                self.praesentation = true;
            }
        });

        // Ton: die zwei Stimmen der Figur
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.ton_an, "Ton");
            ui.add(
                egui::Slider::new(&mut self.grundton, 24..=96)
                    .text("Grundton")
                    .custom_formatter(|v, _| {
                        format!(
                            "{} ({:.0} Hz)",
                            jamcore::grid::note_name(v as i32),
                            jamcore::klang::frequenz(v as i32)
                        )
                    }),
            );
            ui.label(
                egui::RichText::new(format!(
                    "X {:.0} Hz · Y {:.0} Hz",
                    stimmen[0], stimmen[1]
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
        });

        let drehung = if self.endlos {
            if self.richtung_rechts {
                "∞ rechts"
            } else {
                "∞ links"
            }
        } else {
            match self.modus {
                Modus::Saege => "Sägezahn",
                Modus::Weich => "weich",
                _ => "endlos",
            }
        };
        ui.label(
            egui::RichText::new(format!(
                "{}:{}-Figur · {} · Zyklus {} · {:.0} BPM · φ = {:+.0}°",
                self.a,
                self.b,
                drehung,
                jamcore::takte::text(self.takte),
                bpm,
                phi.to_degrees()
            ))
            .small()
            .color(egui::Color32::GRAY),
        );

        self.figur(ui, phi, false);
        ui.ctx().request_repaint();
    }

    /// Figur zeichnen; `voll` = Präsentation (füllt den ganzen Platz).
    fn figur(&self, ui: &mut egui::Ui, phi: f64, voll: bool) -> egui::Response {
        let breite = ui.available_width();
        let hoehe = if voll {
            ui.available_height()
        } else {
            breite
        };
        let (response, painter) =
            ui.allocate_painter(egui::vec2(breite, hoehe), egui::Sense::click());
        let rect = response.rect;
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(14, 14, 18));

        let mitte = rect.center();
        let radius = rect.width().min(rect.height()) * 0.44;
        let n = 512;
        let mut punkte: Vec<egui::Pos2> = lissa::punkte(self.a as f64, self.b as f64, phi, n)
            .into_iter()
            .map(|(x, y)| egui::pos2(mitte.x + x as f32 * radius, mitte.y - y as f32 * radius))
            .collect();
        // Kurve schließen (θ endet einen Schritt vor 2π).
        if let Some(&erster) = punkte.first() {
            punkte.push(erster);
        }
        painter.add(egui::Shape::line(
            punkte,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(232, 232, 242)),
        ));

        if voll {
            painter.text(
                rect.center_bottom() - egui::vec2(0.0, 28.0),
                egui::Align2::CENTER_BOTTOM,
                "Tippen zum Beenden",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(90),
            );
        }
        response
    }
}
