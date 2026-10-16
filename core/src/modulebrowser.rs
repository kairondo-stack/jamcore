//! Datei-Browser für Moduldateien (Player-Tab „Öffnen").
//!
//! Reine Listen-Logik, damit die App nur zeichnet: welche Namen sind
//! Moduldateien, wie wird sortiert (Ordner zuerst), wie heißt der Elternpfad.

/// Endungen, die als Modul gelten (wie `rrn5`/`it_live`).
pub const EXTENSIONS: [&str; 5] = ["it", "xm", "mod", "s3m", "mtm"];

/// Ein Eintrag der Liste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub name: String,
    pub ist_ordner: bool,
}

/// Ist der Name eine Moduldatei (Endung, Groß/Klein egal)?
pub fn ist_modul(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((_stamm, endung)) => EXTENSIONS
            .iter()
            .any(|e| e.eq_ignore_ascii_case(endung)),
        None => false,
    }
}

/// Ordner zuerst, dann Dateien, je alphabetisch (Groß/Klein egal).
pub fn sortiere(eintraege: &mut [Eintrag]) {
    eintraege.sort_by(|a, b| {
        b.ist_ordner
            .cmp(&a.ist_ordner)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Elternpfad: `/a/b` → `/a`, `/a` → `/`, `/` → `/`, `a` → `.`.
pub fn elternpfad(pfad: &str) -> String {
    let p = pfad.trim_end_matches('/');
    if p.is_empty() {
        return "/".to_string();
    }
    match p.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
        None => ".".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, ordner: bool) -> Eintrag {
        Eintrag {
            name: name.to_string(),
            ist_ordner: ordner,
        }
    }

    #[test]
    fn nur_moduldateien() {
        assert!(ist_modul("song.it"));
        assert!(ist_modul("SONG.IT"), "Endung groß geschrieben");
        assert!(ist_modul("a.b.mod"), "mehrere Punkte");
        assert!(!ist_modul("bild.png"));
        assert!(!ist_modul("ohne_endung"));
        assert!(!ist_modul(""));
    }

    #[test]
    fn ordner_zuerst_und_alphabetisch() {
        let mut liste = vec![
            e("zebra.it", false),
            e("Alpha", true),
            e("beta.xm", false),
            e("anfang", true),
        ];
        sortiere(&mut liste);
        assert_eq!(
            liste,
            vec![
                e("Alpha", true),
                e("anfang", true),
                e("beta.xm", false),
                e("zebra.it", false),
            ],
            "Ordner zuerst, dann Dateien, je alphabetisch ohne Groß/Klein"
        );
    }

    #[test]
    fn elternpfad_kuerzt_den_pfad() {
        assert_eq!(elternpfad("/sdcard/Download"), "/sdcard");
        assert_eq!(elternpfad("/sdcard/Download/"), "/sdcard");
        assert_eq!(elternpfad("/sdcard"), "/");
        assert_eq!(elternpfad("/"), "/");
        assert_eq!(elternpfad("module"), ".", "relativer Pfad ohne Schrägstrich");
    }
}
