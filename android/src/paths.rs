//! Verzeichnisse, in denen nach Modulen gesucht wird.

use std::path::PathBuf;

pub const EXTENSIONS: [&str; 5] = ["it", "xm", "mod", "s3m", "mtm"];

#[cfg(target_os = "android")]
pub fn search_dirs(app: &android_activity::AndroidApp) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(external) = crate::insets::external_files_dir(app) {
        let base = PathBuf::from(external);
        dirs.push(base.join("module"));
        dirs.push(base);
    }
    dirs.push(PathBuf::from("/sdcard/Download"));
    dirs.push(PathBuf::from("/sdcard/Documents"));
    dirs
}

#[cfg(not(target_os = "android"))]
pub fn search_dirs(_app: &()) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join("Downloads"));
        dirs.push(PathBuf::from(home).join("Documents"));
    }
    dirs.push(PathBuf::from("."));
    dirs
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
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                found.push(path);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}
