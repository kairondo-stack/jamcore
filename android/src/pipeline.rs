//! Querschnitt durch die App: **Pipeline-Übersicht**, **Quellen-Merker** und
//! der **IT-Export mit Song-Message**.
//!
//! Warum ein eigenes Modul: die Stufen (Bild/Video/Kamera → Laub → Rhythmus →
//! `.it`, dazu Audio und Tempo) liegen in vielen Dateien. Der Überblick-Tab
//! soll sie an **einer** Stelle zeigen und markieren, was gerade läuft; der
//! Export braucht dieselben Angaben (Datum, Quelle, Notiz) für die
//! Song-Message. Beides gehört nicht in die jeweiligen Fachmodule, sondern
//! hierher.
//!
//! Der Export ist der zentrale Weg „Raster + Slices → `.it` mit Meta":
//! Samples kommen aus dem Slicer ([`crate::slicer::slices_fuer_it`]), Patterns
//! aus der Export-Liste (jedes merkt sich Datum, Quelle und Notiz beim
//! Hinzufügen). Geschrieben wird mit [`jamcore::itwrite`], die Zeilen der
//! Song-Message stehen im IT-Textfeld (Song Message, Special-Bit 0).

use eframe::egui;
use jamcore::grid::Pattern;
use jamcore::itwrite::{meta_zeile, Meta, Pattern as ItPattern, Sample, Zelle};
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Welche Bild-/Videoquelle zuletzt geladen wurde: `("bild", "klein_Quebec.jpg")`.
static QUELLE: OnceLock<Mutex<Option<(String, String)>>> = OnceLock::new();

fn quelle_slot() -> &'static Mutex<Option<(String, String)>> {
    QUELLE.get_or_init(|| Mutex::new(None))
}

/// Zuletzt geladenes Bild/Video merken (für die `quelle=`-Angabe im Export).
///
/// `art` ist `"bild"`, `"video"` oder `"kamera"`; `pfad` darf ein voller Pfad
/// sein — gespeichert wird nur der Dateiname, damit die Message kurz bleibt.
pub fn quelle_setzen(art: &str, pfad: &str) {
    let name = Path::new(pfad)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| pfad.to_string());
    if let Ok(mut slot) = quelle_slot().lock() {
        *slot = Some((art.to_string(), name.clone()));
    }
    info!("pipeline: Quelle {art}:{name}");
}

/// Die Quelle als Meta-Wert (`bild:klein_Quebec.jpg`); leer, wenn unbekannt.
pub fn quelle_meta() -> String {
    quelle_slot()
        .lock()
        .ok()
        .and_then(|q| q.clone())
        .map(|(art, name)| format!("{art}:{name}"))
        .unwrap_or_default()
}

/// Die Quelle für die Anzeige; `None` = noch nichts geladen.
pub fn quelle() -> Option<(String, String)> {
    quelle_slot().lock().ok().and_then(|q| q.clone())
}

// ==== Pipeline-Übersicht ==================================================

/// Was gerade läuft — `JamApp` füllt das je Frame aus dem, was messbar ist.
#[derive(Clone, Copy, Default)]
pub struct Live {
    /// Kamera liefert neue Frames (oder der Start-Knopf wurde gedrückt).
    pub kamera: bool,
    /// Ein Video wird geprüft (Laub-Tab).
    pub video: bool,
    /// Der Laub-Tab ist offen **und** es kommen Bilder — dort läuft die
    /// Erkennung je Anzeigeframe.
    pub laub: bool,
    /// Der Rhythmus-Tab ist offen (Rendern/Pattern-Bearbeitung).
    pub rhythmus: bool,
    /// Ein Export wurde gerade geschrieben (kurz nachleuchten).
    pub export: bool,
}

