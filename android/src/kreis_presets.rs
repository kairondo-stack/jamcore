//! Tonfolgen-Presets im Kreis-Tab: Audio analysieren, wiederkehrende Motive
//! finden und ihre x-Bereiche auf den geteilten Kreis-/Scan-Bereich legen.
//!
//! Warum ein eigener Zustand mit Thread: Die Datei kann mehrere Minuten lang
//! sein. Würde die Analyse im UI-Thread laufen, stünde die App währenddessen
//! still; Fortschritt und Abbruch brauchen deshalb einen Hintergrund-Thread.
//! Der Zustand liegt global, damit `view_kreis` nur eine Zeile braucht und
//! die übrigen Panels unangetastet bleiben.
//!
//! Dazu kommen drei Quellen ohne Audiodatei: feste Thai-Motive (7-TET, aus
//! `jamcore::tonfolge`), Akkord-Presets aus `crate::akkord` und die interne
//! Aufnahme des Mixer-Ausgangs (`crate::klang`, kein Mikrofon). Alle landen
//! im selben `Preset`-Typ, damit der Mini-Editor (Start/Ende, Umkehren,
//! Zuschneiden, Löschen, Rückgängig) für jede Quelle gleich funktioniert.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;

use jamcore::klang::Welle;
use jamcore::tonfolge::{self, Motiv};

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
/// Abtastrate der selbst erzeugten Presets (Thai, Akkorde). 44,1 kHz passt
/// direkt zum Klang-Thread, damit `puffer_spielen` nicht umrechnen muss.
const SYNTH_RATE: u32 = 44_100;
/// Dauer eines Thai-Tons in der Synthese (die x-Bereiche sind gleich breit).
const THAI_TON_S: f32 = 0.25;
/// Dauer eines Akkord-Presets (alle Töne klingen gleichzeitig).
const AKKORD_S: f32 = 1.5;

/// Ergebnis eines Analyse-Laufs aus dem Hintergrund-Thread.
enum Ergebnis {
    Fertig {
        presets: Vec<Preset>,
        gesamt_s: f32,
        noten: usize,
        orig_s: f32,
        gekuerzt: bool,
        herkunft: Herkunft,
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

/// Woher ein Preset kommt. Bestimmt die Beschriftung und beim erneuten
/// Analysieren, welche alten Presets ersetzt werden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Herkunft {
    Datei,
    Aufnahme,
    Thai,
    Akkord,
}

/// Ein Preset: Noten, x-Bereiche und die PCM-Daten für den Mini-Editor.
///
/// `x_bereiche` sind **relativ zum eigenen PCM** (0..1). `basis` ist die
/// Platzierung im geteilten Kreis-/Bild-Raum; so kann ein Motiv aus einer
/// Datei weiterhin an seiner Stelle im Stück liegen, während Thai/Akkorde
/// das ganze Bild (0..1) belegen.
struct Preset {
    name: String,
    herkunft: Herkunft,
    noten: Vec<i32>,
    x_bereiche: Vec<(f32, f32)>,
    basis: (f32, f32),
    pcm: Vec<f32>,
    rate: u32,
    /// Ein-Schritt-Rückgängig für den Mini-Editor.
    undo: Option<Box<PresetSnapshot>>,
}

#[derive(Clone)]
struct PresetSnapshot {
    noten: Vec<i32>,
    x_bereiche: Vec<(f32, f32)>,
    basis: (f32, f32),
    pcm: Vec<f32>,
}

impl Preset {
    fn dauer_s(&self) -> f32 {
        if self.rate == 0 {
            0.0
        } else {
            self.pcm.len() as f32 / self.rate as f32
        }
    }

    /// Beschriftung der Zeile, z. B. „Motiv: C E G · 1.8 s“.
    fn titel(&self) -> String {
        let prefix = match self.herkunft {
            Herkunft::Datei | Herkunft::Aufnahme => "Motiv",
            Herkunft::Thai => "Thai",
            Herkunft::Akkord => "Akkord",
        };
        format!("{prefix}: {} · {:.1} s", self.name, self.dauer_s())
    }

    /// x-Bereiche im geteilten Kreis-Raum (0..1).
    fn kreis_bereiche(&self) -> Vec<(f32, f32)> {
        let (basis_von, basis_bis) = self.basis;
        let spanne = (basis_bis - basis_von).clamp(0.0, 1.0);
        self.x_bereiche
            .iter()
            .map(|&(von, bis)| {
                (
                    (basis_von + von * spanne).clamp(0.0, 1.0),
                    (basis_von + bis * spanne).clamp(0.0, 1.0),
                )
            })
            .collect()
    }

    /// PCM als 16-Bit für den vorhandenen Klangweg (`klang::puffer_spielen`).
    fn pcm_i16(&self) -> Vec<i16> {
        pcm_i16(&self.pcm)
    }

    fn schnappschuss(&mut self) {
        self.undo = Some(Box::new(PresetSnapshot {
            noten: self.noten.clone(),
            x_bereiche: self.x_bereiche.clone(),
            basis: self.basis,
            pcm: self.pcm.clone(),
        }));
    }

    /// Letzten Schnappschuss wiederherstellen; `false`, wenn keiner da ist.
    fn zurueck(&mut self) -> bool {
        let Some(s) = self.undo.take() else {
            return false;
        };
        self.noten = s.noten;
        self.x_bereiche = s.x_bereiche;
        self.basis = s.basis;
        self.pcm = s.pcm;
        true
    }

