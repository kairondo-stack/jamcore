//! Tonfolgen-Presets im Kreis-Tab: Audio analysieren, wiederkehrende Motive
//! finden und ihre x-Bereiche auf den geteilten Kreis-/Scan-Bereich legen.
//!
//! Warum ein eigener Zustand mit Thread: Die Datei kann mehrere Minuten lang
//! sein. Würde die Analyse im UI-Thread laufen, stünde die App währenddessen
//! still; Fortschritt und Abbruch brauchen deshalb einen Hintergrund-Thread.
//! Der Zustand liegt global, damit `view_kreis` nur eine Zeile braucht und
//! die übrigen Panels unangetastet bleiben.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;

use crate::browser;
use crate::quint::QuintPanel;

/// Mehr als das lohnt sich für Presets nicht und dauert auf dem Gerät zu
/// lange. Analysiert wird der Anfang; das wird geloggt.
const MAX_SEKUNDEN: f32 = 60.0;
/// Kürzere Folgen sind selten echte Motive, längere zu speziell.
const MOTIV_LAENGE: usize = 3;
/// Blockgröße für Fortschritt und Abbruch (muss nicht exakt sein).
const BLOCK_SEKUNDEN: f32 = 5.0;
/// So viele Presets zeigt die Liste höchstens.
const MAX_PRESETS: usize = 12;

/// Ergebnis eines Analyse-Laufs aus dem Hintergrund-Thread.
enum Ergebnis {
    Fertig {
        motive: Vec<jamcore::tonfolge::Motiv>,
        gesamt_s: f32,
        noten: usize,
        orig_s: f32,
        gekuerzt: bool,
    },
    Abgebrochen,
    Fehler(String),
}

/// Ein laufender Analyse-Thread samt Fortschritt und Abbruchschalter.
struct Lauf {
    rx: Receiver<Ergebnis>,
    abbruch: Arc<AtomicBool>,
    fortschritt: Arc<AtomicU32>,
}

struct Zustand {
    /// Datei-Browser, offen solange `Some`.
    browser: Option<browser::Browser>,
    lauf: Option<Lauf>,
    motive: Vec<jamcore::tonfolge::Motiv>,
    /// Zuletzt analysierte (oder gerade analysierte) Datei.
    datei: Option<String>,
    gesamt_s: f32,
    noten: usize,
    orig_s: f32,
    gekuerzt: bool,
    status: String,
}

impl Default for Zustand {
    fn default() -> Self {
        Self {
            browser: None,
            lauf: None,
            motive: Vec::new(),
            datei: None,
            gesamt_s: 0.0,
            noten: 0,
            orig_s: 0.0,
            gekuerzt: false,
            status: "Audio analysieren: wiederkehrende Tonfolgen werden als Presets angeboten."
                .to_string(),
        }
    }
}

impl Zustand {
    /// Ergebnis des Hintergrund-Threads abholen (nicht blockieren).
    fn ergebnis_abholen(&mut self) {
        let empfangen = match self.lauf.as_ref() {
            Some(lauf) => lauf.rx.try_recv(),
            None => return,
        };
        match empfangen {
            Ok(Ergebnis::Fertig {
                motive,
                gesamt_s,
                noten,
                orig_s,
                gekuerzt,
            }) => {
                let anzahl = motive.len();
                self.lauf = None;
                self.motive = motive;
                self.gesamt_s = gesamt_s;
                self.noten = noten;
                self.orig_s = orig_s;
                self.gekuerzt = gekuerzt;
                let grenze = if gekuerzt {
                    format!(" (nur Anfang, Original {orig_s:.1} s)")
                } else {
                    String::new()
                };
                self.status = if anzahl == 0 {
                    format!("kreis: keine wiederkehrenden Tonfolgen in {gesamt_s:.1} s{grenze}")
                } else {
                    format!("kreis: {anzahl} Motiv(e) in {gesamt_s:.1} s{grenze}")
                };
                log::info!("{}", self.status);
            }
            Ok(Ergebnis::Abgebrochen) => {
                self.lauf = None;
                self.status = "kreis: Analyse abgebrochen".to_string();
                log::info!("kreis: Analyse abgebrochen");
            }
            Ok(Ergebnis::Fehler(e)) => {
                self.lauf = None;
                self.status = format!("kreis: Analyse fehlgeschlagen: {e}");
                log::info!("kreis: Analyse fehlgeschlagen: {e}");
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.lauf = None;
                self.status = "kreis: Analyse-Thread ohne Ergebnis beendet".to_string();
                log::info!("kreis: Analyse-Thread ohne Ergebnis beendet");
            }
        }
    }