/// Farben der Stufen: jede Datei/Station hat ihre eigene Farbe, damit die
/// Pipeline im Überblick auf einen Blick lesbar ist.
const FARBE_QUELLE: egui::Color32 = egui::Color32::from_rgb(58, 110, 180); // blau
const FARBE_LAUB: egui::Color32 = egui::Color32::from_rgb(52, 140, 88); // grün
const FARBE_RHYTHMUS: egui::Color32 = egui::Color32::from_rgb(190, 130, 40); // orange
const FARBE_IT: egui::Color32 = egui::Color32::from_rgb(128, 88, 170); // violett
const FARBE_AUDIO: egui::Color32 = egui::Color32::from_rgb(40, 140, 140); // türkis
const FARBE_TEMPO: egui::Color32 = egui::Color32::from_rgb(170, 72, 72); // rot
const FARBE_EXPORT: egui::Color32 = egui::Color32::from_rgb(96, 96, 120); // grau

/// Eine Stufe als farbiger Kasten; aktiv = volle Farbe + „*", sonst gedimmt.
fn stufe(ui: &mut egui::Ui, text: &str, farbe: egui::Color32, aktiv: bool) {
    let (fuellung, schrift) = if aktiv {
        (farbe, egui::Color32::WHITE)
    } else {
        // Gedimmt: dieselbe Farbe, aber dunkel genug für weiße Schrift.
        (farbe.linear_multiply(0.45), egui::Color32::from_gray(205))
    };
    egui::Frame::NONE
        .fill(fuellung)
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| {
            let text = if aktiv {
                format!("* {text}")
            } else {
                text.to_string()
            };
            ui.label(egui::RichText::new(text).small().color(schrift));
        });
}

fn pfeil(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("->").small().color(egui::Color32::GRAY));
}

/// Der kompakte Pipeline-Abschnitt für den Überblick-Tab.
///
/// Kein eigenes Tab: die Stufen stehen als farbige Zeilen untereinander, die
/// aktive Stufe leuchtet („●"). So bleibt der Überblick der Ort, an dem man
/// den Weg Bild → `.it` sieht, ohne zwischen Tabs zu springen.
pub fn ui_pipeline(ui: &mut egui::Ui, live: &Live) {
    // Zeile 1: der Bildweg — jede Stufe mit ihrer Datei.
    ui.horizontal_wrapped(|ui| {
        stufe(ui, "Bild/Video/Kamera", FARBE_QUELLE, live.kamera || live.video);
        pfeil(ui);
        stufe(ui, "core/laub.rs", FARBE_LAUB, live.laub);
        pfeil(ui);
        stufe(ui, "laub.rs", FARBE_LAUB, live.laub);
        pfeil(ui);
        stufe(ui, "rhythm.rs", FARBE_RHYTHMUS, live.rhythmus);
        pfeil(ui);
        stufe(ui, "itwrite.rs", FARBE_IT, live.export);
        pfeil(ui);
        stufe(ui, ".it", FARBE_IT, live.export);
    });
    // Zeile 2: was neben dem Bildweg mitspielt.
    ui.horizontal_wrapped(|ui| {
        stufe(ui, "Audio: klang.rs", FARBE_AUDIO, live.export);
        pfeil(ui);
        stufe(ui, "slicer.rs", FARBE_AUDIO, live.export);
        pfeil(ui);
        stufe(ui, "Tempo: videotempo.rs", FARBE_TEMPO, live.video);
        pfeil(ui);
        stufe(ui, "Export: module/", FARBE_EXPORT, live.export);
    });
    let quelle = match quelle() {
        Some((art, name)) => format!("Quelle: {art}:{name}"),
        None => "Quelle: noch kein Bild/Video geladen".to_string(),
    };
    ui.label(
        egui::RichText::new(format!("* = läuft gerade · {quelle}"))
            .small()
            .color(egui::Color32::GRAY),
    );
}

// ==== Export mit Song-Message =============================================

/// Ein Pattern in der Export-Liste. Datum, Quelle und Notiz werden **beim
/// Hinzufügen** festgehalten — so steht später in der Message, wann und wozu
/// das Pattern aufgenommen wurde, auch wenn danach eine andere Quelle geladen
/// oder die Notiz geändert wird.
#[derive(Clone, Debug)]
struct Eintrag {
    pattern: Pattern,
    datum: String,
    quelle: String,
    notiz: String,
}

