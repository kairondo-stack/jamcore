//! Maske 2: Tastenfeld in Nummernblock-Anordnung mit „Ende“.
//!
//! Unterschiede zur alten Maske (Nutzerentscheidungen 2026-10-14/22/23):
//! - **Zehn** Tasten in Nummernblock-Anordnung (7-8-9 / 4-5-6 / 1-2-3 / 0).
//! - Der USB-Nummernblock trifft die **raeumlich gleiche** Taste
//!   (`maske::position_numpad`).
//! - Die Wertigkeit einer Taste ist eine **Dauer in Schritten**, keine
//!   Zeitmessung: 8, 4, 6 hintereinander ergeben 8 + 4 + 6 = 18 gleiche
//!   Schritte (Block 0–7, 8–11, 12–17). Es wird nichts auf die Zahl der
//!   Drücke normalisiert.
//! - Zusätzlich werden die **realen Tippabstaende** gemessen und in Schritte
//!   gemappt; im Modus „beides“ stehen sie als Pausen vor dem jeweiligen
//!   Block. Ergebnis = Summe(Abstaende) + Summe(Dauern).
//! - **„Ende“** haelt fest. Ist in Maske 1 ein Bereich gewaehlt, wird die
//!   neue Gesamtzeit (Bloecke + Pausen) proportional auf die Bereichszeit
//!   gemappt und nur dieser Bereich ersetzt; sonst ist die Aufnahme der
//!   ganze Verlauf.
//! - Der Verlauf liegt im gemeinsamen Speicher von [`crate::maske`]; Maske 1
//!   zeigt ihn (gleiche Datenquelle, keine Kopie), Pausen als Luecken.

use std::time::Instant;

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

/// Was in den Verlauf geschrieben wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Takt {
    /// Nur die Werte der Tasten (Dauern), keine Pausen.
    Werte,
    /// Nur die gemessenen Tippabstaende als Blocklaengen (mind. 1 Schritt).
    Tippzeiten,
    /// Tippabstaende als Pausen + Wertigkeit als Blockdauer (Standard).
    Beides,
}

impl Takt {
    fn name(self) -> &'static str {
        match self {
            Takt::Werte => "Werte",
            Takt::Tippzeiten => "Tippzeiten",
            Takt::Beides => "beides",
        }
    }
}