    /// Nach einer PCM-Bearbeitung Noten und x-Bereiche synchron übernehmen.
    /// `aenderungen` kommt aus `tonfolge::bereiche_*` und nennt den alten
    /// Notenindex, damit die Notenliste nicht heimlich verrutscht.
    fn setze_bereiche(&mut self, aenderungen: Vec<(usize, (f32, f32))>) {
        let mut noten = Vec::with_capacity(aenderungen.len());
        let mut bereiche = Vec::with_capacity(aenderungen.len());
        for (index, bereich) in aenderungen {
            if let Some(&note) = self.noten.get(index) {
                noten.push(note);
                bereiche.push(bereich);
            }
        }
        self.noten = noten;
        self.x_bereiche = bereiche;
    }
}

/// Start-/Ende-Marker im Mini-Editor (als Anteile 0..1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Start,
    Ende,
}

/// Zustand des offenen Mini-Editors. Nur ein Preset ist gleichzeitig offen;
/// so bleibt die Liste auf dem Handy kurz und die Marker sind eindeutig.
struct Editor {
    preset: usize,
    von: f32,
    bis: f32,
    ziehen: Option<Marker>,
}

impl Editor {
    fn neu(preset: usize) -> Self {
        Self {
            preset,
            von: 0.0,
            bis: 1.0,
            ziehen: None,
        }
    }
}

struct Zustand {
    /// Datei-Browser, offen solange `Some`.
    browser: Option<browser::Browser>,
    lauf: Option<Lauf>,
    presets: Vec<Preset>,
    /// Zuletzt analysierte Datei (bei einer Aufnahme `None`).
    datei: Option<String>,
    gesamt_s: f32,
    noten: usize,
    orig_s: f32,
    gekuerzt: bool,
    status: String,
    editor: Option<Editor>,
    /// Grundton der Akkord-Knöpfe im Kreis-Tab (unabhängig vom Rhythmus-Tab).
    akkord_grundton: i32,
}

