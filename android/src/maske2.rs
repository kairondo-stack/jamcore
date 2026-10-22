//! Maske 2: Tastenfeld in Nummernblock-Anordnung mit „Ende“.
//!
//! Unterschiede zur alten Maske (Nutzerentscheidungen 2026-10-14/22):
//! - **Zehn** Tasten in Nummernblock-Anordnung (7-8-9 / 4-5-6 / 1-2-3 / 0).
//! - Der USB-Nummernblock trifft die **raeumlich gleiche** Taste
//!   (`maske::position_numpad`).
//! - Die Wertigkeit einer Taste ist eine **Dauer in Schritten**, keine
//!   Zeitmessung: 8, 4, 6 hintereinander ergeben 8 + 4 + 6 = 18 gleiche
//!   Schritte (Block 0–7, 8–11, 12–17). Es wird nichts auf die Zahl der
//!   Drücke normalisiert — das war der Nutzerbefund vom 2026-10-22.
//! - **„Ende“** haelt fest. Ist in Maske 1 ein Bereich gewaehlt, wird die
//!   neue Gesamtzeit proportional auf die Bereichszeit gemappt und nur
//!   dieser Bereich ersetzt; sonst ist die Aufnahme der ganze Verlauf.
//! - Der Verlauf liegt im gemeinsamen Speicher von [`crate::maske`]; Maske 1
//!   zeigt ihn (gleiche Datenquelle, keine Kopie).

use eframe::egui::{self, Color32, RichText};
use jamcore::grid::Pattern;
use log::info;

use crate::maske::{self, Aufzeichnung};
use crate::tasten::{tasten, Taste};

/// Anzahl der Tasten (Nummernblock: 1-9 und 0).
pub const TASTEN: usize = 10;

/// Zellenzahl je Reihe im Feld (3x3 plus die Null darunter).
const SPALTEN: usize = 3;

/// Die Tonhöhe einer Taste kommt aus ihrer **Position**, nicht aus dem
/// Wert: der Wert ist die Dauer. So bleibt „8“ eine Dauer von 8 Schritten,
/// auch wenn der Ton 61 heißt.
const TON_BASIS: i32 = 60;

