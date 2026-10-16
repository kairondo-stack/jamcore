//! Instrument-/Sample-Tab: die Eintraege des geladenen Moduls mit
//! Noten- **und** Bildbereichen.
//!
//! Jeder Eintrag kann einen Bereich `von..=bis` (MIDI-Noten, Anzeige als
//! Notenname) und einen **Bildbereich** x/y bekommen (Anteile der Bildbreite
//! bzw. -hoehe, 0..100 %, Vorgabe = ganzes Bild). Der Bild-Scan klingt nur,
//! wenn Note **und** x **und** y im Bereich liegen; ausserhalb bleibt es still.
//! Klingen mehrere Bereiche fuer eine Note, spielen **alle** zusammen
//! (Nutzerentscheidung 2026-10-14). Solange **kein** Notenbereich gesetzt ist,
//! nimmt der Klangweg seinen Standard (wie bisher) — sonst waere der
//! Modul-Klang nach dem ersten Blick in diesen Tab verstummt.
//!
//! Mehrere Eintraege lassen sich mit dem Haken links markieren; „Bereich auf
//! Markierte" setzt dann den Bereich aus der Zeile „Bereich fuer Markierte"
//! auf alle markierten Zeilen gleichzeitig.

use eframe::egui;
use jamcore::grid::note_name;
use jamcore::instrument::{setze_bereich, treffer, treffer_note, Bereich, Eintrag};
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

/// Instrumente fuer eine Note (siehe `jamcore::instrument::treffer_note`).
pub fn instrumente_fuer(note: i32) -> Option<Vec<i32>> {
    treffer_note(&eintraege(), note.clamp(0, 119) as u8)
}

/// Instrumente fuer eine Note an einer Bildstelle: x/y als Anteile der Bild-
/// breite/-hoehe (0..=1). Leer, wenn nichts gesetzt ist oder die Stelle
/// ausserhalb aller Bildbereiche liegt.
pub fn instrumente_fuer_bild(note: i32, x: f32, y: f32) -> Vec<i32> {
    treffer(&eintraege(), note.clamp(0, 119) as u8, x, y)
}

/// Eine Note ueber die Zuordnung spielen.
///
/// `Some(true)` = gespielt, `Some(false)` = still (Bereiche gesetzt, aber
/// kein Treffer), `None` = keine Zuordnung — der Aufrufer nimmt seinen
/// Standard (Instrument 30).
pub fn spielen(quelle: &ModulQuelle, note: i32, volume: f32) -> Option<bool> {
    spielen_mit_kanaelen(quelle, note, volume, None, None).0
}

/// Wie `spielen`, gibt zusätzlich die gestarteten Modul-Kanäle zurück — der
/// Bild-Scan merkt sie sich, damit „Ton aus" sie stoppen kann.
///
/// `x`/`y` sind die Bildanteile des Treffers (0..=1); `None` heisst „keine
/// Einschränkung" — so rufen die Stellen ohne Bildkoordinaten auf. Nur wenn
/// **beide** Werte vorliegen, greifen die x/y-Bereiche der Eintraege.
pub fn spielen_mit_kanaelen(
    quelle: &ModulQuelle,
    note: i32,
    volume: f32,
    x: Option<f32>,
    y: Option<f32>,
) -> (Option<bool>, Vec<i32>) {
    // `None` = kein Eintrag hat einen Notenbereich -> der Aufrufer nimmt
    // seinen Standard (Instrument 30); das gilt auch fuer Bildtreffer.
    let liste = match (x, y) {
        (Some(x), Some(y)) => {
            if instrumente_fuer(note).is_none() {
                return (None, Vec::new());
            }
            instrumente_fuer_bild(note, x, y)
        }
        _ => match instrumente_fuer(note) {
            Some(liste) => liste,
            None => return (None, Vec::new()),
        },
    };
    let note = note.clamp(0, 119);
    let mut kanaele = Vec::new();
    for index in &liste {
        if let Some(kanal) = quelle.note_spielen(*index, note, volume) {
            kanaele.push(kanal);
        }
    }
    (Some(!liste.is_empty()), kanaele)
}

/// Voreinstellung fuer einen noch leeren Bereich: C-4 bis C-5.
const VON_STANDARD: i32 = 60;
const BIS_STANDARD: i32 = 72;