/// Was der Export-Abschnitt vom Aufrufer will.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Aktion {
    Keine,
    /// „Als .it sichern" wurde gedrückt.
    Sichern,
}

/// Export-Bereich im Überblick: Titel, Notiz, Pattern-Liste, Order und Knopf.
pub struct ExportPanel {
    pub titel: String,
    pub notiz: String,
    liste: Vec<Eintrag>,
    status: String,
    /// Pfad der zuletzt geschriebenen Datei (Anzeige).
    letzte_datei: Option<String>,
    /// Order der zuletzt geschriebenen Datei (1-basiert, wie die Anzeige).
    letzte_order: Vec<usize>,
    /// Bis wann der Export in der Pipeline als „läuft" gilt.
    live_bis: Option<Instant>,
}

impl Default for ExportPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportPanel {
    pub fn new() -> Self {
        Self {
            titel: "export".to_string(),
            notiz: String::new(),
            liste: Vec::new(),
            status: "Pattern hinzufügen, dann sichern — die Order steht in der Datei.".to_string(),
            letzte_datei: None,
            letzte_order: Vec::new(),
            live_bis: None,
        }
    }

    /// Leuchtet der Export gerade in der Pipeline? (kurz nach dem Schreiben)
    pub fn export_live(&self) -> bool {
        self.live_bis.is_some_and(|bis| Instant::now() < bis)
    }

    /// Zuletzt geschriebene Datei (Anzeige; der Aufrufer bekommt sie auch aus
    /// [`ExportPanel::sichern`]).
    fn letzte_datei(&self) -> Option<&str> {
        self.letzte_datei.as_deref()
    }

    /// Ein Pattern mit den aktuellen Angaben in die Liste legen.
    fn hinzufuegen(&mut self, pattern: &Pattern) {
        let eintrag = Eintrag {
            pattern: pattern.clone(),
            datum: jamcore::itwrite::datum_heute(),
            quelle: quelle_meta(),
            notiz: einzeilig(self.notiz.trim()),
        };
        let noten = pattern.notes().len();
        self.liste.push(eintrag);
        self.status = format!(
            "Pattern {} hinzugefügt ({noten} Noten, Quelle {})",
            self.liste.len(),
            if let Some((art, name)) = quelle() {
                format!("{art}:{name}")
            } else {
                "—".to_string()
            }
        );
        info!(
            "it: Pattern {} in die Export-Liste ({noten} Noten)",
            self.liste.len()
        );
    }