pub struct Maske2Panel {
    /// Wert (Dauer in Schritten) je Position; die Positionen liegen in
    /// Nummernblock-Anordnung, die Vorgabewerte sind 7,8,9 / 4,5,6 / 1,2,3 / 0.
    werte: [i32; TASTEN],
    /// Die laufende Aufnahme als (Ton, Dauer) in Druckreihenfolge.
    dauern: Vec<(i32, u32)>,
    /// Dieselbe Aufnahme aufgeklappt: (Schrittnummer, Ton), Summe der Dauern.
    verlauf: Vec<(u32, i32)>,
    /// Zeitpunkte für den Bild-Scan (ein Punkt je Schritt). Die Struktur
    /// bleibt, weil `lib.rs` sie über `folge()`/`zeiten()` abholt.
    folge: Vec<Aufzeichnung>,
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
            dauern: Vec::new(),
            verlauf: Vec::new(),
            folge: Vec::new(),
            fest: false,
            status: "Tasten tippen: die Wertigkeit ist die Dauer in Schritten.".to_string(),
            letzte_aenderung: None,
        }
    }

    /// Die aufgezeichnete Folge (Zeitpunkte) fuer den Bild-Scan.
    pub fn folge(&self) -> &[Aufzeichnung] {
        &self.folge
    }

    /// Zeitpunkte fuer den Bild-Scan: ein Punkt je Schritt, gleichmäßig.
    /// Waehrend der Aufnahme zaehlt der Entwurf; danach der festgehaltene
    /// Verlauf (der bei einer Bereichsaufnahme ein anderer ist als der
    /// Entwurf). Nach „neu“ bleibt die gruene Maske so stehen, statt zu
    /// verschwinden.
    pub fn zeiten(&self) -> Vec<f32> {
        if !self.fest && !self.verlauf.is_empty() {
            maske::verlauf_zeiten(&self.verlauf)
        } else {
            maske::verlauf_zeiten(&maske::verlauf_snapshot().schritte)
        }
    }

    pub fn letzte_aenderung(&self) -> Option<std::time::Instant> {
        self.letzte_aenderung
    }

    /// „neu“: die laufende Aufnahme verwerfen und wieder aufnehmen. Der
    /// festgehaltene Verlauf bleibt stehen — sonst waere beim Aufnehmen
    /// eines Bereichs der Inhalt ausserhalb sofort weg.
    pub fn neu(&mut self) {
        self.dauern.clear();
        self.verlauf.clear();
        self.folge.clear();
        self.fest = false;
        self.letzte_aenderung = Some(std::time::Instant::now());
        if let Ok(mut z) = maske::verlauf_zustand().lock() {
            z.entwurf.clear();
        }
        let bereich = maske::verlauf_snapshot().bereich;
        self.status = match bereich {
            Some((von, bis)) => format!(
                "neu — Bereich {von}-{bis} ({} Schritte): Tasten tippen",
                bis - von
            ),
            None => "neu — Tasten tippen; die Dauern haengen sich aneinander".to_string(),
        };
        info!("Maske 2: neu");
    }

    /// „Ende“: festhalten und die Aufnahme stoppen, bis „neu“ kommt.
    ///
    /// Ohne Bereich in Maske 1 wird der ganze Verlauf ersetzt. Mit Bereich
    /// wird die neue Gesamtzeit auf die Bereichszeit gemappt; ausserhalb
    /// bleibt der alte Verlauf stehen.
    pub fn ende(&mut self) {
        if self.dauern.is_empty() {
            self.fest = true;
            self.status = "Ende — keine Schritte aufgenommen".to_string();
            info!("Maske 2: Ende ohne Schritte");
            return;
        }
        let mut z = match maske::verlauf_zustand().lock() {
            Ok(z) => z,
            Err(_) => return,
        };
        z.entwurf.clear();
        match z.bereich.take() {
            Some((von, bis)) => {
                let laenge = z.schritte.len() as u32;
                let von = von.min(laenge);
                let bis = bis.min(laenge);
                match jamcore::maske::bereich_ersetzen(&mut z.schritte, von, bis, &self.dauern) {
                    Some(meldung) => {
                        info!(
                            "maske: Bereich {von}-{bis} neu aufgenommen ({} Schritte aus {})",
                            meldung.schritte, meldung.dauer
                        );
                        info!("maske: Verlauf {} Schritte", meldung.gesamt);
                        self.status = format!(
                            "Bereich {von}-{bis}: {} Schritte aus {} — Verlauf {} Schritte",
                            meldung.schritte, meldung.dauer, meldung.gesamt
                        );
                    }
                    None => {
                        // Leerer/ungueltiger Bereich: nichts loeschen.
                        info!("maske: Bereich {von}-{bis} ohne neue Schritte — unveraendert");
                        self.status =
                            format!("Bereich {von}-{bis}: keine Schritte — Verlauf unveraendert");
                    }
                }
            }
            None => {
                z.schritte = self.verlauf.clone();
                info!("maske: Verlauf {} Schritte", z.schritte.len());
                self.status = format!(
                    "{} Schritte festgehalten — „neu“ nimmt wieder auf",
                    z.schritte.len()
                );
            }
        }
        self.fest = true;
        self.letzte_aenderung = Some(std::time::Instant::now());
        info!(
            "Maske 2: Ende ({} Tasten, {} Schritte)",
            self.dauern.len(),
            self.verlauf.len()
        );
    }

    /// Eine Taste des Feldes anschlagen: die Wertigkeit ist die Dauer.
    fn anschlagen(&mut self, position: usize) {
        if self.fest || position >= TASTEN {
            return;
        }
        let wert = self.werte[position].max(0);
        let dauer = wert as u32;
        if dauer == 0 {
            // Die Null ist die Pause: sie belegt keinen Schritt und darf die
            // laufende Aufnahme nicht verlaengern.
            self.status = "Wert 0 = Pause: kein Schritt aufgenommen".to_string();
            info!("maske2: taste {} ist 0 — Pause, kein Schritt", position + 1);
            return;
        }
        let ton = TON_BASIS + position as i32;
        self.dauern.push((ton, dauer));
        self.nachziehen();

        // Sofort in die gemeinsame Quelle: Maske 1 zeigt den Verlauf live.
        // Ist ein Bereich gewaehlt, bleibt der festgehaltene Verlauf stehen;
        // die Vorschau dort kommt aus `entwurf` (proportional gemappt).
        if let Ok(mut z) = maske::verlauf_zustand().lock() {
            z.entwurf = self.dauern.clone();
            if z.bereich.is_none() {
                z.schritte = self.verlauf.clone();
            }
        }
        self.letzte_aenderung = Some(std::time::Instant::now());
        let dauern_text = self
            .dauern
            .iter()
            .map(|(_, d)| d.to_string())
            .collect::<Vec<_>>()
            .join("+");
        let gesamt = self.verlauf.len();
        self.status = match maske::verlauf_snapshot().bereich {
            Some((von, bis)) => {
                format!("{gesamt} Schritte ({dauern_text}) — Ende mappt auf Bereich {von}-{bis}")
            }
            None => format!("{gesamt} Schritte ({dauern_text}) — Ende haelt fest"),
        };
        info!(
            "maske2: taste {} (dauer {dauer}, ton {ton}) -> {gesamt} Schritte ({dauern_text})",
            position + 1
        );
    }

    /// Die aufgeklappte Schrittliste und die Zeitpunkte neu bauen.
    fn nachziehen(&mut self) {
        self.verlauf = jamcore::maske::schritte_aus_dauern(&self.dauern);
        let n = self.verlauf.len();
        let letzte = (n.saturating_sub(1)).max(1) as f32;
        self.folge = self
            .verlauf
            .iter()
            .enumerate()
            .map(|(i, (_, ton))| Aufzeichnung {
                zeit: i as f32 / letzte,
                indexnum: *ton as f32,
                tempo: Vec::new(),
            })
            .collect();
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

    /// Der Verlauf als Blockleiste: ein Block je Schritt, Farbe nach Ton.
    fn verlauf_streifen(&self, ui: &mut egui::Ui, breite: f32) {
        let (leiste, painter) = ui.allocate_painter(egui::vec2(breite, 40.0), egui::Sense::hover());
        let rect = leiste.rect;
        painter.rect_filled(rect, 2.0, Color32::from_rgb(40, 42, 50));
        let n = self.verlauf.len();
        if n == 0 {
            return;
        }
        let block = rect.width() / n as f32;
        for (i, (_, ton)) in self.verlauf.iter().enumerate() {
            let r = egui::Rect::from_min_size(
                egui::pos2(rect.left() + i as f32 * block, rect.top()),
                egui::vec2((block - 0.5).max(1.0), rect.height()),
            );
            painter.rect_filled(r, 0.0, maske::verlauf_farbe(*ton));
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _pattern: &mut Pattern) {
        ui.heading("Takt-Maske 2");
        ui.horizontal_wrapped(|ui| {
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

        // Der in Maske 1 gewaehlte Bereich gilt fuer die naechste Aufnahme.
        if let Some((von, bis)) = maske::verlauf_snapshot().bereich {
            ui.label(
                RichText::new(format!(
                    "Bereich {von}-{bis} ({} Schritte) aus Maske 1 — „Ende“ mappt die neue \
                     Gesamtzeit darauf.",
                    bis - von
                ))
                .small()
                .color(Color32::from_rgb(150, 200, 255)),
            );
        }

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

        // Der Verlauf: die Summe der Dauern ist direkt als Bloecke sichtbar.
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Verlauf").strong());
            ui.label(format!(
                "{} Schritte aus {} Tasten",
                self.verlauf.len(),
                self.dauern.len()
            ));
        });
        self.verlauf_streifen(ui, breite);
        if !self.dauern.is_empty() {
            let dauern_text = self
                .dauern
                .iter()
                .map(|(_, d)| d.to_string())
                .collect::<Vec<_>>()
                .join(" + ");
            ui.label(
                RichText::new(format!(
                    "{dauern_text} = {} Schritte (keine Normalisierung auf die Zahl der Drücke)",
                    self.verlauf.len()
                ))
                .small()
                .color(Color32::GRAY),
            );
        }

        // Werte der Tasten: Vorgabe sind die Ziffern, aenderbar.
        egui::CollapsingHeader::new("Dauern der Tasten")
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
                        "Der Feldwert ist die Dauer in Schritten (0 = Pause). \
                         Aufeinanderfolgende Tasten haengen an: 8, 4, 6 = 18 \
                         Schritte. Der Ton kommt aus der Position der Taste \
                         (60 + Position), damit die Bloecke im Verlauf getrennt \
                         sichtbar bleiben.",
                    )
                    .small()
                    .color(Color32::DARK_GRAY),
                );
            });

        ui.label(
            RichText::new(
                "Nummernblock: Ziffern treffen die Tasten wie auf dem Block \
                 (7-8-9 / 4-5-6 / 1-2-3 / 0), Enter ist „Ende“. In Maske 1 \
                 laesst sich ein Bereich waehlen; die naechste Aufnahme wird \
                 dann auf dessen Zeit gemappt.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
    }
}
