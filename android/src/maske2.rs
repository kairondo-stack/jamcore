//! Maske 2: Tastenfeld in Nummernblock-Anordnung mit „Ende“.
//!
//! Unterschiede zur alten Maske (Nutzerentscheidung 2026-10-14):
//! - **Zehn** Tasten in Nummernblock-Anordnung (7-8-9 / 4-5-6 / 1-2-3 / 0).
//! - Der USB-Nummernblock trifft die **raeumlich gleiche** Taste
//!   (`maske::position_numpad`).
//! - **„Ende“** (Display-Taste oder Numpad-Enter) haelt fest und **stoppt die
//!   Aufnahme bis „neu“** — die 8-Sekunden-Pause der alten Maske gibt es hier
//!   nicht.
//! - Die Werte der zehn Tasten sind einstellbar (Vorgabe = ihre Ziffer).

use eframe::egui::{self, Color32, RichText};
use jamcore::grid::Pattern;
use log::info;

use crate::maske::Aufzeichnung;
use crate::tasten::{tasten, Taste};

/// Anzahl der Tasten (Nummernblock: 1-9 und 0).
pub const TASTEN: usize = 10;

/// Zellenzahl je Reihe im Feld (3x3 plus die Null darunter).
const SPALTEN: usize = 3;

pub struct Maske2Panel {
    /// Wert (Indexnum) je Position; die Positionen liegen in Nummernblock-
    /// Anordnung, die Vorgabewerte sind 7,8,9 / 4,5,6 / 1,2,3 / 0.
    werte: [i32; TASTEN],
    folge: Vec<Aufzeichnung>,
    zwischenschritte: Vec<f32>,
    anker: Option<std::time::Instant>,
    /// „Ende“ gedrueckt: die Aufnahme steht, bis „neu“ kommt.
    fest: bool,
    status: String,
    /// Wann zuletzt etwas aufgenommen wurde — die zuletzt benutzte Maske
    /// speist den gruenen Balken im Bild-Tab.
    letzte_aenderung: Option<std::time::Instant>,
}

impl Default for Maske2Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Maske2Panel {
    pub fn new() -> Self {
        Self {
            werte: [7, 8, 9, 4, 5, 6, 1, 2, 3, 0],
            folge: Vec::new(),
            zwischenschritte: Vec::new(),
            anker: None,
            fest: false,
            status: "Feld tippen oder Nummernblock: nimmt auf; Ende haelt fest.".to_string(),
            letzte_aenderung: None,
        }
    }

    /// Die aufgezeichnete Folge (Zeitabstaende) fuer den Bild-Scan.
    pub fn folge(&self) -> &[Aufzeichnung] {
        &self.folge
    }

    pub fn zeiten(&self) -> Vec<f32> {
        let tipps: Vec<jamcore::schrittweite::Tipp> =
            self.folge.iter().map(Aufzeichnung::als_tipp).collect();
        jamcore::schrittweite::timestamps(&tipps)
    }

    pub fn letzte_aenderung(&self) -> Option<std::time::Instant> {
        self.letzte_aenderung
    }

    /// „neu“: alles verwerfen, wieder aufnehmen.
    pub fn neu(&mut self) {
        self.folge.clear();
        self.zwischenschritte.clear();
        self.anker = None;
        self.fest = false;
        self.letzte_aenderung = Some(std::time::Instant::now());
        self.status = "neu — Feld tippen nimmt auf".to_string();
        info!("Maske 2: neu");
    }

    /// „Ende“: festhalten und die Aufnahme stoppen, bis „neu“ kommt.
    pub fn ende(&mut self) {
        self.fest = true;
        self.status = format!(
            "Ende — {} Toene festgehalten; „neu“ nimmt wieder auf",
            self.folge.len()
        );
        info!("Maske 2: Ende ({} Toene)", self.folge.len());
    }