    /// Analyse im Hintergrund starten (Pfad ist bereits geprüft/gewählt).
    fn starte(&mut self, pfad: String) {
        if self.lauf.is_some() {
            return;
        }
        let abbruch = Arc::new(AtomicBool::new(false));
        let fortschritt = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();
        let ab = Arc::clone(&abbruch);
        let fo = Arc::clone(&fortschritt);
        let thread_pfad = pfad.clone();
        log::info!("kreis: analysiere {pfad} (Motivlänge {MOTIV_LAENGE}, max {MAX_SEKUNDEN:.0} s)");
        std::thread::spawn(move || {
            let ergebnis = analysiere(&thread_pfad, &ab, &fo);
            // Ein geschlossener Kanal heißt nur: App ist weg.
            let _ = tx.send(ergebnis);
        });
        self.datei = Some(pfad);
        self.motive.clear();
        self.lauf = Some(Lauf {
            rx,
            abbruch,
            fortschritt,
        });
        self.status = "kreis: Analyse läuft …".to_string();
    }

    fn abbrechen(&mut self) {
        if let Some(lauf) = self.lauf.as_ref() {
            lauf.abbruch.store(true, Ordering::Relaxed);
            self.status = "kreis: Abbruch angefordert …".to_string();
            log::info!("kreis: Abbruch angefordert");
        }
    }
}

/// Globaler Zustand: `view_kreis` ruft nur [`ui`] auf.
fn zustand() -> &'static Mutex<Zustand> {
    static ZUSTAND: OnceLock<Mutex<Zustand>> = OnceLock::new();
    ZUSTAND.get_or_init(|| Mutex::new(Zustand::default()))
}

/// Abschnitt „Tonfolgen-Presets" im Kreis-Tab.
///
/// `kreis` ist der geteilte x-Bereich; ein Klick auf ein Motiv schreibt ihn
/// direkt, damit der Aufrufer ihn wie gewohnt an Bild/B2 weiterreicht.
pub fn ui(ui: &mut egui::Ui, kreis: &mut QuintPanel) {
    // Ein vergifteter Mutex darf die App nicht mitnehmen: der Zustand ist
    // reine Anzeige/Analyse und bleibt nach einem Panic benutzbar.
    let mut z = zustand()
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner());
    z.ergebnis_abholen();

    // Datei-Browser (modal) vor dem Abschnitt zeichnen.
    if let Some(mut b) = z.browser.take() {
        let mut offen = true;
        let gewaehlt = b.ui(ui.ctx(), &mut offen);
        if let Some(pfad) = gewaehlt {
            z.starte(pfad.display().to_string());
            offen = false;
        }
        if offen {
            z.browser = Some(b);
        }
    }

    ui.separator();
    ui.heading("Tonfolgen-Presets");
    ui.label(
        egui::RichText::new(
            "Audio nach wiederkehrenden Tonfolgen durchsuchen und als x-Bereich \
             auf Kreis/Bild legen (max. 60 s).",
        )
        .small()
        .color(egui::Color32::DARK_GRAY),
    );

    ui.horizontal_wrapped(|ui| {
        if ui.button("Audio analysieren …").clicked() {
            let start = match crate::current_app() {
                Ok(app) => browser::startordner(&app, browser::Filter::Audio),
                Err(_) => Vec::new(),
            };
            z.browser = Some(browser::Browser::neu_filter(start, browser::Filter::Audio));
            z.status = "kreis: Audiodatei für Tonfolgen wählen …".to_string();
        }
        if z.datei.is_some() && z.lauf.is_none() && ui.button("Analysieren").clicked() {
            if let Some(pfad) = z.datei.clone() {
                z.starte(pfad);
            }
        }
        if z.lauf.is_some() && ui.button("Abbrechen").clicked() {
            z.abbrechen();
        }
    });

    if let Some(lauf) = z.lauf.as_ref() {
        let fortschritt = lauf.fortschritt.load(Ordering::Relaxed) as f32 / 100.0;
        ui.add(egui::ProgressBar::new(fortschritt).show_percentage());
        // Der Thread kann den Wert nicht selbst neu zeichnen.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
    ui.label(egui::RichText::new(&z.status).small());

    if let Some(pfad) = z.datei.as_deref() {
        let name = Path::new(pfad)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| pfad.to_string());
        let zusatz = if z.gekuerzt {
            format!(
                ", auf {MAX_SEKUNDEN:.0} s gekürzt (Original {:.1} s)",
                z.orig_s
            )
        } else {
            String::new()
        };
        ui.label(
            egui::RichText::new(format!(
                "{name}: {:.1} s analysiert, {} Tonhöhen-Fenster{zusatz}",
                z.gesamt_s, z.noten
            ))
            .small()
            .color(egui::Color32::DARK_GRAY),
        );
    }

    if !z.motive.is_empty() {
        ui.label(
            egui::RichText::new("Gefundene Motive (Klick setzt den x-Bereich):")
                .small()
                .color(egui::Color32::DARK_GRAY),
        );
        // Klonen, damit der Klick den Zustand ändern darf, während die Liste
        // noch gezeichnet wird.
        let motive = z.motive.clone();
        for (i, motiv) in motive.iter().enumerate() {
            let text = format!(
                "Motiv {}: {} ({} Noten, {}×)",
                i + 1,
                notennamen(&motiv.noten),
                motiv.noten.len(),
                motiv.starts.len()
            );
            if ui.button(text).clicked() {
                let bereiche = jamcore::tonfolge::x_bereiche(motiv, z.gesamt_s);
                if let (Some(erst), Some(letzt)) = (bereiche.first(), bereiche.last()) {
                    // Ein einziger geteilter x-Bereich: die Spanne des Motivs.
                    kreis.setze_x_bereich(erst.0, letzt.1);
                    let liste = bereiche
                        .iter()
                        .map(|(von, bis)| format!("{von:.2}-{bis:.2}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    z.status = format!(
                        "kreis: Motiv {} -> {} x-Bereiche ({liste})",
                        i + 1,
                        bereiche.len()
                    );
                    log::info!("{}", z.status);
                }
            }
        }
    } else if z.lauf.is_none() && z.datei.is_some() {
        ui.label(
            egui::RichText::new(format!(
                "Keine wiederkehrenden Tonfolgen (mind. {MOTIV_LAENGE} Noten, 2×)."
            ))
            .small()
            .color(egui::Color32::DARK_GRAY),
        );
    }
}

