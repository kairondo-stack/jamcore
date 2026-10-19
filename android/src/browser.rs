//! Einfacher Datei-Browser für Modul-, Audio- **und** Bilddateien (Player-Tab
//! „Öffnen", Slicer-Tab „Audio öffnen", Laub-Tab „Bild laden …").
//!
//! Kein SAF/Java: die Activity ist `android.app.NativeActivity` und hat kein
//! `onActivityResult`. Der Browser läuft in egui und liest die Ordner direkt:
//! App-eigene Ordner zuerst, mit der „Alle Dateien"-Berechtigung zusätzlich
//! DCIM, Pictures, Music, Download und Documents (siehe
//! [`startordner`] und `crate::paths::ordner`). Ordner stehen zuerst, Dateien
//! werden nach [`Filter`] ausgewählt (Module, Audio oder Bilder); sortiert wird
//! mit der gemeinsamen Listen-Logik (`jamcore::modulebrowser::sortiere`).

use eframe::egui;
use jamcore::audio::ist_audio;
use jamcore::modulebrowser::{elternpfad, ist_modul, sortiere, Eintrag};
use std::path::{Path, PathBuf};

/// Was der Browser anzeigen soll (der Rest bleibt unverändert: gleiche Liste,
/// gleiche Bedienung, nur ein anderer Dateifilter und Fenstertitel).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Moduldateien (.it/.xm/.mod/.s3m/.mtm) — Player-Tab.
    Module,
    /// Audiodateien (.wav/.mp3/.ogg/.flac/.m4a/…) — Slicer-Tab.
    Audio,
    /// Bilddateien (.png/.jpg/.jpeg/.bmp) — Laub-Tab „Bild laden …".
    Bild,
}

impl Filter {
    /// Passt der Dateiname zu diesem Filter?
    fn passt(self, name: &str) -> bool {
        match self {
            Filter::Module => ist_modul(name),
            Filter::Audio => ist_audio(name),
            Filter::Bild => ist_bild(name),
        }
    }

    /// Fenstertitel (unterscheidet auch die Browser-Fenster).
    fn titel(self) -> &'static str {
        match self {
            Filter::Module => "Modul öffnen",
            Filter::Audio => "Audio öffnen",
            Filter::Bild => "Bild öffnen",
        }
    }

    /// Welche Ordnerliste gehört zu diesem Filter? (`paths::ordner` liefert
    /// dieselbe Quelle für Player, Slicer und Laub.)
    pub fn art(self) -> crate::paths::OrdnerArt {
        match self {
            Filter::Module => crate::paths::OrdnerArt::Module,
            Filter::Audio => crate::paths::OrdnerArt::Audio,
            Filter::Bild => crate::paths::OrdnerArt::Bild,
        }
    }
}

/// Ist die Datei ein Standbild? Endungen wie im Desktop-rrn5 und Bild-Tab
/// (dort liegt dieselbe Prüfung als private Funktion in `image.rs`).
fn ist_bild(pfad: &str) -> bool {
    std::path::Path::new(pfad)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            ["png", "jpg", "jpeg", "bmp"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        })
        .unwrap_or(false)
}

/// Startordner für einen Filter: App-eigene Ordner zuerst, danach (nur mit der
/// „Alle Dateien"-Berechtigung) die freigegebenen Systemordner. Nicht lesbare
/// Ordner meldet der Browser als Zeile im Fenster — kein Panic.
pub fn startordner(app: &android_activity::AndroidApp, filter: Filter) -> Vec<PathBuf> {
    crate::paths::ordner(app, filter.art())
}

pub struct Browser {
    pfad: PathBuf,
    eintraege: Vec<Eintrag>,
    fehler: String,
    start: Vec<PathBuf>,
    filter: Filter,
}

impl Browser {
    /// Browser für Moduldateien (Player-Tab).
    pub fn neu(start: Vec<PathBuf>) -> Self {
        Self::neu_filter(start, Filter::Module)
    }

    /// Browser für Audiodateien (Slicer-Tab: Wellenform zum Schneiden).
    pub fn neu_audio(start: Vec<PathBuf>) -> Self {
        Self::neu_filter(start, Filter::Audio)
    }

    /// Browser für Bilddateien (Laub-Tab: Foto → Blätter → Rhythmus).
    pub fn neu_bild(start: Vec<PathBuf>) -> Self {
        Self::neu_filter(start, Filter::Bild)
    }

    pub fn neu_filter(start: Vec<PathBuf>, filter: Filter) -> Self {
        let pfad = start
            .iter()
            .find(|p| p.is_dir())
            .cloned()
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut b = Self {
            pfad,
            eintraege: Vec::new(),
            fehler: String::new(),
            start,
            filter,
        };
        b.lesen();
        b
    }

    /// Ordner und passende Dateien des aktuellen Pfads einlesen.
    fn lesen(&mut self) {
        self.eintraege.clear();
        self.fehler.clear();
        match std::fs::read_dir(&self.pfad) {
            Ok(iter) => {
                for e in iter.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let ist_ordner = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if ist_ordner || self.filter.passt(&name) {
                        self.eintraege.push(Eintrag { name, ist_ordner });
                    }
                }
                sortiere(&mut self.eintraege);
            }
            Err(e) => {
                self.fehler = format!("{}: {e}", self.pfad.display());
                log::info!("browser: {}", self.fehler);
            }
        }
    }

    fn wechsle(&mut self, pfad: PathBuf) {
        log::info!("browser: {}", pfad.display());
        self.pfad = pfad;
        self.lesen();
    }

    /// Fenster zeichnen. `Some(pfad)` = eine passende Datei wurde gewählt.
    pub fn ui(&mut self, ctx: &egui::Context, offen: &mut bool) -> Option<PathBuf> {
        let mut gewaehlt = None;
        egui::Window::new(self.filter.titel())
            .open(offen)
            .order(egui::Order::Foreground)
            .resizable(true)
            .default_size([430.0, 520.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("↑").clicked() {
                        let hoch = elternpfad(&self.pfad.to_string_lossy());
                        self.wechsle(PathBuf::from(hoch));
                    }
                    let start = self.start.clone();
                    for s in start {
                        let name = kurz(&s);
                        if ui.small_button(name).clicked() {
                            self.wechsle(s.clone());
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(self.pfad.display().to_string())
                        .small()
                        .color(egui::Color32::GRAY),
                );
                if !self.fehler.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.fehler);
                }
                let mut wechsel: Option<PathBuf> = None;
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    // Eigener Salt je Filter: Modul- und Audio-Browser können
                    // gleichzeitig offen sein und dürfen sich nicht die
                    // Scrollposition teilen.
                    .id_salt(("browser_liste", self.filter.titel()))
                    .show(ui, |ui| {
                        for e in &self.eintraege {
                            let text = if e.ist_ordner {
                                format!("📁 {}", e.name)
                            } else {
                                e.name.clone()
                            };
                            if ui.selectable_label(false, text).clicked() {
                                if e.ist_ordner {
                                    wechsel = Some(self.pfad.join(&e.name));
                                } else {
                                    gewaehlt = Some(self.pfad.join(&e.name));
                                }
                            }
                        }
                    });
                if let Some(p) = wechsel {
                    self.wechsle(p);
                }
            });
        gewaehlt
    }
}

/// Kurzname eines Startordners für den Knopf.
fn kurz(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}
