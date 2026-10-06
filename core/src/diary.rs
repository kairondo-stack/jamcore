//! Musiktagebuch (S10): Einträge mit Metadaten und Noten.
//!
//! Ein Eintrag ist eine Textdatei; die Noten stehen im CSV-Format des
//! Desktop-Recorders (`timestamp,instrument,pitch,glide`), damit dieselben
//! Dateien in beiden Welten lesbar sind.
//!
//! Die Zeit kommt als Unix-Sekunden herein und wird ohne Zeitzone in
//! `JJJJ-MM-TT hh:mm` gerechnet (UTC) — der Zivilkalender ist hier
//! nachgebaut, weil das Projekt keine Datums-Bibliothek mitbringt.

use crate::grid::Pattern;

/// Ein Tagebuch-Eintrag.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Unix-Sekunden (UTC).
    pub zeit: u64,
    pub name: String,
    pub bpm: Option<f32>,
    pub notiz: String,
    pub noten: Pattern,
    /// Dateiname einer Kameraaufnahme (Video mit Ton) im selben Ordner.
    pub video: Option<String>,
}

impl Entry {
    pub fn neu(zeit: u64, name: &str) -> Self {
        Self {
            zeit,
            name: name.to_string(),
            bpm: None,
            notiz: String::new(),
            noten: Pattern::new(),
            video: None,
        }
    }

    /// Datum und Uhrzeit als `JJJJ-MM-TT hh:mm` (UTC).
    pub fn datum_zeit(&self) -> String {
        datum_zeit(self.zeit)
    }

    /// Textdatei-Format; Notizen mit Zeilenumbrüchen werden escaped.
    pub fn to_text(&self) -> String {
        let mut s = String::from("JamCore-Tagebuch v1\n");
        s += &format!("zeit={}\n", self.zeit);
        s += &format!("name={}\n", self.name);
        s += &format!(
            "bpm={}\n",
            self.bpm.map(|b| format!("{b}")).unwrap_or_default()
        );
        s += &format!("notiz={}\n", self.notiz.replace('\n', "\\n"));
        if let Some(video) = &self.video {
            s += &format!("video={video}\n");
        }
        s += "noten:\n";
        s += &self.noten.to_csv();
        s
    }

    /// Liest das Textformat; Fehler nennen die kaputte Stelle.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut zeilen = text.lines();
        let kopf = zeilen.next().unwrap_or_default();
        if kopf != "JamCore-Tagebuch v1" {
            return Err(format!("unbekannter Kopf: {kopf:?}"));
        }
        let mut zeit = 0u64;
        let mut name = String::new();
        let mut bpm = None;
        let mut notiz = String::new();
        let mut video = None;
        let mut noten_text = String::new();
        let mut bei_noten = false;
        for zeile in zeilen {
            if bei_noten {
                noten_text += zeile;
                noten_text += "\n";
                continue;
            }
            if zeile == "noten:" {
                bei_noten = true;
                continue;
            }
            let Some((schluessel, wert)) = zeile.split_once('=') else {
                return Err(format!("Zeile ohne „=“: {zeile:?}"));
            };
            match schluessel {
                "zeit" => zeit = wert.trim().parse().map_err(|e| format!("zeit: {e}"))?,
                "name" => name = wert.to_string(),
                "bpm" => {
                    bpm = if wert.trim().is_empty() {
                        None
                    } else {
                        Some(wert.trim().parse().map_err(|e| format!("bpm: {e}"))?)
                    }
                }
                "notiz" => notiz = wert.replace("\\n", "\n"),
                "video" => video = Some(wert.to_string()),
                anderes => return Err(format!("unbekannter Schlüssel: {anderes:?}")),
            }
        }
        if !bei_noten {
            return Err("Abschnitt „noten:“ fehlt".to_string());
        }
        Ok(Self {
            zeit,
            name,
            bpm,
            notiz,
            video,
            noten: Pattern::from_csv(&noten_text, crate::grid::STEPS_PER_CYCLE)?,
        })
    }
}