    /// Eine Taste des Feldes anschlagen (Touch oder Nummernblock).
    fn anschlagen(&mut self, position: usize) {
        if self.fest {
            return;
        }
        if position >= TASTEN {
            return;
        }
        if self.anker.is_none() {
            self.anker = Some(std::time::Instant::now());
        }
        let zeit = self.anker.map(|a| a.elapsed().as_secs_f32()).unwrap_or(0.0);
        let indexnum = self.werte[position];
        self.folge.push(Aufzeichnung {
            zeit,
            indexnum: indexnum as f32,
            tempo: Vec::new(),
        });
        self.zwischenschritte = self.zeiten();
        self.letzte_aenderung = Some(std::time::Instant::now());
        self.status = format!(
            "{} Toene, {} Zeitpunkte, {zeit:.1} s",
            self.folge.len(),
            self.zwischenschritte.len()
        );
        info!(
            "maske2: taste {} (indexnum {indexnum}) bei {zeit:.2}s",
            position + 1
        );
    }

    /// Zelle -> Position im Feld: 3x3 fuer 1..9, die Null allein darunter.
    fn position_der_zelle(reihe: usize, spalte: usize) -> Option<usize> {
        if reihe < 3 {
            Some(reihe * SPALTEN + spalte)
        } else if spalte == 1 {
            Some(9)
        } else {
            None
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _pattern: &mut Pattern) {
        ui.heading("Takt-Maske 2");
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("neu").min_size(egui::vec2(96.0, 44.0)))
                .clicked()
            {
                self.neu();
            }
            if ui
                .add(egui::Button::new("Ende").min_size(egui::vec2(96.0, 44.0)))
                .clicked()
            {
                self.ende();
            }
            ui.label(RichText::new(&self.status).color(if self.fest {
                Color32::GRAY
            } else {
                Color32::from_rgb(120, 230, 160)
            }));
        });

        // Nummernblock: Ziffer trifft den raeumlich gleichen Knopf,
        // Enter ist „Ende“.
        for taste in tasten(ui) {
            match taste {
                Taste::Ziffer(ziffer) => {
                    if let Some(position) = jamcore::maske::position_numpad(ziffer) {
                        self.anschlagen(position);
                    }
                }
                Taste::Enter => self.ende(),
            }
        }

        // Das Feld: grosse Knoepfe, damit sie mit dem Finger sicher treffen.
        let breite = ui.available_width();
        let seite = ((breite - 2.0 * 8.0) / SPALTEN as f32).min(110.0);
        egui::Grid::new("maske2_feld")
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for reihe in 0..4 {
                    for spalte in 0..SPALTEN {
                        match Self::position_der_zelle(reihe, spalte) {
                            Some(position) => {
                                let knopf = ui.add_sized(
                                    egui::vec2(seite, seite),
                                    egui::Button::new(
                                        RichText::new(format!("{}", self.werte[position]))
                                            .size(24.0),
                                    ),
                                );
                                if knopf.clicked() {
                                    self.anschlagen(position);
                                }
                            }
                            None => {
                                ui.allocate_space(egui::vec2(seite, seite));
                            }
                        }
                    }
                    ui.end_row();
                }
            });

        // Werte der Tasten: Vorgabe sind die Ziffern, aenderbar.
        egui::CollapsingHeader::new("Werte der Tasten")
            .id_salt("maske2_werte")
            .show(ui, |ui| {
                egui::Grid::new("maske2_werte_gitter")
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        for reihe in 0..4 {
                            for spalte in 0..SPALTEN {
                                match Self::position_der_zelle(reihe, spalte) {
                                    Some(position) => {
                                        let mut wert = self.werte[position];
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut wert)
                                                    .range(0..=127)
                                                    .speed(1.0),
                                            )
                                            .changed()
                                        {
                                            self.werte[position] = wert;
                                        }
                                    }
                                    None => {
                                        ui.label("");
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.label(
                    RichText::new(
                        "Der Feldwert ist das Indexnum (Unterteilungen je Schritt): \
                         die Schrittbreite ist 1/Wert. Die Null ist die Pause und \
                         teilt nicht.",
                    )
                    .small()
                    .color(Color32::DARK_GRAY),
                );
            });

        ui.label(format!(
            "Toene: {} — Zeitpunkte mit Zwischenschritten: {}",
            self.folge.len(),
            self.zwischenschritte.len()
        ));
        ui.label(
            RichText::new(
                "Nummernblock: Ziffern treffen die Tasten wie auf dem Block \
                 (7-8-9 / 4-5-6 / 1-2-3 / 0), Enter ist „Ende“. Ohne Ende laeuft \
                 die Aufnahme weiter — es gibt keine Pause, die neu beginnt.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
    }
}