/// Kompakte Anzeige der Bereiche eines Eintrags fuer die Listenzeile.
fn bereiche_text(e: &Eintrag) -> String {
    let noten = match (e.von, e.bis) {
        (Some(von), Some(bis)) => {
            format!("{}–{}", note_name(i32::from(von)), note_name(i32::from(bis)))
        }
        _ => "ohne Note".to_string(),
    };
    if e.xy_eingeschraenkt() {
        format!(
            "{noten} · x {:.0}–{:.0} % · y {:.0}–{:.0} %",
            e.x_von * 100.0,
            e.x_bis * 100.0,
            e.y_von * 100.0,
            e.y_bis * 100.0
        )
    } else {
        format!("{noten} · ganzes Bild")
    }
}

/// Anteile (0..=1) als ganze Prozent fuer die DragValues.
fn prozent(von: f32, bis: f32) -> (f32, f32) {
    ((von * 100.0).round(), (bis * 100.0).round())
}

/// Ein Paar DragValues „von–bis" in Prozent; `true`, wenn eines bewegt wurde.
fn xy_regler(ui: &mut egui::Ui, von: &mut f32, bis: &mut f32) -> bool {
    let a = ui.add(
        egui::DragValue::new(von)
            .range(0.0..=100.0)
            .suffix(" %"),
    );
    ui.label(egui::RichText::new("–").small());
    let b = ui.add(
        egui::DragValue::new(bis)
            .range(0.0..=100.0)
            .suffix(" %"),
    );
    a.changed() || b.changed()
}

pub struct InstrumentPanel {
    /// Instrumente oder Samples listen?
    pub samples: bool,
    status: String,
    /// Auswaehlhaken je Listenzeile („Bereich auf Markierte").
    markiert: Vec<bool>,
    /// Die Fader „Bereich fuer Markierte": sie gelten fuer alle markierten
    /// Zeilen, wenn „Bereich auf Markierte" gedrueckt wird.
    bereich: Bereich,
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
            status: "Bereich einstellen: von/bis als Notennamen, x/y als Anteil des Bildes; \
                     leere Plaetze sind moeglich."
                .to_string(),
            markiert: Vec::new(),
            bereich: Bereich {
                von: VON_STANDARD,
                bis: BIS_STANDARD,
                x_von: 0.0,
                x_bis: 1.0,
                y_von: 0.0,
                y_bis: 1.0,
            },
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
                    "Mehrere Bereiche duerfen sich ueberlappen — dann klingen alle. \
                     x/y grenzen den Bild-Scan ein.",
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
        // Haken gehoeren zur Zeile: nach einem Quellenwechsel auf die neue
        // Laenge bringen (neue Zeilen starten ohne Markierung).
        if self.markiert.len() != liste.len() {
            self.markiert.resize(liste.len(), false);
        }

        let mut geaendert = false;