/// `JJJJ-MM-TT hh:mm` aus Unix-Sekunden (UTC), ohne Datums-Bibliothek.
pub fn datum_zeit(unix: u64) -> String {
    let tage = (unix / 86_400) as i64;
    let rest = unix % 86_400;
    let (jahr, monat, tag) = zivil_aus_tagen(tage);
    format!(
        "{jahr:04}-{monat:02}-{tag:02} {:02}:{:02}",
        rest / 3600,
        (rest % 3600) / 60
    )
}

/// Tage seit 1970-01-01 nach Zivilkalender (Howard Hinnants Algorithmus).
fn zivil_aus_tagen(tage_seit_epoche: i64) -> (i64, u32, u32) {
    let z = tage_seit_epoche + 719_468;
    let aera = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - aera * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + aera * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let jahr = if m <= 2 { y + 1 } else { y };
    (jahr, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Note;

    fn beispiel() -> Entry {
        let mut e = Entry::neu(1_791_000_000, "Zug-Skizze");
        e.bpm = Some(120.5);
        e.notiz = "erste Idee\nzweite Zeile".to_string();
        e.noten.insert(Note {
            step: 0,
            instrument: 0,
            pitch: 60,
            glide: false,
        });
        e.noten.insert(Note {
            step: 4,
            instrument: 1,
            pitch: 67,
            glide: true,
        });
        e
    }

    #[test]
    fn text_und_zurueck_erhaelt_alles() {
        let e = beispiel();
        let gelesen = Entry::from_text(&e.to_text()).expect("lesbar");
        assert_eq!(gelesen, e, "Notiz mit Umbruch, BPM, Noten");
    }

    #[test]
    fn video_dateiname_ueberlebt_den_roundtrip() {
        let mut e = beispiel();
        e.video = Some("demo.mp4".to_string());
        let text = e.to_text();
        assert!(text.contains("video=demo.mp4"), "{text}");
        let gelesen = Entry::from_text(&text).expect("lesbar");
        assert_eq!(gelesen.video.as_deref(), Some("demo.mp4"));
        assert_eq!(gelesen, e);
    }

    #[test]
    fn text_ohne_bpm_und_notiz_geht_auch() {
        let e = Entry::neu(0, "leer");
        let gelesen = Entry::from_text(&e.to_text()).expect("lesbar");
        assert_eq!(gelesen, e);
        assert!(gelesen.bpm.is_none());
    }

    #[test]
    fn kaputte_dateien_werden_gemeldet() {
        assert!(Entry::from_text("Quatsch\n").is_err());
        assert!(Entry::from_text("JamCore-Tagebuch v1\nzeit=abc\nnoten:\n").is_err());
        assert!(
            Entry::from_text("JamCore-Tagebuch v1\nzeit=1\n").is_err(),
            "noten fehlt"
        );
        assert!(
            Entry::from_text("JamCore-Tagebuch v1\nzeit=1\ntier=hund\nnoten:\n").is_err(),
            "unbekannter Schlüssel"
        );
    }

    #[test]
    fn datum_an_der_epoche() {
        assert_eq!(datum_zeit(0), "1970-01-01 00:00");
        assert_eq!(datum_zeit(86_399), "1970-01-01 23:59");
        assert_eq!(datum_zeit(86_400), "1970-01-02 00:00");
    }

    #[test]
    fn datum_heute_und_schaltjahr() {
        // 2024-02-29 12:34 UTC = 1709210040
        assert_eq!(datum_zeit(1_709_210_040), "2024-02-29 12:34");
        // 2026-10-06 00:00 UTC = 1791244800
        assert_eq!(datum_zeit(1_791_244_800), "2026-10-06 00:00");
    }

    #[test]
    fn noten_bleiben_desktop_lesbar() {
        let e = beispiel();
        let text = e.to_text();
        let noten_zeilen: Vec<&str> = text
            .lines()
            .skip_while(|z| *z != "noten:")
            .skip(1)
            .collect();
        assert_eq!(noten_zeilen, vec!["0,0,60,0", "0.25,1,67,1"]);
    }
}
