//! Musiktagebuch (S10): Einträge anlegen, ansehen, speichern.
//!
//! Ein Eintrag übernimmt das aktuelle Rhythmus-Raster und — wenn gemessen —
//! das Tempo aus dem Tempo-Tab. Gespeichert wird je Eintrag eine Textdatei im
//! App-Ordner `tagebuch/`; das Notenformat ist das des Desktop-Recorders
//! (`jamcore::diary` und `jamcore::grid`).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Color32};
use jamcore::diary::Entry;
use jamcore::grid::Pattern;
use log::info;

pub struct DiaryPanel {
    eintraege: Vec<(PathBuf, Entry)>,
    auswahl: Option<usize>,
    name: String,
    notiz: String,
    status: String,
    geladen: bool,
}

impl Default for DiaryPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl DiaryPanel {
    pub fn new() -> Self {
        Self {
            eintraege: Vec::new(),
            auswahl: None,
            name: String::new(),
            notiz: String::new(),
            status: "Eintrag übernimmt das Rhythmus-Raster und das gemessene Tempo.".to_string(),
            geladen: false,
        }
    }

    fn ordner(&self) -> Option<PathBuf> {
        let app = crate::current_app().ok()?;
        let basis = crate::insets::external_files_dir(&app)?;
        let ordner = PathBuf::from(basis).join("tagebuch");
        std::fs::create_dir_all(&ordner).ok()?;
        Some(ordner)
    }

    fn laden(&mut self) {
        self.eintraege.clear();
        let Some(ordner) = self.ordner() else {
            self.status = "Kein App-Verzeichnis für das Tagebuch.".to_string();
            return;
        };
        let Ok(eintraege) = std::fs::read_dir(&ordner) else {
            return;
        };
        for eintrag in eintraege.flatten() {
            let pfad = eintrag.path();
            if pfad.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&pfad) else {
                continue;
            };
            match Entry::from_text(&text) {
                Ok(inhalt) => self.eintraege.push((pfad, inhalt)),
                Err(e) => info!("tagebuch: {} nicht lesbar: {e}", pfad.display()),
            }
        }
        // Neueste zuerst.
        self.eintraege.sort_by(|a, b| b.1.zeit.cmp(&a.1.zeit));
        self.geladen = true;
        self.status = format!("{} Einträge", self.eintraege.len());
    }

    fn speichern(&mut self, bpm: Option<f32>, noten: &Pattern) {
        let zeit = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut eintrag = Entry::neu(zeit, &self.name);
        eintrag.bpm = bpm;
        eintrag.notiz = self.notiz.clone();
        eintrag.noten = noten.clone();
        let Some(ordner) = self.ordner() else {
            self.status = "Kein App-Verzeichnis für das Tagebuch.".to_string();
            return;
        };
        let dateiname = format!("{}.txt", eintrag.datum_zeit().replace([' ', ':'], "_"));
        let pfad = ordner.join(&dateiname);
        match schreiben_atomar(&pfad, &eintrag.to_text()) {
            Ok(()) => {
                self.status = format!(
                    "Gespeichert: {} ({} Noten)",
                    dateiname,
                    eintrag.noten.notes().len()
                );
                info!("tagebuch: gespeichert {}", pfad.display());
                self.name.clear();
                self.notiz.clear();
                self.laden();
            }
            Err(e) => self.status = format!("Fehler beim Speichern: {e}"),
        }
    }

    /// Neben die Zieldatei schreiben und umbenennen (wie beim Rhythmus-Export).
    fn schreiben_atomar(pfad: &Path, inhalt: &str) -> std::io::Result<()> {
        let tmp = pfad.with_extension("tmp");
        std::fs::write(&tmp, inhalt)?;
        std::fs::rename(&tmp, pfad)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, bpm: Option<f32>, noten: &Pattern) {
        if !self.geladen {
            self.laden();
        }

        ui.horizontal(|ui| {
            if ui.button("Neu laden").clicked() {
                self.laden();
            }
            ui.label(&self.status);
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.name);
            let tempo = bpm
                .map(|b| format!("{b:.1} BPM"))
                .unwrap_or_else(|| "kein Tempo gemessen".to_string());
            ui.label(format!("· {tempo} · {} Noten im Raster", noten.notes().len()));
            if ui.button("Eintrag speichern").clicked() {
                self.speichern(bpm, noten);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Notiz");
            ui.text_edit_singleline(&mut self.notiz);
        });
        ui.separator();

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label("Einträge");
                let mut auswahl = self.auswahl;
                for (i, (_, e)) in self.eintraege.iter().enumerate() {
                    let text = format!(
                        "{} · {} ({} Noten)",
                        e.datum_zeit(),
                        if e.name.is_empty() { "ohne Namen" } else { &e.name },
                        e.noten.notes().len()
                    );
                    if ui.selectable_label(auswahl == Some(i), text).clicked() {
                        auswahl = Some(i);
                    }
                }
                self.auswahl = auswahl;
            });
            ui.separator();
            ui.vertical(|ui| match self.auswahl.and_then(|i| self.eintraege.get(i)) {
                Some((pfad, e)) => {
                    ui.label(
                        egui::RichText::new(format!("{} · {}", e.datum_zeit(), e.name))
                            .strong(),
                    );
                    if let Some(b) = e.bpm {
                        ui.label(format!("Tempo: {b:.1} BPM"));
                    }
                    if !e.notiz.is_empty() {
                        ui.label(format!("Notiz: {}", e.notiz));
                    }
                    ui.label(format!("Datei: {}", pfad.display()));
                    ui.label(egui::RichText::new("Noten (Desktop-CSV):").small());
                    let csv = e.noten.to_csv();
                    ui.label(
                        egui::RichText::new(if csv.is_empty() {
                            "(leer)".to_string()
                        } else {
                            csv
                        })
                        .monospace()
                        .small(),
                    );
                }
                None => {
                    ui.colored_label(Color32::GRAY, "Eintrag wählen");
                }
            });
        });
    }
}