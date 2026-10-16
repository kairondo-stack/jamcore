//! Verzeichnisse, in denen nach Modulen, Audio- und Bilddateien gesucht wird.
//!
//! Zwei Quellen (Nutzerentscheidung D2, 2026-10-16):
//!
//! 1. die **App-eigenen Ordner** unter `…/files/` — ohne Berechtigung lesbar
//!    und per USB erreichbar,
//! 2. die **Systemordner** (`DCIM`, `Pictures`, `Music`, `Download`,
//!    `Documents`) — nur mit der „Alle Dateien"-Berechtigung
//!    ([`crate::ordner::alle_dateien_erlaubt`]).

use std::path::PathBuf;

/// Wofür eine Ordnerliste gedacht ist (der Dateifilter bleibt beim Aufrufer).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrdnerArt {
    /// Bilder und Videos — Bild-Tab. Der Bild-Tab (`image.rs`) scannt seine
    /// Ordner heute selbst; diese Liste ist für Browser und Scan bereit und
    /// wird noch nicht aufgerufen.
    #[allow(dead_code)]
    Bild,
    /// Audiodateien (.wav/.mp3/…) — Slicer-Tab und Aufnahmen.
    Audio,
    /// Moduldateien (.it/.xm/.mod/.s3m/.mtm) — Player-Tab.
    Module,
}

impl OrdnerArt {
    /// App-eigene Ordner (immer lesbar, per USB erreichbar).
    fn app_ordner(self) -> &'static [&'static str] {
        match self {
            OrdnerArt::Bild => &["bild", "bilder", "videos"],
            OrdnerArt::Audio => &["audio", "recordings", "wav"],
            OrdnerArt::Module => &["module"],
        }
    }

    /// Systemordner unter `/sdcard` — nur mit „Alle Dateien" lesbar.
    fn system_ordner(self) -> &'static [&'static str] {
        match self {
            OrdnerArt::Bild => &["DCIM", "Pictures", "Download", "Documents"],
            OrdnerArt::Audio => &["Music", "Download", "Documents"],
            OrdnerArt::Module => &["Download", "Documents"],
        }
    }
}

/// Ordner für eine Dateiart: App-eigene zuerst, dann — nur wenn die
/// „Alle Dateien"-Berechtigung erteilt ist — die Systemordner.
#[cfg(target_os = "android")]
pub fn ordner(app: &android_activity::AndroidApp, art: OrdnerArt) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(external) = crate::insets::external_files_dir(app) {
        let basis = PathBuf::from(external);
        // Beim ersten Zugriff anlegen (der Start macht es schon in
        // `android_main`); Fehler landen nur im Log.
        crate::ordner::app_ordner_anlegen_in(&basis);
        for name in art.app_ordner() {
            dirs.push(basis.join(name));
        }
        dirs.push(basis);
    } else {
        log::info!("jamandroid: kein App-Ordner (external files dir fehlt)");
    }
    if crate::ordner::alle_dateien_erlaubt(app) {
        for name in art.system_ordner() {
            dirs.push(PathBuf::from("/sdcard").join(name));
        }
    }
    dirs
}

/// Suchordner für Module (Player-Tab) — App-Ordner zuerst, mit „Alle Dateien"
/// zusätzlich `Download` und `Documents`.
#[cfg(target_os = "android")]
pub fn search_dirs(app: &android_activity::AndroidApp) -> Vec<PathBuf> {
    ordner(app, OrdnerArt::Module)
}

#[cfg(not(target_os = "android"))]
pub fn ordner(_app: &(), _art: OrdnerArt) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join("Downloads"));
        dirs.push(PathBuf::from(home).join("Documents"));
    }
    dirs.push(PathBuf::from("."));
    dirs
}

#[cfg(not(target_os = "android"))]
pub fn search_dirs(app: &()) -> Vec<PathBuf> {
    ordner(app, OrdnerArt::Module)
}

pub fn scan(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            log::info!("jamandroid: {} nicht lesbar (Berechtigung?)", dir.display());
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if jamcore::modulebrowser::ist_modul(name) {
                found.push(path);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}