impl Default for Zustand {
    fn default() -> Self {
        Self {
            browser: None,
            lauf: None,
            presets: Vec::new(),
            datei: None,
            gesamt_s: 0.0,
            noten: 0,
            orig_s: 0.0,
            gekuerzt: false,
            status: "Audio analysieren, Thai-Motive, Akkorde oder intern aufnehmen."
                .to_string(),
            editor: None,
            akkord_grundton: 60,
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
                presets,
                gesamt_s,
                noten,
                orig_s,
                gekuerzt,
                herkunft,
            }) => {
                let anzahl = presets.len();
                self.lauf = None;
                // Alte Presets derselben Quelle ersetzen, andere (Thai,
                // Akkorde, frühere Aufnahmen) bleiben erhalten.
                self.presets.retain(|p| p.herkunft != herkunft);
                let mut alle = presets;
                alle.append(&mut self.presets);
                alle.truncate(MAX_PRESETS);
                self.presets = alle;
                self.gesamt_s = gesamt_s;
                self.noten = noten;
                self.orig_s = orig_s;
                self.gekuerzt = gekuerzt;
                self.editor = None;
                let grenze = if gekuerzt {
                    format!(" (Ringpuffer/Anfang, Original {orig_s:.1} s)")
                } else {
                    String::new()
                };
                self.status = if herkunft == Herkunft::Aufnahme {
                    let motive = match anzahl {
                        0 => "keine Motive".to_string(),
                        1 => "1 Motiv".to_string(),
                        n => format!("{n} Motive"),
                    };
                    format!("kreis: intern aufgenommen {gesamt_s:.1} s{grenze}, {motive}")
                } else if anzahl == 0 {
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

    /// Datei-Analyse im Hintergrund starten (Pfad ist bereits geprüft/gewählt).
    fn starte(&mut self, pfad: String) {
        if self.lauf.is_some() {
            self.status = "kreis: Analyse läuft bereits".to_string();
            log::info!("{}", self.status);
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
            let ergebnis = analysiere_datei(&thread_pfad, &ab, &fo);
            // Ein geschlossener Kanal heißt nur: App ist weg.
            let _ = tx.send(ergebnis);
        });
        self.datei = Some(pfad);
        self.lauf = Some(Lauf {
            rx,
            abbruch,
            fortschritt,
        });
        self.status = "kreis: Analyse läuft …".to_string();
    }

    /// Bereits vorhandenes PCM (interne Aufnahme) analysieren.
    fn starte_pcm(&mut self, pcm: Vec<f32>, rate: u32, orig_s: f32, herkunft: Herkunft) {
        if self.lauf.is_some() {
            self.status = "kreis: Analyse läuft bereits — Aufnahme verworfen".to_string();
            log::info!("{}", self.status);
            return;
        }
        let rate = rate.max(1);
        let gesamt_s = pcm.len() as f32 / rate as f32;
        let gekuerzt = orig_s > gesamt_s + 0.05;
        let abbruch = Arc::new(AtomicBool::new(false));
        let fortschritt = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();
        let ab = Arc::clone(&abbruch);
        let fo = Arc::clone(&fortschritt);
        log::info!(
            "kreis: analysiere {} ({:.1} s, Rate {rate})",
            if herkunft == Herkunft::Aufnahme {
                "interne Aufnahme"
            } else {
                "PCM"
            },
            gesamt_s
        );
        std::thread::spawn(move || {
            let ergebnis =
                analysiere_proben(pcm, rate, herkunft, orig_s, gekuerzt, &ab, &fo);
            let _ = tx.send(ergebnis);
        });
        self.datei = None;
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

    /// Interne Aufnahme starten (Mixer-Ausgang, kein Mikrofon).
    fn aufnahme_starten(&mut self) {
        match crate::klang::intern_aufnahme_starten() {
            Ok(()) => {
                self.status = "kreis: interne Aufnahme läuft (Mixer, max. 8 s)".to_string();
                log::info!("kreis: interne Aufnahme gestartet");
            }
            Err(e) => {
                self.status = format!("kreis: interne Aufnahme nicht gestartet: {e}");
                log::info!("{}", self.status);
            }
        }
    }

    /// Interne Aufnahme stoppen und analysieren.
    fn aufnahme_stoppen(&mut self) {
        let Some(aufnahme) = crate::klang::intern_aufnahme_stoppen() else {
            self.status = "kreis: keine interne Aufnahme aktiv".to_string();
            return;
        };
        if aufnahme.pcm.len() < aufnahme.rate as usize / 10 {
            self.status = format!(
                "kreis: interne Aufnahme zu kurz ({:.2} s)",
                aufnahme.dauer_s
            );
            log::info!("{}", self.status);
            return;
        }
        log::info!(
            "kreis: interne Aufnahme {:.1} s -> Analyse",
            aufnahme.dauer_s
        );
        self.starte_pcm(
            aufnahme.pcm,
            aufnahme.rate,
            aufnahme.dauer_s,
            Herkunft::Aufnahme,
        );
    }

    /// Die sechs festen Thai-Motive als Presets anbieten. Das PCM wird aus
    /// den 7-TET-Stufen synthetisiert, damit die Motive auch ohne Audiodatei
    /// hörbar und im Mini-Editor bearbeitbar sind.
    fn thai_einbauen(&mut self) {
        let motive = tonfolge::thai_motive();
        let mut neu = Vec::with_capacity(motive.len());
        for (i, motiv) in motive.iter().enumerate() {
            let name = tonfolge::THAI_NAMEN.get(i).copied().unwrap_or("Thai");
            neu.push(Preset {
                name: format!("{name}: {}", notennamen(&motiv.noten)),
                herkunft: Herkunft::Thai,
                noten: motiv.noten.clone(),
                x_bereiche: tonfolge::gleich_bereiche(motiv.noten.len()),
                basis: (0.0, 1.0),
                pcm: synth_notenfolge(&motiv.noten, SYNTH_RATE, true),
                rate: SYNTH_RATE,
                undo: None,
            });
        }
        // Nur alte Thai-Presets ersetzen; Aufnahmen/Akkorde bleiben stehen.
        self.presets.retain(|p| p.herkunft != Herkunft::Thai);
        let mut alle = neu;
        alle.append(&mut self.presets);
        alle.truncate(MAX_PRESETS);
        self.presets = alle;
        self.editor = None;
        self.status = format!(
            "kreis: {} Thai-Motive eingebaut (7-TET, {:.0} Cent)",
            motive.len(),
            tonfolge::THAI_CENT
        );
        log::info!("{}", self.status);
    }

    /// Akkord-Preset anlegen/ersetzen. Die x-Bereiche setzt der Aufrufer,
    /// weil dafür der geteilte Kreis-Bereich gebraucht wird.
    fn akkord_einbauen(&mut self, preset: usize, grundton: i32) -> Option<String> {
        let (name, _) = crate::akkord::PRESETS.get(preset)?;
        let name = (*name).to_string();
        let noten = crate::akkord::noten(preset, grundton);
        if noten.is_empty() {
            return None;
        }
        let titel = format!("{} {}", notennamen(&[grundton]), name);
        let neu = Preset {
            name: titel.clone(),
            herkunft: Herkunft::Akkord,
            noten: noten.clone(),
            x_bereiche: tonfolge::gleich_bereiche(noten.len()),
            basis: (0.0, 1.0),
            pcm: synth_akkord(&noten, SYNTH_RATE),
            rate: SYNTH_RATE,
            undo: None,
        };
        if let Some(pos) = self
            .presets
            .iter()
            .position(|p| p.herkunft == Herkunft::Akkord && p.name == titel)
        {
            self.presets[pos] = neu;
        } else {
            self.presets.insert(0, neu);
            self.presets.truncate(MAX_PRESETS);
        }
        self.editor = None;
        Some(titel)
    }
}

/// Globaler Zustand: `view_kreis` ruft nur [`ui`] auf.
fn zustand() -> &'static Mutex<Zustand> {
    static ZUSTAND: OnceLock<Mutex<Zustand>> = OnceLock::new();
    ZUSTAND.get_or_init(|| Mutex::new(Zustand::default()))
}

/// Abschnitt „Tonfolgen-Presets" im Kreis-Tab.
///
/// `kreis` ist der geteilte x-Bereich; ein Klick auf ein Preset schreibt ihn
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
             auf Kreis/Bild legen (max. 60 s). Thai und Akkorde brauchen keine Datei.",
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
        if ui.button("Thai-Motive").clicked() {
            z.thai_einbauen();
        }
        if crate::klang::intern_aufnahme_laeuft() {
            if ui.button("Aufnahme stoppen").clicked() {
                z.aufnahme_stoppen();
            }
        } else if ui.button("Intern aufnehmen").clicked() {
            z.aufnahme_starten();
        }
    });

    if crate::klang::intern_aufnahme_laeuft() {
        let dauer = crate::klang::intern_aufnahme_dauer();
        ui.label(
            egui::RichText::new(format!(
                "Aufnahme läuft: {dauer:.1} s (Ringpuffer 8 s, Mixer-Ausgang)"
            ))
            .small(),
        );
        // Der Klang-Thread kann den Wert nicht selbst neu zeichnen.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }

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

    // Akkord-Presets: dieselben Definitionen wie im Rhythmus-Tab
    // (`crate::akkord::PRESETS`), hier als x-Bereiche plus Klang.
    ui.separator();
    ui.label(
        egui::RichText::new("Akkord-Presets (Klick setzt x-Bereiche und spielt):")
            .small()
            .color(egui::Color32::DARK_GRAY),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label("Grundton");
        let mut grundton = z.akkord_grundton;
        if ui
            .add(
                egui::DragValue::new(&mut grundton)
                    .range(36..=84)
                    .speed(0.5),
            )
            .changed()
        {
            z.akkord_grundton = grundton;
        }
        ui.label(egui::RichText::new(notennamen(&[z.akkord_grundton])).strong());
        for (i, (name, _)) in crate::akkord::PRESETS.iter().enumerate() {
            if ui
                .add(egui::Button::new(*name).small())
                .on_hover_text("Töne als x-Bereiche setzen und anspielen")
                .clicked()
            {
                let grundton = z.akkord_grundton;
                if let Some(titel) = z.akkord_einbauen(i, grundton) {
                    let (bereiche, anzahl) = z
                        .presets
                        .iter()
                        .find(|p| p.herkunft == Herkunft::Akkord && p.name == titel)
                        .map(|p| (p.kreis_bereiche(), p.noten.len()))
                        .unwrap_or_default();
                    if let (Some(erst), Some(letzt)) = (bereiche.first(), bereiche.last()) {
                        kreis.setze_x_bereich(erst.0, letzt.1);
                    }
                    let liste = bereiche_liste(&bereiche);
                    z.status = format!(
                        "kreis: Akkord {name} ({anzahl} Toene) -> x-Bereiche ({liste})"
                    );
                    log::info!("{}", z.status);
                    // Vorhandener Klangweg: eine Stimme je Ton, wie im
                    // Rhythmus-Tab. Die PCM-Version liegt zusätzlich im
                    // Preset für den Editor und „Preset anklicken“.
                    let noten = crate::akkord::noten(i, grundton);
                    for note in noten {
                        crate::klang::spielen_welle(note, Welle::Sinus);
                    }
                }
            }
        }
    });

    // Preset-Liste mit Mini-Editor. `presets`, `editor` und `status` werden
    // getrennt ausgeliehen, damit die Zeile den Editor öffnen darf.
    if z.presets.is_empty() {
        if z.lauf.is_none() {
            ui.label(
                egui::RichText::new(
                    "Noch keine Presets: Audio analysieren, Thai-Motive wählen, \
                     einen Akkord antippen oder intern aufnehmen.",
                )
                .small()
                .color(egui::Color32::DARK_GRAY),
            );
        }
    } else {
        ui.label(
            egui::RichText::new("Presets (Klick setzt x-Bereiche und spielt):")
                .small()
                .color(egui::Color32::DARK_GRAY),
        );
    }

    let Zustand {
        presets,
        editor,
        status,
        ..
    } = &mut *z;
    let mut entfernen: Option<usize> = None;
    let mut oeffnen: Option<usize> = None;
    let mut schliessen = false;
    for i in 0..presets.len() {
        let titel = presets[i].titel();
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(titel)
                .on_hover_text("x-Bereiche setzen und PCM anhören")
                .clicked()
            {
                let bereiche = presets[i].kreis_bereiche();
                if let (Some(erst), Some(letzt)) = (bereiche.first(), bereiche.last()) {
                    kreis.setze_x_bereich(erst.0, letzt.1);
                    let liste = bereiche_liste(&bereiche);
                    *status = format!(
                        "kreis: {} -> {} x-Bereiche ({liste})",
                        presets[i].name,
                        bereiche.len()
                    );
                    log::info!("{}", status.as_str());
                } else {
                    *status = format!("kreis: {} hat keine x-Bereiche", presets[i].name);
                    log::info!("{}", status.as_str());
                }
                if !presets[i].pcm.is_empty() {
                    crate::klang::puffer_spielen(presets[i].pcm_i16(), presets[i].rate, 0.8);
                }
            }
            if ui
                .add(egui::Button::new("Bearbeiten").small())
                .on_hover_text("Start/Ende, Umkehren, Zuschneiden, Löschen, Rückgängig")
                .clicked()
            {
                if editor.as_ref().is_some_and(|e| e.preset == i) {
                    schliessen = true;
                } else {
                    oeffnen = Some(i);
                }
            }
            if ui
                .add(egui::Button::new("×").small())
                .on_hover_text("Preset löschen")
                .clicked()
            {
                entfernen = Some(i);
            }
        });
        if editor.as_ref().is_some_and(|e| e.preset == i) && !schliessen {
            if let Some(mut ed) = editor.take() {
                editor_ui(ui, &mut presets[i], &mut ed, status);
                *editor = Some(ed);
            }
        }
    }
    if let Some(i) = entfernen {
        if i < presets.len() {
            let name = presets.remove(i).name;
            if let Some(e) = editor.as_mut() {
                if e.preset == i {
                    *editor = None;
                } else if e.preset > i {
                    e.preset -= 1;
                }
            }
            *status = format!("kreis: Preset {name} gelöscht");
            log::info!("{}", status.as_str());
        }
    } else if let Some(i) = oeffnen {
        *editor = Some(Editor::neu(i));
    }
    if schliessen {
        *editor = None;
    }
}