/// MIDI-Noten als kurze Namen, z. B. „C E G" (ohne Oktave, wie im Beispiel).
fn notennamen(noten: &[i32]) -> String {
    const NAMEN: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    noten
        .iter()
        .map(|n| NAMEN[n.rem_euclid(12) as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// Analyse im Hintergrund: laden, auf 60 s kürzen, Tonhöhen blockweise
/// bestimmen und daraus Motive suchen.
fn analysiere(pfad: &str, abbruch: &AtomicBool, fortschritt: &AtomicU32) -> Ergebnis {
    fortschritt.store(2, Ordering::Relaxed);
    let audio = match jamcore::audio::laden(pfad) {
        Ok(audio) => audio,
        Err(e) => return Ergebnis::Fehler(e),
    };
    if abbruch.load(Ordering::Relaxed) {
        return Ergebnis::Abgebrochen;
    }
    let rate = audio.rate.max(1);
    let orig_s = audio.proben.len() as f32 / rate as f32;
    let mut proben = audio.proben;
    let max = (MAX_SEKUNDEN * rate as f32) as usize;
    let gekuerzt = proben.len() > max;
    if gekuerzt {
        proben.truncate(max);
        log::info!(
            "kreis: Audio auf {MAX_SEKUNDEN:.0} s begrenzt (nur Anfang; Original {orig_s:.1} s)"
        );
    }
    let gesamt_s = proben.len() as f32 / rate as f32;
    fortschritt.store(8, Ordering::Relaxed);

    // Blockweise, damit Abbruch und Fortschritt auch bei langen Dateien
    // greifen. Die Blocklänge ist ein Vielfaches der 50-ms-Fenster.
    let block = ((rate as f32 * BLOCK_SEKUNDEN) as usize).max(1);
    let mut spur: Vec<(f32, f32)> = Vec::with_capacity(proben.len() / block.max(1) + 1);
    let mut pos = 0;
    while pos < proben.len() {
        if abbruch.load(Ordering::Relaxed) {
            return Ergebnis::Abgebrochen;
        }
        let ende = (pos + block).min(proben.len());
        let offset = pos as f32 / rate as f32;
        spur.extend(
            jamcore::tonfolge::tonhoehen(&proben[pos..ende], rate)
                .into_iter()
                .map(|(t, f)| (t + offset, f)),
        );
        pos = ende;
        let anteil = pos as f32 / proben.len().max(1) as f32;
        fortschritt.store(10 + (anteil * 70.0) as u32, Ordering::Relaxed);
    }

    if abbruch.load(Ordering::Relaxed) {
        return Ergebnis::Abgebrochen;
    }
    let noten: Vec<(f32, i32)> = spur
        .iter()
        .map(|(t, f)| (*t, jamcore::tonfolge::midi(*f)))
        .collect();
    fortschritt.store(85, Ordering::Relaxed);

    let mut motive = jamcore::tonfolge::finde_motive(&noten, MOTIV_LAENGE);
    // Häufige Motive zuerst: mehr Vorkommen = verlässlicheres Preset.
    motive.sort_by(|a, b| {
        b.starts
            .len()
            .cmp(&a.starts.len())
            .then(a.starts[0].total_cmp(&b.starts[0]))
    });
    motive.truncate(MAX_PRESETS);
    fortschritt.store(100, Ordering::Relaxed);
    log::info!(
        "kreis: {} Motiv(e) aus {} Noten ({:.1} s)",
        motive.len(),
        noten.len(),
        gesamt_s
    );
    Ergebnis::Fertig {
        motive,
        gesamt_s,
        noten: noten.len(),
        orig_s,
        gekuerzt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notennamen_ohne_oktave() {
        assert_eq!(notennamen(&[60, 64, 67]), "C E G");
        assert_eq!(notennamen(&[69, 71, 72]), "A B C");
        // Negative/kleine Werte dürfen nicht panicken (rem_euclid).
        assert_eq!(notennamen(&[0, -1]), "C B");
    }
}