    /// Order-Anzeige `1 2 3 4` für `n` Patterns.
    fn order_text(anzahl: usize) -> String {
        (1..=anzahl)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Der Export-Abschnitt. `rhythmus`/`bild` sind die aktuellen Raster, die
    /// die „+"-Knöpfe in die Liste übernehmen.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rhythmus: &Pattern,
        bild: &Pattern,
    ) -> Aktion {
        let mut aktion = Aktion::Keine;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Titel").small());
            ui.add(
                egui::TextEdit::singleline(&mut self.titel)
                    .desired_width(120.0)
                    .hint_text("Dateiname"),
            );
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.notiz)
                .desired_rows(2)
                .hint_text("Notiz zum Pattern (Datum/Quelle kommen automatisch)"),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Rhythmus-Pattern +").clicked() {
                self.hinzufuegen(rhythmus);
            }
            if ui.button("Bild-Pattern +").clicked() {
                self.hinzufuegen(bild);
            }
            if ui.button("Liste leeren").clicked() {
                self.liste.clear();
                self.status = "Export-Liste geleert".to_string();
                info!("it: Export-Liste geleert");
            }
        });
        // Die Order ist das, was in der Datei steht: 1-basiert nummeriert,
        // Reihenfolge = Liste. Ohne Liste wird das aktuelle Rhythmus-Pattern
        // exportiert (eine Order).
        let anzahl = self.liste.len().max(1);
        ui.label(
            egui::RichText::new(format!(
                "Order: {} ({anzahl} Patterns)",
                Self::order_text(anzahl)
            ))
            .small()
            .color(egui::Color32::from_rgb(150, 200, 255)),
        );
        for (i, e) in self.liste.iter().enumerate().take(4) {
            ui.label(
                egui::RichText::new(format!(
                    "Pattern {}: {} · {} · {}",
                    i + 1,
                    e.datum,
                    if e.quelle.is_empty() { "—" } else { &e.quelle },
                    if e.notiz.is_empty() { "ohne Notiz" } else { &e.notiz }
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
        }
        if self.liste.len() > 4 {
            ui.label(
                egui::RichText::new(format!("… {} weitere", self.liste.len() - 4))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
        if let Some(pfad) = self.letzte_datei() {
            ui.label(
                egui::RichText::new(format!("zuletzt: {pfad}"))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(egui::Button::new("Als .it sichern").min_size(egui::vec2(132.0, 36.0)))
                .clicked()
            {
                aktion = Aktion::Sichern;
            }
            if !self.letzte_order.is_empty() {
                let order = self
                    .letzte_order
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                ui.label(
                    egui::RichText::new(format!("geschrieben: Order {order}"))
                        .small()
                        .color(egui::Color32::from_rgb(150, 200, 255)),
                );
            }
        });
        ui.label(
            egui::RichText::new(&self.status)
                .small()
                .color(egui::Color32::GRAY),
        );
        aktion
    }

    /// Die Song-Message aus der Export-Liste: je Pattern eine Zeile, danach
    /// die freien Notizen (Absätze bleiben erhalten).
    fn meta(&self, eintraege: &[Eintrag]) -> Meta {
        let mut zeilen: Vec<String> = eintraege
            .iter()
            .enumerate()
            .map(|(i, e)| meta_zeile(i + 1, &e.datum, &e.quelle, &e.notiz))
            .collect();
        let notizen = self.notiz.trim();
        if !notizen.is_empty() {
            zeilen.push(String::new()); // Leerzeile trennt die Notizen sichtbar
            zeilen.push("Notizen:".to_string());
            zeilen.extend(notizen.lines().map(|z| z.trim_end().to_string()));
        }
        Meta {
            titel: self.titel.trim().to_string(),
            zeilen,
        }
    }

    /// Das Raster als IT-Pattern. Ein leeres Raster wird zum leeren
    /// 64-Zeilen-Pattern (Parapointer 0), damit die Datei klein bleibt.
    fn it_pattern(raster: &Pattern, samples: usize) -> ItPattern {
        if raster.notes().is_empty() {
            return ItPattern::leer();
        }
        let zeilen = raster.steps_per_cycle().max(1);
        // Kanäle: das Raster ist polyphon (Tracker); IT hat 64.
        let kanaele = raster
            .notes()
            .iter()
            .map(|n| u16::from(n.channel) + 1)
            .max()
            .unwrap_or(1)
            .clamp(1, 64);
        let mut zellen = vec![Zelle::default(); zeilen as usize * kanaele as usize];
        for note in raster.notes() {
            let zeile = (note.step % zeilen) as usize;
            let kanal = usize::from(note.channel).min(kanaele as usize - 1);
            // IT zählt Samples 1-basiert; ohne Samples bleibt das Instrument
            // leer (sonst zeigte die Note ins Leere).
            let instrument = if samples > 0 {
                let nummer = usize::from(note.instrument) + 1;
                (nummer <= samples).then_some(nummer as u8)
            } else {
                None
            };
            zellen[zeile * kanaele as usize + kanal] = Zelle {
                note: Some(note.pitch.clamp(0, 119) as u8),
                instrument,
                ..Zelle::default()
            };
        }
        ItPattern {
            zeilen,
            kanaele,
            zellen,
        }
    }

    /// Export schreiben: Samples aus dem Slicer, Patterns aus der Liste (ohne
    /// Liste: das aktuelle `rhythmus`-Raster), Meta aus Datum/Quelle/Notiz.
    pub fn sichern(&mut self, rhythmus: &Pattern) -> Result<String, String> {
        let eintraege: Vec<Eintrag> = if self.liste.is_empty() {
            vec![Eintrag {
                pattern: rhythmus.clone(),
                datum: jamcore::itwrite::datum_heute(),
                quelle: quelle_meta(),
                notiz: einzeilig(self.notiz.trim()),
            }]
        } else {
            self.liste.clone()
        };
        // Slices als Samples: dieselben Puffer, die der Slicer spielt.
        let (rate, slices) = crate::slicer::slices_fuer_it().unwrap_or((44_100, Vec::new()));
        let samples: Vec<Sample> = slices
            .into_iter()
            .map(|(name, daten)| Sample {
                name,
                daten,
                rate,
                c5speed: rate,
            })
            .collect();
        let patterns: Vec<ItPattern> = eintraege
            .iter()
            .map(|e| Self::it_pattern(&e.pattern, samples.len()))
            .collect();
        let meta = self.meta(&eintraege);
        let titel = self.titel.trim();
        let titel = if titel.is_empty() { "export" } else { titel };
        let daten = jamcore::itwrite::bytes_mit_patterns_und_meta(&samples, titel, &patterns, &meta)?;
        let ordner = zielordner()?;
        let pfad = ordner.join(format!("{}.it", dateiname(titel)));
        std::fs::write(&pfad, &daten).map_err(|e| format!("{e} ({})", pfad.display()))?;
        let pfad_text = pfad.display().to_string();
        // Order wie im Kopf der Datei: 0, 1, 2 … — angezeigt und geloggt wird
        // 1-basiert wie „Pattern 1".
        let order: Vec<usize> = (1..=patterns.len()).collect();
        let order_csv = order
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        info!("it: Order {order_csv} ({} Patterns)", patterns.len());
        info!(
            "it: geschrieben: {pfad_text} ({} Patterns, {} Samples, {} Meta-Zeilen, {} Bytes)",
            patterns.len(),
            samples.len(),
            meta.zeilen.len(),
            daten.len()
        );
        self.letzte_datei = Some(pfad_text.clone());
        self.letzte_order = order;
        self.live_bis = Some(Instant::now() + Duration::from_secs(3));
        self.status = format!(
            "gesichert: {pfad_text} (Order {}, {} Samples)",
            order_csv,
            samples.len()
        );
        Ok(pfad_text)
    }
}

/// Eine Notiz für die Meta-Zeile: Zeilenumbrüche werden zu „ / ", damit eine
/// Pattern-Zeile eine Zeile bleibt (die Voll-Notiz steht darunter).
fn einzeilig(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|z| !z.is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Ordner für den Export: der des letzten Slicer-Exports, sonst
/// `<App-Ordner>/module` (dort sucht der Player).
fn zielordner() -> Result<PathBuf, String> {
    if let Some(pfad) = crate::slicer::letzter_export() {
        if let Some(eltern) = Path::new(&pfad).parent() {
            if eltern.is_dir() {
                return Ok(eltern.to_path_buf());
            }
        }
    }
    let app = crate::current_app()?;
    let basis = crate::insets::external_files_dir(&app)
        .ok_or_else(|| "kein App-Verzeichnis".to_string())?;
    let ordner = PathBuf::from(basis).join("module");
    std::fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    Ok(ordner)
}

/// Dateiname aus dem Titel: nur Zeichen, die auf Android/ext4 sicher sind.
fn dateiname(titel: &str) -> String {
    let name: String = titel
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches('_');
    if name.is_empty() {
        "export".to_string()
    } else {
        name.to_string()
    }
}

/// Den Schreibfehler eines Exports einheitlich loggen (der Aufrufer zeigt ihn
/// im Status). `warn`, weil ein fehlgeschlagener Export nicht die App bricht.
pub fn export_fehler(fehler: &str) {
    warn!("it: Export fehlgeschlagen: {fehler}");
}
