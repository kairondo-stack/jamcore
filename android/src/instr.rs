//! Instrument-/Sample-Tab: die Eintraege des geladenen Moduls mit
//! Notenbereichen.
//!
//! Jeder Eintrag kann einen Bereich `von..=bis` (MIDI-Noten, Anzeige als
//! Notenname) bekommen. Klingen mehrere Bereiche fuer eine Note, spielen
//! **alle** zusammen (Nutzerentscheidung 2026-10-14); ausserhalb aller
//! Bereiche bleibt es still. Solange **kein** Bereich gesetzt ist, nimmt der
//! Klangweg seinen Standard (wie bisher) — sonst waere der Modul-Klang nach
//! dem ersten Blick in diesen Tab verstummt.

use eframe::egui;
use jamcore::grid::note_name;
use jamcore::instrument::{treffer, Eintrag};
use log::info;
use std::sync::{Mutex, OnceLock};

use crate::player::ModulQuelle;

/// Die aktuelle Zuordnung: der Tab schreibt sie, der Klangweg liest sie.
static MAPPING: OnceLock<Mutex<Vec<Eintrag>>> = OnceLock::new();

fn slot() -> &'static Mutex<Vec<Eintrag>> {
    MAPPING.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn eintraege() -> Vec<Eintrag> {
    slot().lock().map(|e| e.clone()).unwrap_or_default()
}

pub fn set_eintraege(neu: Vec<Eintrag>) {
    if let Ok(mut liste) = slot().lock() {
        *liste = neu;
    }
}

/// Instrumente fuer eine Note (siehe `jamcore::instrument::treffer`).
pub fn instrumente_fuer(note: i32) -> Option<Vec<i32>> {
    treffer(&eintraege(), note.clamp(0, 119) as u8)
}

/// Eine Note ueber die Zuordnung spielen.
///
/// `Some(true)` = gespielt, `Some(false)` = still (Bereiche gesetzt, aber
/// kein Treffer), `None` = keine Zuordnung — der Aufrufer nimmt seinen
/// Standard (Instrument 30).
pub fn spielen(quelle: &ModulQuelle, note: i32, volume: f32) -> Option<bool> {
    match instrumente_fuer(note) {
        Some(liste) => {
            for index in &liste {
                let _ = quelle.note_spielen(*index, note, volume);
            }
            Some(!liste.is_empty())
        }
        None => None,
    }
}

/// Voreinstellung fuer einen noch leeren Bereich: C-4 bis C-5.
const VON_STANDARD: i32 = 60;
const BIS_STANDARD: i32 = 72;

pub struct InstrumentPanel {
    /// Instrumente oder Samples listen?
    pub samples: bool,
    status: String,
}

impl Default for InstrumentPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl InstrumentPanel {
    pub fn new() -> Self {
        Self {
            samples: false,
            status: "Bereich einstellen: von/bis als Notennamen; leere Plaetze sind moeglich."
                .to_string(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, quelle: Option<&ModulQuelle>) {
        ui.heading("Instrumente & Samples");
        let Some(quelle) = quelle else {
            ui.label("Kein Modul geladen — im Player-Tab ein Modul laden.");
            return;
        };
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.samples, "Instrumente").clicked() {
                self.samples = false;
            }
            if ui.selectable_label(self.samples, "Samples").clicked() {
                self.samples = true;
            }
            ui.label(
                egui::RichText::new(
                    "Mehrere Bereiche duerfen sich ueberlappen — dann klingen alle.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });
        ui.label(
            egui::RichText::new(
                "Achtung: Samples und Instrumente von Modulen sind urheberrechtlich geschützt \
                 — vor einer Wiederverwendung (Aufnahme, Export, Weitergabe) die Rechte klären.",
            )
            .small()
            .color(egui::Color32::DARK_GRAY),
        );

        // Liste des Moduls holen und die Zuordnung angleichen.
        let liste_quelle: Vec<(i32, String)> = if self.samples {
            quelle.sample_liste()
        } else {
            quelle.instrument_liste()
        };
        if liste_quelle.is_empty() {
            ui.label(if self.samples {
                "Das Modul hat keine Samples."
            } else {
                "Das Modul hat keine Instrumente (MOD/S3M haben nur Samples)."
            });
            return;
        }
        let mut liste = eintraege();
        let passt = liste.len() == liste_quelle.len()
            && liste
                .iter()
                .zip(&liste_quelle)
                .all(|(a, (i, n))| a.index == *i && a.name == *n);
        if !passt {
            // Bereiche erhalten, wo Index und Name passen (z. B. nach dem
            // Wechsel Instrumente -> Samples bleibt nichts uebrig).
            let neu: Vec<Eintrag> = liste_quelle
                .iter()
                .map(|(i, n)| {
                    liste
                        .iter()
                        .find(|e| e.index == *i && e.name == *n)
                        .cloned()
                        .unwrap_or_else(|| Eintrag::neu(*i, n.clone()))
                })
                .collect();
            info!("instr: Liste neu aufgebaut ({} Eintraege)", neu.len());
            set_eintraege(neu);
            liste = eintraege();
        }

        let mut geaendert = false;
        egui::ScrollArea::vertical()
            .id_salt("instr_liste")
            .show(ui, |ui| {
                for e in liste.iter_mut() {
                    ui.horizontal(|ui| {
                        let kurz: String = e.name.chars().take(18).collect();
                        ui.label(
                            egui::RichText::new(format!("{:>3} {}", e.index + 1, kurz)).small(),
                        );
                        let bereich = e.von.is_some();
                        let mut von = e.von.map(i32::from).unwrap_or(VON_STANDARD);
                        let mut bis = e.bis.map(i32::from).unwrap_or(BIS_STANDARD);
                        let a = ui.add(egui::DragValue::new(&mut von).range(0..=119).speed(0.5));
                        ui.label(egui::RichText::new(note_name(von)).small());
                        let b = ui.add(egui::DragValue::new(&mut bis).range(0..=119).speed(0.5));
                        ui.label(egui::RichText::new(note_name(bis)).small());
                        if a.changed() || b.changed() {
                            e.bereich_setzen(von as u8, bis as u8);
                            geaendert = true;
                            self.status =
                                format!("{}: {} bis {}", e.name, note_name(von), note_name(bis));
                        }
                        if bereich && ui.button("×").clicked() {
                            e.bereich_loeschen();
                            geaendert = true;
                            self.status = format!("{}: Bereich entfernt", e.name);
                        }
                    });
                }
            });
        if geaendert {
            set_eintraege(liste);
        }

        let mit_bereich = eintraege().iter().filter(|e| e.von.is_some()).count();
        ui.label(
            egui::RichText::new(format!(
                "{} Eintraege, {mit_bereich} mit Bereich. Ohne Bereich gilt der Standard \
                 (Instrument 30); Bereiche ausserhalb klingen nicht.",
                liste_quelle.len()
            ))
            .small()
            .color(egui::Color32::GRAY),
        );
        ui.label(
            egui::RichText::new(&self.status)
                .small()
                .color(egui::Color32::GRAY),
        );
    }
}