        // Markierung: Bereich fuer mehrere Eintraege auf einmal setzen.
        let markierte = self.markiert.iter().filter(|m| **m).count();
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("Markierte: {markierte}")).small());
            if ui.button("alle markieren").clicked() {
                self.markiert.iter_mut().for_each(|m| *m = true);
            }
            if ui.button("Markierung weg").clicked() {
                self.markiert.iter_mut().for_each(|m| *m = false);
            }
            if ui.button("x/y zuruecksetzen (ganzes Bild)").clicked() {
                for e in liste.iter_mut() {
                    e.xy_zuruecksetzen();
                }
                geaendert = true;
                self.status = format!(
                    "x/y auf das ganze Bild zurueckgesetzt ({} Eintraege)",
                    liste.len()
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Bereich fuer Markierte:").small());
            let mut von = self.bereich.von;
            let mut bis = self.bereich.bis;
            let a = ui.add(egui::DragValue::new(&mut von).range(0..=119).speed(0.5));
            ui.label(egui::RichText::new(note_name(von)).small());
            let b = ui.add(egui::DragValue::new(&mut bis).range(0..=119).speed(0.5));
            ui.label(egui::RichText::new(note_name(bis)).small());
            ui.label(egui::RichText::new("x").small());
            let (mut x_von, mut x_bis) = prozent(self.bereich.x_von, self.bereich.x_bis);
            let x_geaendert = xy_regler(ui, &mut x_von, &mut x_bis);
            ui.label(egui::RichText::new("y").small());
            let (mut y_von, mut y_bis) = prozent(self.bereich.y_von, self.bereich.y_bis);
            let y_geaendert = xy_regler(ui, &mut y_von, &mut y_bis);
            if a.changed() || b.changed() || x_geaendert || y_geaendert {
                self.bereich = Bereich {
                    von,
                    bis,
                    x_von: (x_von / 100.0).clamp(0.0, 1.0),
                    x_bis: (x_bis / 100.0).clamp(0.0, 1.0),
                    y_von: (y_von / 100.0).clamp(0.0, 1.0),
                    y_bis: (y_bis / 100.0).clamp(0.0, 1.0),
                };
            }
            if ui.button("Bereich auf Markierte").clicked() {
                let indizes: Vec<usize> = self
                    .markiert
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| **m)
                    .map(|(i, _)| i)
                    .collect();
                if indizes.is_empty() {
                    self.status = "Nichts markiert — erst links Haken setzen.".to_string();
                } else {
                    setze_bereich(&mut liste, &indizes, self.bereich);
                    geaendert = true;
                    self.status = format!(
                        "Bereich auf {} Eintraege: {}–{}, x {:.0}–{:.0} %, y {:.0}–{:.0} %",
                        indizes.len(),
                        note_name(self.bereich.von),
                        note_name(self.bereich.bis),
                        self.bereich.x_von * 100.0,
                        self.bereich.x_bis * 100.0,
                        self.bereich.y_von * 100.0,
                        self.bereich.y_bis * 100.0
                    );
                }
            }
        });

        egui::ScrollArea::vertical()
            .id_salt("instr_liste")
            .show(ui, |ui| {
                for (i, e) in liste.iter_mut().enumerate() {
                    // Zeile 1: Haken, Name, kompakte Anzeige der Bereiche.
                    ui.horizontal(|ui| {
                        if let Some(m) = self.markiert.get_mut(i) {
                            ui.checkbox(m, "");
                        }
                        let kurz: String = e.name.chars().take(18).collect();
                        ui.label(
                            egui::RichText::new(format!("{:>3} {}", e.index + 1, kurz)).small(),
                        );
                        // Die Anzeige darf die Zeile nicht sprengen: sie wird
                        // am Zeilenende abgeschnitten (Werte stehen in den
                        // Reglern darunter).
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(bereiche_text(e))
                                    .small()
                                    .color(egui::Color32::GRAY),
                            )
                            .truncate(),
                        );
                    });
                    // Zeile 2: Notenbereich (wie bisher) plus „×" zum Loeschen.
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("von").small());
                        let bereich = e.von.is_some();
                        let mut von = e.von.map(i32::from).unwrap_or(VON_STANDARD);
                        let mut bis = e.bis.map(i32::from).unwrap_or(BIS_STANDARD);
                        let a = ui.add(egui::DragValue::new(&mut von).range(0..=119).speed(0.5));
                        ui.label(egui::RichText::new(note_name(von)).small());
                        ui.label(egui::RichText::new("bis").small());
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
                    // Zeile 3: Bildbereich x/y in Prozent.
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(egui::RichText::new("x").small());
                        let (mut x_von, mut x_bis) = prozent(e.x_von, e.x_bis);
                        let x_geaendert = xy_regler(ui, &mut x_von, &mut x_bis);
                        ui.label(egui::RichText::new("y").small());
                        let (mut y_von, mut y_bis) = prozent(e.y_von, e.y_bis);
                        let y_geaendert = xy_regler(ui, &mut y_von, &mut y_bis);
                        if x_geaendert || y_geaendert {
                            e.xy_setzen(
                                x_von / 100.0,
                                x_bis / 100.0,
                                y_von / 100.0,
                                y_bis / 100.0,
                            );
                            geaendert = true;
                            self.status = format!(
                                "{}: x {:.0}–{:.0} %, y {:.0}–{:.0} %",
                                e.name,
                                e.x_von * 100.0,
                                e.x_bis * 100.0,
                                e.y_von * 100.0,
                                e.y_bis * 100.0
                            );
                        }
                    });
                    ui.add_space(2.0);
                }
            });
        if geaendert {
            set_eintraege(liste);
        }

        let mit_bereich = eintraege().iter().filter(|e| e.von.is_some()).count();
        let mit_bild = eintraege().iter().filter(|e| e.xy_eingeschraenkt()).count();
        ui.label(
            egui::RichText::new(format!(
                "{} Eintraege, {mit_bereich} mit Notenbereich, {mit_bild} mit x/y. Ohne \
                 Notenbereich gilt der Standard (Instrument 30); ausserhalb der Bereiche \
                 klingt nichts.",
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