/// Mini-Editor: Wellenform mit ziehbaren Start-/Ende-Markern und den
/// Bearbeitungen aus `jamcore::audioedit`. Die x-Bereiche werden mit den
/// passenden `tonfolge::bereiche_*`-Funktionen mitgeführt.
fn editor_ui(ui: &mut egui::Ui, preset: &mut Preset, ed: &mut Editor, status: &mut String) {
    ui.separator();
    ui.label(
        egui::RichText::new("Preset beschneiden (Start/Ende ziehen)")
            .small()
            .strong(),
    );
    if preset.pcm.is_empty() || preset.rate == 0 {
        ui.label(
            egui::RichText::new("Kein PCM vorhanden (leer).")
                .small()
                .color(egui::Color32::DARK_GRAY),
        );
        // Auch nach „alles löschen“ muss der letzte Schritt zurückgehen.
        if ui
            .add_enabled(preset.undo.is_some(), egui::Button::new("Rückgängig").small())
            .clicked()
            && preset.zurueck()
        {
            ed.von = 0.0;
            ed.bis = 1.0;
            *status = format!(
                "kreis: Preset zurückgesetzt ({:.2} s, {} Noten)",
                preset.dauer_s(),
                preset.noten.len()
            );
            log::info!("{}", status.as_str());
        }
        return;
    }
    let n = preset.pcm.len();

    // Werkzeuge zuerst: Auf dem Handy liegen sie unter der Wellenform sonst
    // unter dem Bildschirmrand. Die Marker darunter bleiben zum Ziehen da.
    let von = ((ed.von * n as f32).round() as usize).min(n);
    let bis = ((ed.bis * n as f32).round() as usize).min(n);
    let bereich_ok = bis > von;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(bereich_ok, egui::Button::new("Umkehren").small())
            .clicked()
        {
            preset.schnappschuss();
            jamcore::audioedit::umkehren(&mut preset.pcm, von, bis);
            let aenderungen = tonfolge::bereiche_umkehren(&preset.x_bereiche);
            preset.setze_bereiche(aenderungen);
            *status = format!(
                "kreis: Preset umgekehrt ({:.2} s, {} Noten)",
                preset.dauer_s(),
                preset.noten.len()
            );
            log::info!("{}", status.as_str());
        }
        if ui
            .add_enabled(bereich_ok, egui::Button::new("Zuschneiden").small())
            .clicked()
        {
            preset.schnappschuss();
            jamcore::audioedit::zuschneiden(&mut preset.pcm, von, bis);
            // `basis` bleibt: das Preset sitzt weiter an derselben Stelle im
            // Stück, nur seine inneren x-Bereiche werden auf das neue PCM
            // normiert (0..1).
            let aenderungen =
                tonfolge::bereiche_zuschneiden(&preset.x_bereiche, ed.von, ed.bis);
            preset.setze_bereiche(aenderungen);
            ed.von = 0.0;
            ed.bis = 1.0;
            *status = format!(
                "kreis: Preset zugeschnitten ({:.2} s, {} Noten)",
                preset.dauer_s(),
                preset.noten.len()
            );
            log::info!("{}", status.as_str());
        }
        if ui
            .add_enabled(bereich_ok, egui::Button::new("Bereich löschen").small())
            .clicked()
        {
            preset.schnappschuss();
            jamcore::audioedit::bereich_loeschen(&mut preset.pcm, von, bis);
            let aenderungen = tonfolge::bereiche_loeschen(&preset.x_bereiche, ed.von, ed.bis);
            preset.setze_bereiche(aenderungen);
            ed.von = 0.0;
            ed.bis = 1.0;
            *status = format!(
                "kreis: Preset-Bereich gelöscht ({:.2} s, {} Noten)",
                preset.dauer_s(),
                preset.noten.len()
            );
            log::info!("{}", status.as_str());
        }
        if ui
            .add_enabled(preset.undo.is_some(), egui::Button::new("Rückgängig").small())
            .clicked()
            && preset.zurueck()
        {
            ed.von = 0.0;
            ed.bis = 1.0;
            *status = format!(
                "kreis: Preset zurückgesetzt ({:.2} s, {} Noten)",
                preset.dauer_s(),
                preset.noten.len()
            );
            log::info!("{}", status.as_str());
        }
        if ui
            .add_enabled(bereich_ok, egui::Button::new("Auswahl anhören").small())
            .clicked()
        {
            let pcm = pcm_i16(&preset.pcm[von..bis]);
            crate::klang::puffer_spielen(pcm, preset.rate, 0.8);
        }
    });

    // Nach einer Bearbeitung kann sich die PCM-Länge geändert haben.
    let dauer = preset.dauer_s().max(0.001);
    let n = preset.pcm.len();

    // Wellenform: je Pixel ein Min/Max-Bereich. Der Sprung hält die Arbeit
    // auch bei 8-s-Aufnahmen klein (kein Scan aller Proben je Frame).
    let breite = ui.available_width().max(80.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(breite, 56.0),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(18, 18, 24));
    let schritt = (n / (rect.width() as usize * 2).max(1)).max(1);
    let mitte = rect.center().y;
    let halb = rect.height() * 0.45;
    let farbe = egui::Color32::from_rgb(0, 180, 255);
    let mut i = 0;
    while i < n {
        let ende = (i + schritt).min(n);
        let mut min = 0.0f32;
        let mut max = 0.0f32;
        for &s in &preset.pcm[i..ende] {
            min = min.min(s);
            max = max.max(s);
        }
        let x0 = rect.left() + rect.width() * (i as f32 / n as f32);
        let x1 = rect.left() + rect.width() * (ende as f32 / n as f32);
        painter.line_segment(
            [
                egui::pos2(x0, mitte - max * halb),
                egui::pos2(x1, mitte - max * halb),
            ],
            egui::Stroke::new(1.0, farbe),
        );
        painter.line_segment(
            [
                egui::pos2(x0, mitte - min * halb),
                egui::pos2(x1, mitte - min * halb),
            ],
            egui::Stroke::new(1.0, farbe),
        );
        i = ende;
    }

    let x_von = rect.left() + rect.width() * ed.von;
    let x_bis = rect.left() + rect.width() * ed.bis;
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(x_von, rect.top()),
            egui::pos2(x_bis, rect.bottom()),
        ),
        0.0,
        egui::Color32::from_rgba_unmultiplied(0, 200, 255, 26),
    );
    painter.line_segment(
        [
            egui::pos2(x_von, rect.top()),
            egui::pos2(x_von, rect.bottom()),
        ],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 220, 120)),
    );
    painter.line_segment(
        [
            egui::pos2(x_bis, rect.top()),
            egui::pos2(x_bis, rect.bottom()),
        ],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 140, 0)),
    );

    // Marker über den rohen Zeiger lesen, nicht über `Response::dragged`:
    // Die umgebende Scroll-Fläche greift sich den Drag (egui-Bug/Verhalten),
    // Tippen und Ziehen kommen so trotzdem an. `ed.ziehen` merkt sich, dass
    // der Druck im Rechteck begann — Züge von außen verschieben nichts.
    let (gedrueckt, unten, losgelassen, zeiger) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
        )
    });
    let minimum = 1.0 / n as f32;
    if gedrueckt {
        if let Some(pos) = zeiger {
            if rect.contains(pos) {
                let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let marker = if (f - ed.von).abs() <= (f - ed.bis).abs() {
                    Marker::Start
                } else {
                    Marker::Ende
                };
                ed.ziehen = Some(marker);
                match marker {
                    Marker::Start => ed.von = f.min(ed.bis - minimum).max(0.0),
                    Marker::Ende => ed.bis = f.max(ed.von + minimum).min(1.0),
                }
            }
        }
    }
    if unten {
        if let (Some(pos), Some(marker)) = (zeiger, ed.ziehen) {
            let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            match marker {
                Marker::Start => ed.von = f.min(ed.bis - minimum).max(0.0),
                Marker::Ende => ed.bis = f.max(ed.von + minimum).min(1.0),
            }
        }
    }
    if losgelassen {
        ed.ziehen = None;
    }

    // Start/Ende in einer Zeile: die Marker sind die schnelle Bedienung,
    // die DragValues die letzte Probe. Zusammen spart das eine Zeile, damit
    // die Werkzeugknöpfe auf dem Handy nicht unter den Rand rutschen.
    ui.horizontal_wrapped(|ui| {
        ui.label("Start");
        let mut von_s = ed.von * dauer;
        if ui
            .add(
                egui::DragValue::new(&mut von_s)
                    .range(0.0..=ed.bis * dauer)
                    .speed(0.01)
                    .suffix(" s"),
            )
            .changed()
        {
            ed.von = (von_s / dauer).clamp(0.0, ed.bis);
        }
        ui.label("Ende");
        let mut bis_s = ed.bis * dauer;
        if ui
            .add(
                egui::DragValue::new(&mut bis_s)
                    .range(ed.von * dauer..=dauer)
                    .speed(0.01)
                    .suffix(" s"),
            )
            .changed()
        {
            ed.bis = (bis_s / dauer).clamp(ed.von, 1.0);
        }
        ui.label(
            egui::RichText::new(format!("({:.2} s)", (ed.bis - ed.von) * dauer))
                .small()
                .color(egui::Color32::DARK_GRAY),
        );
    });
    ui.label(
        egui::RichText::new("Marker ziehen: Start (grün) / Ende (orange).")
            .small()
            .color(egui::Color32::DARK_GRAY),
    );
}