pub struct Maske2Panel {
    /// Wert (Dauer in Schritten) je Position; die Positionen liegen in
    /// Nummernblock-Anordnung, die Vorgabewerte sind 7,8,9 / 4,5,6 / 1,2,3 / 0.
    werte: [i32; TASTEN],
    /// Die laufende Aufnahme als (Ton, Dauer) in Druckreihenfolge.
    dauern: Vec<(i32, u32)>,
    /// Gemessener Abstand zum vorigen Druck in Millisekunden (erster = 0).
    abstaende_ms: Vec<f64>,
    /// Derselbe Abstand, beim Druck mit dem damaligen Tempo in Schritte
    /// gemappt. So aendert eine spaetere Tempoaenderung die Vergangenheit
    /// nicht.
    abstand_schritte: Vec<u32>,
    /// Die aktiven Pausen vor jedem Block (je nach [`Takt`]).
    pausen: Vec<u32>,
    /// Aufgeklappter Verlauf: (Schrittnummer, Ton), Pausen als Luecken.
    verlauf: Vec<(u32, i32)>,
    /// Zeitpunkte für den Bild-Scan (ein Punkt je klingendem Schritt).
    folge: Vec<Aufzeichnung>,
    /// Was der Verlauf gerade zeigt.
    takt: Takt,
    /// Zeitpunkt des letzten Drucks fuer den naechsten Abstand.
    letzter_druck: Option<Instant>,
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
            abstaende_ms: Vec::new(),
            abstand_schritte: Vec::new(),
            pausen: Vec::new(),
            verlauf: Vec::new(),
            folge: Vec::new(),
            takt: Takt::Beides,
            letzter_druck: None,
            fest: false,
            status: "Tasten tippen: Dauer + Tippabstand werden aufgenommen.".to_string(),
            letzte_aenderung: None,
        }
    }

    /// Die aufgezeichnete Folge (Zeitpunkte) fuer den Bild-Scan.
    pub fn folge(&self) -> &[Aufzeichnung] {
        &self.folge
    }

    /// Zeitpunkte fuer den Bild-Scan: ein Punkt je klingendem Schritt, an
    /// seiner Stelle im Gesamtverlauf (Pausen bleiben Luecken).
    ///
    /// Waehrend der Aufnahme zaehlt der Entwurf; danach der festgehaltene
    /// Verlauf (der bei einer Bereichsaufnahme ein anderer ist als der
    /// Entwurf). Nach „neu“ bleibt die gruene Maske stehen, statt zu
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

    /// Dauer eines Rasterschritts in ms: `60_000 / bpm / (feinheit/4)`.
    /// Ueber die Kern-Uhr gerechnet (dieselbe Formel wie Transport/Bild).
    fn step_ms(&self) -> f64 {
        jamcore::transport::step_dauer(crate::transport::bpm(), crate::transport::feinheit())
            * 1000.0
    }

    /// Die Blockdauern, die der aktuelle Takt in den Verlauf schreibt.
    fn aktive_dauern(&self) -> Vec<(i32, u32)> {
        match self.takt {
            Takt::Werte | Takt::Beides => self.dauern.clone(),
            Takt::Tippzeiten => self
                .dauern
                .iter()
                .enumerate()
                .map(|(i, &(ton, _))| {
                    let schritte = self.abstand_schritte.get(i).copied().unwrap_or(0);
                    (ton, schritte.max(1))
                })
                .collect(),
        }
    }

    /// Die Pausen vor den Blocken, die der aktuelle Takt schreibt.
    fn aktive_pausen(&self) -> Vec<u32> {
        match self.takt {
            Takt::Beides => self.abstand_schritte.clone(),
            Takt::Werte | Takt::Tippzeiten => vec![0; self.dauern.len()],
        }
    }

    /// „neu“: die laufende Aufnahme verwerfen und wieder aufnehmen. Der
    /// festgehaltene Verlauf bleibt stehen — sonst waere beim Aufnehmen
    /// eines Bereichs der Inhalt ausserhalb sofort weg.
    pub fn neu(&mut self) {
        self.dauern.clear();
        self.abstaende_ms.clear();
        self.abstand_schritte.clear();
        self.pausen.clear();
        self.verlauf.clear();
        self.folge.clear();
        self.letzter_druck = None;
        self.fest = false;
        self.letzte_aenderung = Some(std::time::Instant::now());
        if let Ok(mut z) = maske::verlauf_zustand().lock() {
            z.entwurf = maske::Entwurf::default();
        }
        let bereich = maske::verlauf_snapshot().bereich;
        self.status = match bereich {
            Some((von, bis)) => format!(
                "neu — Bereich {von}-{bis} ({} Schritte), Takt {}",
                bis - von,
                self.takt.name()
            ),
            None => format!("neu — Takt {}: Tasten tippen", self.takt.name()),
        };
        info!("Maske 2: neu (Takt {})", self.takt.name());
    }

    /// „Ende“: festhalten und die Aufnahme stoppen, bis „neu“ kommt.
    ///
    /// Ohne Bereich in Maske 1 wird der ganze Verlauf ersetzt. Mit Bereich
    /// wird die neue Gesamtzeit (Bloecke + Pausen) auf die Bereichszeit
    /// gemappt; ausserhalb bleibt der alte Verlauf stehen.
    pub fn ende(&mut self) {
        if self.dauern.is_empty() {
            self.fest = true;
            self.status = "Ende — keine Schritte aufgenommen".to_string();
            info!("Maske 2: Ende ohne Schritte");
            return;
        }
        let dauern = self.aktive_dauern();
        let pausen = self.aktive_pausen();
        let block_summe: u32 = dauern.iter().map(|(_, d)| *d).sum();
        let pausen_summe: u32 = pausen.iter().sum();
        let gesamt = block_summe + pausen_summe;
        info!("maske2: Verlauf {gesamt} Schritte (Pausen {pausen_summe} + Bloecke {block_summe})");

        let mut z = match maske::verlauf_zustand().lock() {
            Ok(z) => z,
            Err(_) => return,
        };
        z.entwurf = maske::Entwurf::default();
        match z.bereich.take() {
            Some((von, bis)) => {
                let laenge = jamcore::maske::verlauf_laenge(&z.schritte);
                let von = von.min(laenge);
                let bis = bis.min(laenge);
                let ersetzt = if pausen_summe > 0 {
                    jamcore::maske::bereich_ersetzen_mit_pausen(
                        &mut z.schritte,
                        von,
                        bis,
                        &dauern,
                        &pausen,
                    )
                } else {
                    jamcore::maske::bereich_ersetzen(&mut z.schritte, von, bis, &dauern)
                };
                match ersetzt {
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
                info!("maske: Verlauf {gesamt} Schritte");
                self.status = format!("{gesamt} Schritte festgehalten — „neu“ nimmt wieder auf");
            }
        }
        self.fest = true;
        self.letzte_aenderung = Some(std::time::Instant::now());
        info!(
            "Maske 2: Ende ({} Tasten, {gesamt} Schritte)",
            self.dauern.len()
        );
    }

    /// Eine Taste des Feldes anschlagen: Wertigkeit = Dauer, dazu den
    /// Abstand zum vorigen Druck als Pause (je nach Takt-Modus).
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
        let jetzt = Instant::now();
        let abstand_ms = self
            .letzter_druck
            .map(|vorher| jetzt.duration_since(vorher).as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        self.letzter_druck = Some(jetzt);
        let schritte = jamcore::maske::schritte_aus_abstaenden(&[abstand_ms], self.step_ms())
            .first()
            .copied()
            .unwrap_or(0);
        self.abstaende_ms.push(abstand_ms);
        self.abstand_schritte.push(schritte);

        let ton = TON_BASIS + position as i32;
        self.dauern.push((ton, dauer));
        self.nachziehen();

        self.letzte_aenderung = Some(std::time::Instant::now());
        info!("maske2: Abstand {abstand_ms:.0} ms = {schritte} Schritte");
        info!("maske2: taste {} (dauer {dauer}, ton {ton})", position + 1);
        let pausen_summe: u32 = self.pausen.iter().sum();
        let block_summe: u32 = self.aktive_dauern().iter().map(|(_, d)| *d).sum();
        info!(
            "maske2: Verlauf {} Schritte (Pausen {pausen_summe} + Bloecke {block_summe})",
            block_summe + pausen_summe
        );
        self.status = format!(
            "{} Schritte · Takt {} — Ende haelt fest",
            block_summe + pausen_summe,
            self.takt.name()
        );
    }

    /// Verlauf und Bild-Zeitpunkte aus dem aktuellen Takt-Modus bauen und in
    /// die gemeinsame Quelle schreiben.
    fn nachziehen(&mut self) {
        let dauern = self.aktive_dauern();
        let pausen = self.aktive_pausen();
        self.pausen = pausen.clone();
        self.verlauf = jamcore::maske::verlauf_mit_pausen(&dauern, &pausen);
        let zeiten = maske::verlauf_zeiten(&self.verlauf);
        self.folge = self
            .verlauf
            .iter()
            .zip(zeiten)
            .map(|((_, ton), zeit)| Aufzeichnung {
                zeit,
                indexnum: *ton as f32,
                tempo: Vec::new(),
            })
            .collect();
        if let Ok(mut z) = maske::verlauf_zustand().lock() {
            z.entwurf = maske::Entwurf {
                dauern: dauern.clone(),
                pausen,
            };
            // Nach „Ende“ ist der Verlauf festgeschrieben; ein Moduswechsel
            // zeigt dann nur die Alternative im Entwurf, statt eine bereits
            // auf einen Bereich gemappte Aufnahme wieder zu ueberschreiben.
            // Auch ein leerer Entwurf (direkt nach „neu“) darf den alten
            // Verlauf nicht loeschen.
            if z.bereich.is_none() && !self.fest && !self.dauern.is_empty() {
                z.schritte = self.verlauf.clone();
            }
        }
    }

    /// Die Anzeige-Zeile „18 Schritte (8+4+6) · Pausen 2+3 (5)“.
    fn zusammenfassung(&self) -> String {
        let dauern = self.aktive_dauern();
        let pausen = self.aktive_pausen();
        let block_summe: u32 = dauern.iter().map(|(_, d)| *d).sum();
        let pausen_summe: u32 = pausen.iter().sum();
        let dauer_text = dauern
            .iter()
            .map(|(_, d)| d.to_string())
            .collect::<Vec<_>>()
            .join("+");
        match self.takt {
            Takt::Werte => format!("{block_summe} Schritte ({dauer_text}) · Takt: Werte"),
            Takt::Tippzeiten => {
                format!("{block_summe} Schritte ({dauer_text}) · Takt: Tippzeiten")
            }
            Takt::Beides => {
                let pausen_text = if pausen_summe == 0 {
                    "—".to_string()
                } else {
                    pausen
                        .iter()
                        .filter(|p| **p > 0)
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join("+")
                };
                format!(
                    "{block_summe} Schritte ({dauer_text}) · Pausen {pausen_text} \
                     ({pausen_summe}) · gesamt {}",
                    block_summe + pausen_summe
                )
            }
        }
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

    /// Der Verlauf als Blockleiste: ein Platz je Gesamtschritt, Pausen
    /// bleiben dunkle Luecken.
    fn verlauf_streifen(&self, ui: &mut egui::Ui, breite: f32) {
        let (leiste, painter) = ui.allocate_painter(egui::vec2(breite, 40.0), egui::Sense::hover());
        let rect = leiste.rect;
        painter.rect_filled(rect, 2.0, Color32::from_rgb(40, 42, 50));
        let n = jamcore::maske::verlauf_laenge(&self.verlauf);
        if n == 0 {
            return;
        }
        let block = rect.width() / n as f32;
        for (schritt, ton) in self.verlauf.iter() {
            let r = egui::Rect::from_min_size(
                egui::pos2(rect.left() + *schritt as f32 * block, rect.top()),
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
                     Gesamtzeit (Bloecke + Pausen) darauf.",
                    bis - von
                ))
                .small()
                .color(Color32::from_rgb(150, 200, 255)),
            );
        }

        // Takt-Modus: Werte / Tippzeiten / beides.
        ui.horizontal_wrapped(|ui| {
            ui.label("Takt:");
            let mut geaendert = false;
            for takt in [Takt::Werte, Takt::Tippzeiten, Takt::Beides] {
                geaendert |= ui
                    .selectable_value(&mut self.takt, takt, takt.name())
                    .changed();
            }
            if geaendert {
                self.nachziehen();
                self.status = format!(
                    "Takt {} — {} Schritte",
                    self.takt.name(),
                    jamcore::maske::verlauf_laenge(&self.verlauf)
                );
                info!("maske2: Takt {}", self.takt.name());
            }
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

        // Der Verlauf: Gesamtschritte inklusive Pausen-Luecken.
        let gesamt = jamcore::maske::verlauf_laenge(&self.verlauf);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Verlauf").strong());
            ui.label(format!(
                "{gesamt} Schritte aus {} Tasten",
                self.dauern.len()
            ));
        });
        self.verlauf_streifen(ui, breite);
        if !self.dauern.is_empty() {
            ui.label(
                RichText::new(self.zusammenfassung())
                    .small()
                    .color(Color32::from_rgb(200, 210, 225)),
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
                         Schritte. Die Tippabstaende werden mit dem Transport-\
                         Tempo in Schritte gemappt; „beides“ legt sie als \
                         Pausen vor die Bloecke.",
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