/// x-Bereiche als kurze Liste für Status/Log, z. B. „0.00-0.25, 0.25-0.50“.
fn bereiche_liste(bereiche: &[(f32, f32)]) -> String {
    bereiche
        .iter()
        .map(|(von, bis)| format!("{von:.2}-{bis:.2}"))
        .collect::<Vec<_>>()
        .join(", ")
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

/// f32-PCM (−1 … 1) als 16-Bit für `klang::puffer_spielen`.
fn pcm_i16(pcm: &[f32]) -> Vec<i16> {
    pcm.iter()
        .map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect()
}

/// Weiche Hüllkurve (5 ms an, 10 ms aus) gegen Klicks an den Rändern.
fn huelle(i: usize, n: usize, rate: u32) -> f32 {
    if n == 0 || rate == 0 {
        return 0.0;
    }
    let an = (rate as f32 * 0.005) as usize;
    let aus = (rate as f32 * 0.010) as usize;
    let mut h = 1.0f32;
    if an > 0 && i < an {
        h = h.min(i as f32 / an as f32);
    }
    if aus > 0 && i + aus > n {
        h = h.min((n - i) as f32 / aus as f32);
    }
    h.clamp(0.0, 1.0)
}

/// 7-TET-Frequenz aus der MIDI-Näherung zurückrechnen (Thai-Synthese).
fn thai_frequenz(note: i32) -> f32 {
    let stufe =
        ((note - tonfolge::THAI_GRUNDTON) as f32 * 7.0 / 12.0).round() as i32;
    tonfolge::thai_frequenz(stufe, tonfolge::THAI_GRUNDTON)
}

/// Eine Thai-Folge als Sinus-PCM (ein Ton nach dem anderen, 0,25 s je Ton).
fn synth_notenfolge(noten: &[i32], rate: u32, thai: bool) -> Vec<f32> {
    if noten.is_empty() || rate == 0 {
        return Vec::new();
    }
    let pro = (rate as f32 * THAI_TON_S) as usize;
    let mut pcm = Vec::with_capacity(pro * noten.len());
    for &note in noten {
        let freq = if thai {
            thai_frequenz(note)
        } else {
            jamcore::klang::frequenz(note)
        };
        for i in 0..pro {
            let t = i as f32 / rate as f32;
            let h = huelle(i, pro, rate);
            pcm.push((std::f32::consts::TAU * freq * t).sin() * 0.45 * h);
        }
    }
    pcm
}

/// Einen Akkord als Sinus-PCM (alle Töne gleichzeitig, 1,5 s).
fn synth_akkord(noten: &[i32], rate: u32) -> Vec<f32> {
    if noten.is_empty() || rate == 0 {
        return Vec::new();
    }
    let n = (rate as f32 * AKKORD_S) as usize;
    let mut pcm = vec![0.0f32; n];
    let kopf = 1.0 / (noten.len() as f32).sqrt();
    for &note in noten {
        let freq = jamcore::klang::frequenz(note);
        for (i, wert) in pcm.iter_mut().enumerate() {
            let t = i as f32 / rate as f32;
            *wert += (std::f32::consts::TAU * freq * t).sin() * 0.4 * kopf;
        }
    }
    for (i, wert) in pcm.iter_mut().enumerate() {
        *wert *= huelle(i, n, rate);
    }
    pcm
}

/// Datei laden, auf 60 s kürzen und an [`analysiere_proben`] weiterreichen.
fn analysiere_datei(pfad: &str, abbruch: &AtomicBool, fortschritt: &AtomicU32) -> Ergebnis {
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
    analysiere_proben(
        proben,
        rate,
        Herkunft::Datei,
        orig_s,
        gekuerzt,
        abbruch,
        fortschritt,
    )
}

/// Gemeinsame Analyse für Datei und interne Aufnahme: Tonhöhen blockweise,
/// Motive suchen und je Motiv ein Preset mit PCM-Schnitt bauen.
fn analysiere_proben(
    proben: Vec<f32>,
    rate: u32,
    herkunft: Herkunft,
    orig_s: f32,
    gekuerzt: bool,
    abbruch: &AtomicBool,
    fortschritt: &AtomicU32,
) -> Ergebnis {
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
            tonfolge::tonhoehen(&proben[pos..ende], rate)
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
        .map(|(t, f)| (*t, tonfolge::midi(*f)))
        .collect();
    fortschritt.store(85, Ordering::Relaxed);

    let mut motive = tonfolge::finde_motive(&noten, MOTIV_LAENGE);
    // Häufige Motive zuerst: mehr Vorkommen = verlässlicheres Preset.
    motive.sort_by(|a, b| {
        b.starts
            .len()
            .cmp(&a.starts.len())
            .then(a.starts[0].total_cmp(&b.starts[0]))
    });
    motive.truncate(MAX_PRESETS);

    let mut presets = presets_aus_motiven(&motive, &proben, rate, gesamt_s, herkunft);
    // Eine Aufnahme ohne wiederkehrendes Motiv soll trotzdem als Preset
    // landen (z. B. ein einzelner gehaltener Akkord). Der Schnitt ist dann
    // die ganze Aufnahme; die Noten kommen aus der Tonhöhen-Spur.
    if presets.is_empty() && herkunft == Herkunft::Aufnahme {
        presets.push(preset_aus_aufnahme(&proben, rate, gesamt_s));
    }

    fortschritt.store(100, Ordering::Relaxed);
    log::info!(
        "kreis: {} Motiv(e), {} Preset(s) aus {} Noten ({:.1} s, {:?})",
        motive.len(),
        presets.len(),
        noten.len(),
        gesamt_s,
        herkunft
    );
    Ergebnis::Fertig {
        presets,
        gesamt_s,
        noten: noten.len(),
        orig_s,
        gekuerzt,
        herkunft,
    }
}

/// Je Motiv einen PCM-Schnitt (erste Ausführung) und ein Preset bauen.
/// Die x-Bereiche sind lokal gleich breit; `basis` ist die Lage im Stück.
fn presets_aus_motiven(
    motive: &[Motiv],
    proben: &[f32],
    rate: u32,
    gesamt_s: f32,
    herkunft: Herkunft,
) -> Vec<Preset> {
    let mut presets = Vec::new();
    for motiv in motive {
        let start = motiv.starts.first().copied().unwrap_or(0.0).max(0.0);
        let dauer = motiv.dauer_s.max(0.0);
        let von = ((start * rate as f32).round() as usize).min(proben.len());
        let bis = (((start + dauer) * rate as f32).round() as usize).min(proben.len());
        if bis <= von {
            continue;
        }
        let original = tonfolge::x_bereiche(motiv, gesamt_s);
        let basis = match (original.first(), original.last()) {
            (Some(erst), Some(letzt)) if letzt.1 > erst.0 => (erst.0, letzt.1),
            _ => (0.0, 1.0),
        };
        presets.push(Preset {
            name: notennamen(&motiv.noten),
            herkunft,
            noten: motiv.noten.clone(),
            x_bereiche: tonfolge::gleich_bereiche(motiv.noten.len()),
            basis,
            pcm: proben[von..bis].to_vec(),
            rate,
            undo: None,
        });
    }
    presets
}

/// Fallback für eine Aufnahme ohne wiederkehrendes Motiv: die ganze
/// Aufnahme als ein Preset, Noten aus den erkannten Abschnitten.
fn preset_aus_aufnahme(proben: &[f32], rate: u32, gesamt_s: f32) -> Preset {
    let segmente = tonfolge::noten_segmente(proben, rate);
    let noten: Vec<i32> = segmente.iter().map(|(note, _, _)| *note).collect();
    let x_bereiche = tonfolge::x_bereiche_aus_segmenten(&segmente, gesamt_s);
    Preset {
        name: if noten.is_empty() {
            "Aufnahme".to_string()
        } else {
            notennamen(&noten)
        },
        herkunft: Herkunft::Aufnahme,
        noten,
        x_bereiche,
        basis: (0.0, 1.0),
        pcm: proben.to_vec(),
        rate,
        undo: None,
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

    #[test]
    fn preset_uebernimmt_bereiche_und_noten_synchron() {
        let mut preset = Preset {
            name: "Test".to_string(),
            herkunft: Herkunft::Thai,
            noten: vec![60, 63, 67, 72],
            x_bereiche: tonfolge::gleich_bereiche(4),
            basis: (0.0, 1.0),
            pcm: vec![0.0; 400],
            rate: 1000,
            undo: None,
        };
        // Zuschneiden auf die mittleren beiden Noten: Indizes 1 und 2.
        let aenderungen =
            tonfolge::bereiche_zuschneiden(&preset.x_bereiche, 0.25, 0.75);
        preset.setze_bereiche(aenderungen);
        assert_eq!(preset.noten, vec![63, 67]);
        assert_eq!(preset.x_bereiche.len(), 2);
        assert!((preset.x_bereiche[0].0 - 0.0).abs() < 1e-6);
        assert!((preset.x_bereiche[1].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn schnappschuss_stellt_pcm_und_bereiche_wieder_her() {
        let mut preset = Preset {
            name: "Test".to_string(),
            herkunft: Herkunft::Akkord,
            noten: vec![60, 64, 67],
            x_bereiche: tonfolge::gleich_bereiche(3),
            basis: (0.2, 0.8),
            pcm: vec![0.1, 0.2, 0.3],
            rate: 1000,
            undo: None,
        };
        preset.schnappschuss();
        jamcore::audioedit::umkehren(&mut preset.pcm, 0, 3);
        let aenderungen = tonfolge::bereiche_umkehren(&preset.x_bereiche);
        preset.setze_bereiche(aenderungen);
        assert_eq!(preset.noten, vec![67, 64, 60]);
        assert!(preset.zurueck());
        assert_eq!(preset.noten, vec![60, 64, 67]);
        assert_eq!(preset.pcm, vec![0.1, 0.2, 0.3]);
        assert_eq!(preset.basis, (0.2, 0.8));
        assert!(!preset.zurueck());
    }

    #[test]
    fn kreis_bereiche_liegen_in_der_basis() {
        let preset = Preset {
            name: "Test".to_string(),
            herkunft: Herkunft::Datei,
            noten: vec![60, 64],
            x_bereiche: vec![(0.0, 0.5), (0.5, 1.0)],
            basis: (0.2, 0.6),
            pcm: Vec::new(),
            rate: 1000,
            undo: None,
        };
        let bereiche = preset.kreis_bereiche();
        assert_eq!(bereiche.len(), 2);
        assert!((bereiche[0].0 - 0.2).abs() < 1e-6);
        assert!((bereiche[0].1 - 0.4).abs() < 1e-6);
        assert!((bereiche[1].0 - 0.4).abs() < 1e-6);
        assert!((bereiche[1].1 - 0.6).abs() < 1e-6);
    }
}
