//! Maske 2: Aufnahme nach der Vorlage `pattern_recorder` (notes -> notes3 ->
//! notes4).
//!
//! Nutzerentscheidungen 2026-10-23:
//! - Die **0 ist die Pause** (kein Ton, die Schritte zaehlen aber weiter).
//!   Das Ende kommt ausschliesslich vom „Ende“-Knopf oder von Enter — nicht
//!   mehr von der 0 wie im C++-Recorder.
//! - Ein Druck mit Wert `v` gilt von seinem Zeitstempel bis zum naechsten
//!   Druck (bzw. bis zum Ende) und wird dort in **v gleiche Schritte**
//!   zerlegt (notes3). Die Schrittfolge wird danach auf die Taktzeit gelegt
//!   (notes4) und in den gemeinsamen Verlauf von Maske 1 geschrieben.
//! - Zusaetzlich laesst sich die **Zwischenschrittlaenge von/bis** in ms
//!   einstellen. Der Fader waehlt zwischen Min und Max (Startwert: die
//!   Haelfte), die vier Pfeil-Modi (Links/Oben/Unten/Rechts) bestimmen,
//!   welche Randzeitstempel in die Rechnung eingehen.
//!
//! Der Verlauf liegt im gemeinsamen Speicher von [`crate::maske`]; Maske 1
//! zeigt ihn (gleiche Datenquelle, keine Kopie), Pausen als Luecken.

use std::time::Instant;

use eframe::egui::{self, Color32, RichText};
use jamcore::grid::Pattern;
use jamcore::maske::{AufnahmeNote, AufnahmeOptionen, RandModus};
use log::info;

use crate::maske::{self, Aufzeichnung};
use crate::tasten::{tasten, Taste};

/// Anzahl der Tasten (Nummernblock: 1-9 und 0).
pub const TASTEN: usize = 10;

/// Zellenzahl je Reihe im Feld (3x3 plus die Null darunter).
const SPALTEN: usize = 3;

pub struct Maske2Panel {
    /// Wert je Position; die Positionen liegen in Nummernblock-Anordnung,
    /// die Vorgabewerte sind 7,8,9 / 4,5,6 / 1,2,3 / 0.
    werte: [i32; TASTEN],
    /// Rohaufnahme (notes): (Wert, Zeitpunkt) in Druckreihenfolge. Die
    /// Zeitpunkte sind absolute `Instant`s; relativ gerechnet wird erst beim
    /// Auswerten, damit ein spaeterer Bezugspunkt die Vergangenheit nicht
    /// verschiebt.
    aufnahme: Vec<(i32, Instant)>,
    /// Erster Druck = Nullpunkt der Aufnahme.
    start: Option<Instant>,
    /// „Ende“ gedrueckt: der Endzeitpunkt steht fest.
    ende: Option<Instant>,
    /// notes3 (gleiche Zwischenschritte) fuer Anzeige und Log.
    notes3: Vec<AufnahmeNote>,
    /// notes4/Verlauf: (Schrittnummer, Wert), Pausen als Luecken.
    verlauf: Vec<(u32, i32)>,
    /// Zeitpunkte fuer den Bild-Scan (ein Punkt je klingendem Schritt).
    folge: Vec<Aufzeichnung>,
    /// Einstellungen der Auswertung (von/bis, Randmodus, Fader, Rampe).
    optionen: AufnahmeOptionen,
    /// Gemessenes Min/Max der Rand-Schrittlaengen in ms.
    min_max: Option<(f64, f64)>,
    /// Solange true, folgen die von/bis-Felder dem gemessenen Min/Max.
    /// Sobald der Nutzer sie anfasst, bleiben seine Werte stehen.
    grenzen_auto: bool,
    /// „Ende“ gedrueckt: die Aufnahme steht, bis „neu“ kommt.
    fest: bool,
    /// Nach einem Pfeiltasten-Modus den Fokus am Frame-Ende zuruecksetzen
    /// (egui verschiebt ihn sonst in die Tab-Leiste).
    fokus_zurueck: Option<Option<egui::Id>>,
    status: String,
    /// Wann zuletzt etwas aufgenommen wurde — die zuletzt benutzte Maske
    /// speist den gruenen Balken im Bild-Tab.
    letzte_aenderung: Option<Instant>,
}

impl Default for Maske2Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Maske2Panel {
    pub fn new() -> Self {
        Self {
            werte: [7, 8, 9, 4, 5, 6, 1, 2, 3, 0],
            aufnahme: Vec::new(),
            start: None,
            ende: None,
            notes3: Vec::new(),
            verlauf: Vec::new(),
            folge: Vec::new(),
            optionen: AufnahmeOptionen::default(),
            min_max: None,
            grenzen_auto: true,
            fest: false,
            fokus_zurueck: None,
            status: "Tasten tippen: Wert = gleiche Schritte, 0 = Pause, Ende = Enter.".to_string(),
            letzte_aenderung: None,
        }
    }

    /// Die aufgezeichnete Folge (Zeitpunkte) fuer den Bild-Scan.
    pub fn folge(&self) -> &[Aufzeichnung] {
        &self.folge
    }

    /// Zeitpunkte fuer den Bild-Scan: ein Punkt je klingendem Schritt, an
    /// seiner Stelle im Gesamtverlauf (Pausen bleiben Luecken).
    ///
    /// Waehrend der Aufnahme zaehlt der Entwurf; danach der festgehaltene
    /// Verlauf (der bei einer Bereichsaufnahme ein anderer ist als der
    /// Entwurf). Nach „neu“ bleibt die gruene Maske stehen, statt zu
    /// verschwinden.
    pub fn zeiten(&self) -> Vec<f32> {
        if !self.fest && !self.verlauf.is_empty() {
            maske::verlauf_zeiten(&self.verlauf)
        } else {
            maske::verlauf_zeiten(&maske::verlauf_snapshot().schritte)
        }
    }

    pub fn letzte_aenderung(&self) -> Option<Instant> {
        self.letzte_aenderung
    }

    /// Rohaufnahme mit relativen Sekunden (notes).
    fn rohzeiten(&self) -> Vec<(i32, f64)> {
        let Some(start) = self.start else {
            return Vec::new();
        };
        self.aufnahme
            .iter()
            .map(|(wert, zeit)| (*wert, zeit.duration_since(start).as_secs_f64()))
            .collect()
    }

    /// Ende der Aufnahme in Sekunden. Laeuft die Aufnahme noch, gilt der
    /// letzte Druck als vorlaeufiges Ende — sonst wuerde der letzte Block
    /// mit jedem Frame weiterwachsen und die Vorschau flackern.
    fn ende_s(&self) -> f64 {
        let Some(start) = self.start else {
            return 0.0;
        };
        let ende = self
            .ende
            .unwrap_or_else(|| self.aufnahme.last().map(|(_, zeit)| *zeit).unwrap_or(start));
        ende.duration_since(start).as_secs_f64().max(0.0)
    }

    /// Ende des letzten **klingenden** Blocks. Eine nachlaufende Pause (0)
    /// bis zum „Ende“ gehoert nicht dazu — sonst wuerde die letzte
    /// Schrittlaenge auf die Wartezeit verkuerzt.
    fn sounding_ende_s(&self) -> f64 {
        let Some(start) = self.start else {
            return 0.0;
        };
        let mut ende = 0.0f64;
        for (i, (wert, _)) in self.aufnahme.iter().enumerate() {
            if *wert <= 0 {
                continue;
            }
            let block_ende = self
                .aufnahme
                .get(i + 1)
                .map(|(_, zeit)| *zeit)
                .or(self.ende)
                .unwrap_or_else(|| self.aufnahme.last().map(|(_, zeit)| *zeit).unwrap_or(start));
            ende = ende.max(block_ende.duration_since(start).as_secs_f64());
        }
        ende.min(self.ende_s())
    }

    /// Das gemessene Min/Max der Rand-Schrittlaengen neu bestimmen und —
    /// solange der Nutzer die Felder nicht angefasst hat — in von/bis
    /// uebernehmen.
    fn grenzen_aktualisieren(&mut self) {
        let werte = self.rohzeiten();
        let ende = self.ende_s();
        let laengen = jamcore::maske::schrittlaengen_aus_aufnahme(&werte, ende, &self.optionen);
        let links = jamcore::maske::rand_schritt_ms_aus_laengen(&laengen, RandModus::Links);
        let rechts = jamcore::maske::rand_schritt_ms_aus_laengen(&laengen, RandModus::Rechts);
        let mitte = jamcore::maske::rand_schritt_ms_aus_laengen(&laengen, RandModus::Unten);
        let (min, max) = match self.optionen.rand_modus {
            RandModus::Links => links.unwrap_or((0.0, 0.0)),
            RandModus::Rechts => rechts.unwrap_or((0.0, 0.0)),
            RandModus::Oben => match (links, rechts) {
                (Some(l), Some(r)) => (l.0.min(r.0), l.1.max(r.1)),
                (Some(l), None) => l,
                (None, Some(r)) => r,
                (None, None) => (0.0, 0.0),
            },
            RandModus::Unten => mitte.unwrap_or((0.0, 0.0)),
        };
        if max > 0.0 {
            self.min_max = Some((min, max));
        }
        if !self.grenzen_auto {
            return;
        }
        match self.optionen.rand_modus {
            RandModus::Links => {
                self.optionen.step_min_ms = min;
                self.optionen.step_max_ms = max;
            }
            RandModus::Rechts => {
                self.optionen.step_min_ms = min;
                self.optionen.step_max_ms = max;
            }
            RandModus::Oben => {
                // „von“ ist der linke Rand, „bis“ der rechte; die Reihenfolge
                // darf sich unterscheiden (750 -> 375 im Nutzerbeispiel).
                if let (Some(l), Some(r)) = (links, rechts) {
                    self.optionen.step_min_ms = l.0;
                    self.optionen.step_max_ms = r.0;
                }
            }
            RandModus::Unten => {
                self.optionen.step_min_ms = min;
                self.optionen.step_max_ms = max;
            }
        }
    }

    /// „neu“: die laufende Aufnahme verwerfen und wieder aufnehmen. Der
    /// festgehaltene Verlauf bleibt stehen — sonst waere beim Aufnehmen
    /// eines Bereichs der Inhalt ausserhalb sofort weg.
    pub fn neu(&mut self) {
        self.aufnahme.clear();
        self.start = None;
        self.ende = None;
        self.notes3.clear();
        self.verlauf.clear();
        self.folge.clear();
        self.min_max = None;
        self.fest = false;
        self.letzte_aenderung = Some(Instant::now());
        if let Ok(mut z) = maske::verlauf_zustand().lock() {
            z.entwurf = maske::Entwurf::default();
        }
        let bereich = maske::verlauf_snapshot().bereich;
        self.status = match bereich {
            Some((von, bis)) => format!(
                "neu — Bereich {von}-{bis} ({} Schritte), Rand {}",
                bis - von,
                self.optionen.rand_modus.name()
            ),
            None => format!(
                "neu — Rand {}: Tasten tippen",
                self.optionen.rand_modus.name()
            ),
        };
        info!("Maske 2: neu (Rand {})", self.optionen.rand_modus.name());
    }

    /// „Ende“: festhalten und die Aufnahme stoppen, bis „neu“ kommt.
    ///
    /// Ohne Bereich in Maske 1 wird der ganze Verlauf ersetzt. Mit Bereich
    /// werden die Blockdauern und Pausen proportional auf die Bereichszeit
    /// gemappt; ausserhalb bleibt der alte Verlauf stehen.
    pub fn ende(&mut self) {
        if self.aufnahme.is_empty() {
            self.fest = true;
            self.status = "Ende — keine Schritte aufgenommen".to_string();
            info!("Maske 2: Ende ohne Schritte");
            return;
        }
        self.ende = Some(Instant::now());
        self.fest = true;
        self.grenzen_aktualisieren();
        self.nachziehen();

        let werte = self.rohzeiten();
        let ende = self.ende_s();
        let (dauern, pausen) = jamcore::maske::aufnahme_blockstruktur(&werte, ende, &self.optionen);
        let block_summe: u32 = dauern.iter().map(|(_, d)| *d).sum();
        let pausen_summe: u32 = pausen.iter().sum();
        let gesamt = block_summe + pausen_summe;
        info!("maske2: Verlauf {gesamt} Schritte (Pausen {pausen_summe} + Bloecke {block_summe})");
        info!(
            "maske2: notes3 {} Zeitpunkte, notes4 {} Schritte",
            self.notes3.len(),
            jamcore::maske::verlauf_laenge(&self.verlauf)
        );

        let mut z = match maske::verlauf_zustand().lock() {
            Ok(z) => z,
            Err(_) => return,
        };
        z.entwurf = maske::Entwurf::default();
        match z.bereich.take() {
            Some((von, bis)) => {
                let laenge = jamcore::maske::verlauf_laenge(&z.schritte);
                let von = von.min(laenge);
                let bis = bis.min(laenge);
                let ersetzt = if pausen_summe > 0 {
                    jamcore::maske::bereich_ersetzen_mit_pausen(
                        &mut z.schritte,
                        von,
                        bis,
                        &dauern,
                        &pausen,
                    )
                } else {
                    jamcore::maske::bereich_ersetzen(&mut z.schritte, von, bis, &dauern)
                };
                match ersetzt {
                    Some(meldung) => {
                        info!(
                            "maske: Bereich {von}-{bis} neu aufgenommen ({} Schritte aus {})",
                            meldung.schritte, meldung.dauer
                        );
                        info!("maske: Verlauf {} Schritte", meldung.gesamt);
                        self.status = format!(
                            "Bereich {von}-{bis}: {} Schritte aus {} — Verlauf {} Schritte",
                            meldung.schritte, meldung.dauer, meldung.gesamt
                        );
                    }
                    None => {
                        // Leerer/ungueltiger Bereich: nichts loeschen.
                        info!("maske: Bereich {von}-{bis} ohne neue Schritte — unveraendert");
                        self.status =
                            format!("Bereich {von}-{bis}: keine Schritte — Verlauf unveraendert");
                    }
                }
            }
            None => {
                z.schritte = self.verlauf.clone();
                info!("maske: Verlauf {gesamt} Schritte");
                self.status = format!("{gesamt} Schritte festgehalten — „neu“ nimmt wieder auf");
            }
        }
        self.letzte_aenderung = Some(Instant::now());
        info!(
            "Maske 2: Ende ({} Tasten, {gesamt} Schritte)",
            self.aufnahme.len()
        );
    }

    /// Eine Taste des Feldes anschlagen: (Wert, Zeitpunkt) aufnehmen.
    /// Auch die 0 wird aufgenommen — sie ist die Pause.
    fn anschlagen(&mut self, position: usize) {
        if self.fest || position >= TASTEN {
            return;
        }
        let wert = self.werte[position].max(0);
        let jetzt = Instant::now();
        if self.start.is_none() {
            self.start = Some(jetzt);
        }
        self.aufnahme.push((wert, jetzt));
        self.grenzen_aktualisieren();
        self.nachziehen();
        self.letzte_aenderung = Some(jetzt);
        let t = self
            .start
            .map(|start| jetzt.duration_since(start).as_secs_f64())
            .unwrap_or(0.0);
        info!("maske2: taste {} wert {wert} bei {t:.3} s", position + 1);
        let gesamt = jamcore::maske::verlauf_laenge(&self.verlauf);
        self.status = if wert == 0 {
            format!("{gesamt} Schritte — 0 = Pause, Ende hält fest")
        } else {
            format!("{gesamt} Schritte — Ende hält fest")
        };
    }

    /// notes3/notes4 aus der aktuellen Rohaufnahme bauen und in die
    /// gemeinsame Quelle schreiben.
    fn nachziehen(&mut self) {
        let werte = self.rohzeiten();
        let ende = self.ende_s();
        self.notes3 = jamcore::maske::zwischenschritte_notes3(&werte, ende, &self.optionen);
        self.verlauf =
            jamcore::maske::auf_taktzeit_mappen_notes4(&self.notes3, ende, &self.optionen);
        let (dauern, pausen) = jamcore::maske::aufnahme_blockstruktur(&werte, ende, &self.optionen);
        let zeiten = maske::verlauf_zeiten(&self.verlauf);
        self.folge = self
            .verlauf
            .iter()
            .zip(zeiten)
            .map(|((_, wert), zeit)| Aufzeichnung {
                zeit,
                indexnum: *wert as f32,
                tempo: Vec::new(),
            })
            .collect();
        if let Ok(mut z) = maske::verlauf_zustand().lock() {
            z.entwurf = maske::Entwurf { dauern, pausen };
            // Nach „Ende“ ist der Verlauf festgeschrieben; ein Optionswechsel
            // zeigt dann nur die Alternative im Entwurf, statt eine bereits
            // auf einen Bereich gemappte Aufnahme wieder zu ueberschreiben.
            // Auch ein leerer Entwurf (direkt nach „neu“) darf den alten
            // Verlauf nicht loeschen.
            if z.bereich.is_none() && !self.fest && !self.aufnahme.is_empty() {
                z.schritte = self.verlauf.clone();
            }
        }
    }

    /// Die Anzeige-Zeile „11 Schritte (2+4+5) · Pausen — (0) · gesamt 11“.
    fn zusammenfassung(&self) -> String {
        let werte = self.rohzeiten();
        let ende = self.ende_s();
        let (dauern, pausen) = jamcore::maske::aufnahme_blockstruktur(&werte, ende, &self.optionen);
        let block_summe: u32 = dauern.iter().map(|(_, d)| *d).sum();
        let pausen_summe: u32 = pausen.iter().sum();
        let dauer_text = if dauern.is_empty() {
            "—".to_string()
        } else {
            dauern
                .iter()
                .map(|(_, d)| d.to_string())
                .collect::<Vec<_>>()
                .join("+")
        };
        let pausen_text = if pausen_summe == 0 {
            "—".to_string()
        } else {
            pausen
                .iter()
                .filter(|p| **p > 0)
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("+")
        };
        format!(
            "{block_summe} Schritte ({dauer_text}) · Pausen {pausen_text} \
             ({pausen_summe}) · gesamt {}",
            block_summe + pausen_summe
        )
    }

    /// Zelle -> Position im Feld: 3x3 fuer 1..9, die Null allein darunter.
    fn position_der_zelle(reihe: usize, spalte: usize) -> Option<usize> {
        if reihe < 3 {
            Some(reihe * SPALTEN + spalte)
        } else if spalte == 1 {
            Some(9)
        } else {
            None
        }
    }

    /// Der Verlauf als Blockleiste: ein Platz je Gesamtschritt, Pausen
    /// bleiben dunkle Luecken.
    fn verlauf_streifen(&self, ui: &mut egui::Ui, breite: f32) {
        let (leiste, painter) = ui.allocate_painter(egui::vec2(breite, 40.0), egui::Sense::hover());
        let rect = leiste.rect;
        painter.rect_filled(rect, 2.0, Color32::from_rgb(40, 42, 50));
        let n = jamcore::maske::verlauf_laenge(&self.verlauf);
        if n == 0 {
            return;
        }
        let block = rect.width() / n as f32;
        for (schritt, wert) in self.verlauf.iter() {
            let r = egui::Rect::from_min_size(
                egui::pos2(rect.left() + *schritt as f32 * block, rect.top()),
                egui::vec2((block - 0.5).max(1.0), rect.height()),
            );
            painter.rect_filled(r, 0.0, maske::verlauf_farbe(*wert));
        }
    }

    /// Die vier Pfeiltasten der Tastatur schalten denselben Randmodus wie die
    /// kleinen Knoepfe — wie `K`+Pfeile in der Vorlage. Das ist auf dem
    /// Emulator (kein Touch) und mit USB-Tastatur der schnellste Weg.
    fn pfeiltasten(&mut self, ui: &egui::Ui) {
        // egui verschiebt bei Pfeiltasten auch den Fokus; den vorherigen
        // Fokus danach zuruecksetzen, damit der Fokusring nicht in die
        // Tab-Leiste springt.
        let vorher = ui.ctx().memory(|m| m.focused());
        let (links, oben, unten, rechts) = ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        let modus = if links {
            Some(RandModus::Links)
        } else if oben {
            Some(RandModus::Oben)
        } else if unten {
            Some(RandModus::Unten)
        } else if rechts {
            Some(RandModus::Rechts)
        } else {
            None
        };
        if let Some(modus) = modus {
            self.optionen.rand_modus = modus;
            self.grenzen_auto = true;
            self.grenzen_aktualisieren();
            self.nachziehen();
            let min_max = self
                .min_max
                .map(|(min, max)| format!("{min:.0}/{max:.0} ms"))
                .unwrap_or_else(|| "—".to_string());
            self.status = format!("Rand {} — Min/Max {min_max}", modus.name());
            info!("maske2: Rand {} (Pfeiltaste)", modus.name());
            // Erst am Frame-Ende zuruecksetzen: egui setzt den Fokus beim
            // Zeichnen der Tab-Leiste, also nach dieser Funktion.
            self.fokus_zurueck = Some(vorher);
        }
    }

    /// Den Fokus nach einem Pfeiltasten-Modus wiederherstellen bzw. leeren.
    fn fokus_zurueck_anwenden(&mut self, ui: &egui::Ui) {
        let Some(vorher) = self.fokus_zurueck.take() else {
            return;
        };
        if let Some(id) = vorher {
            ui.ctx().memory_mut(|m| m.request_focus(id));
        } else if let Some(neu) = ui.ctx().memory(|m| m.focused()) {
            ui.ctx().memory_mut(|m| m.surrender_focus(neu));
        }
    }

    /// Die Einstellungen fuer die Auswertung: Randmodus, von/bis, Fader,
    /// Rampe und die Anzeige des wirksamen Bereichs.
    fn einstellungen(&mut self, ui: &mut egui::Ui) {
        // Die vier Pfeil-Modi stehen **vor** dem „-“ der von/bis-Zeile,
        // wie in der Vorlage (K + Pfeiltasten).
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Rand").strong());
            let mut modus_geaendert = false;
            for modus in [
                RandModus::Links,
                RandModus::Oben,
                RandModus::Unten,
                RandModus::Rechts,
            ] {
                let knopf = egui::Button::new(RichText::new(modus.name()).size(12.0))
                    .min_size(egui::vec2(48.0, 28.0))
                    .selected(self.optionen.rand_modus == modus);
                if ui.add(knopf).clicked() {
                    self.optionen.rand_modus = modus;
                    modus_geaendert = true;
                }
            }

            let mut grenzen_geaendert = false;
            ui.label("von");
            grenzen_geaendert |= ui
                .add(
                    egui::DragValue::new(&mut self.optionen.step_min_ms)
                        .range(0.0..=60_000.0)
                        .speed(1.0)
                        .suffix(" ms"),
                )
                .changed();
            ui.label("-");
            ui.label("bis");
            grenzen_geaendert |= ui
                .add(
                    egui::DragValue::new(&mut self.optionen.step_max_ms)
                        .range(0.0..=60_000.0)
                        .speed(1.0)
                        .suffix(" ms"),
                )
                .changed();
            ui.label("1/");
            let mut teiler = (1.0 / self.optionen.rand_anteil).round().max(1.0) as u32;
            if ui
                .add(egui::DragValue::new(&mut teiler).range(1..=32).speed(1.0))
                .changed()
            {
                self.optionen.rand_anteil = 1.0 / teiler as f64;
                modus_geaendert = true;
            }
            if modus_geaendert {
                // Ein Moduswechsel ist eine neue Vorgabe: die gemessenen
                // Grenzen duerfen die Felder wieder fuellen.
                self.grenzen_auto = true;
                self.grenzen_aktualisieren();
                self.nachziehen();
            } else if grenzen_geaendert {
                // Der Nutzer hat die Grenzen selbst gesetzt: nicht mehr
                // automatisch ueberschreiben.
                self.grenzen_auto = false;
                self.nachziehen();
            }
        });

        ui.horizontal_wrapped(|ui| {
            if let Some((min, max)) = self.min_max {
                ui.label(
                    RichText::new(format!("Min/Max: {min:.0} / {max:.0} ms"))
                        .small()
                        .color(Color32::from_rgb(150, 200, 255)),
                );
            } else {
                ui.label(
                    RichText::new("Min/Max: noch keine zwei Zeitstempel")
                        .small()
                        .color(Color32::DARK_GRAY),
                );
            }
            ui.label("Fader");
            let mut f = if self.optionen.haelfte_aus_minmax {
                0.5
            } else {
                self.optionen.fader
            };
            if ui
                .add(egui::Slider::new(&mut f, 0.0..=1.0).show_value(false))
                .changed()
            {
                self.optionen.haelfte_aus_minmax = false;
                self.optionen.fader = f;
                self.nachziehen();
            }
            if ui
                .add(egui::Button::new("1/2").min_size(egui::vec2(36.0, 24.0)))
                .on_hover_text("Hälfte aus Min/Max als Startwert")
                .clicked()
            {
                self.optionen.haelfte_aus_minmax = true;
                self.optionen.fader = 0.5;
                self.nachziehen();
            }
            let eff = jamcore::maske::fader_schritt_ms(
                self.optionen.step_min_ms,
                self.optionen.step_max_ms,
                &self.optionen,
            );
            ui.label(RichText::new(format!("{eff:.0} ms")).small());
            if ui
                .checkbox(&mut self.optionen.rampe, "Rampe")
                .on_hover_text("Schrittlängen von „von“ nach „bis“ statt gleich")
                .changed()
            {
                self.nachziehen();
            }
        });

        // Der wirksame Randbereich: die letzten 1/8 (bzw. 1/Teiler) der
        // Aufnahme, aus denen die rechten Zeitstempel kommen.
        let (zone_von, zone_bis) =
            jamcore::maske::randzone(self.sounding_ende_s(), self.optionen.rand_anteil);
        let teiler = (1.0 / self.optionen.rand_anteil).round().max(1.0) as u32;
        ui.label(
            RichText::new(format!(
                "wirksam: {zone_von:.3}–{zone_bis:.3} s (letzte 1/{teiler}, Modus {})",
                self.optionen.rand_modus.name()
            ))
            .small()
            .color(Color32::from_rgb(160, 170, 185)),
        );
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _pattern: &mut Pattern) {
        ui.heading("Takt-Maske 2");
        self.pfeiltasten(ui);
        // Enter ist immer „Ende“ — auch wenn ein Knopf den Fokus hat. Sonst
        // wuerde Enter z. B. „neu“ ausloesen und die Aufnahme loeschen,
        // bevor `ende()` sie sieht.
        let enter = ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if enter && !self.fest {
            self.ende();
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(egui::Button::new("neu").min_size(egui::vec2(96.0, 44.0)))
                .clicked()
            {
                self.neu();
            }
            if ui
                .add(egui::Button::new("Ende").min_size(egui::vec2(96.0, 44.0)))
                .clicked()
            {
                self.ende();
            }
            ui.label(RichText::new(&self.status).color(if self.fest {
                Color32::GRAY
            } else {
                Color32::from_rgb(120, 230, 160)
            }));
        });

        // Der in Maske 1 gewaehlte Bereich gilt fuer die naechste Aufnahme.
        if let Some((von, bis)) = maske::verlauf_snapshot().bereich {
            ui.label(
                RichText::new(format!(
                    "Bereich {von}-{bis} ({} Schritte) aus Maske 1 — „Ende“ mappt die neue \
                     Gesamtzeit (Bloecke + Pausen) darauf.",
                    bis - von
                ))
                .small()
                .color(Color32::from_rgb(150, 200, 255)),
            );
        }

        self.einstellungen(ui);

        // Nummernblock: Ziffer trifft den raeumlich gleichen Knopf,
        // Enter ist „Ende“.
        for taste in tasten(ui) {
            match taste {
                Taste::Ziffer(ziffer) => {
                    if let Some(position) = jamcore::maske::position_numpad(ziffer) {
                        self.anschlagen(position);
                    }
                }
                Taste::Enter => self.ende(),
            }
        }

        // Das Feld: grosse Knoepfe, damit sie mit dem Finger sicher treffen.
        let breite = ui.available_width();
        let seite = ((breite - 2.0 * 8.0) / SPALTEN as f32).min(110.0);
        egui::Grid::new("maske2_feld")
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for reihe in 0..4 {
                    for spalte in 0..SPALTEN {
                        match Self::position_der_zelle(reihe, spalte) {
                            Some(position) => {
                                let knopf = ui.add_sized(
                                    egui::vec2(seite, seite),
                                    egui::Button::new(
                                        RichText::new(format!("{}", self.werte[position]))
                                            .size(24.0),
                                    ),
                                );
                                if knopf.clicked() {
                                    self.anschlagen(position);
                                }
                            }
                            None => {
                                ui.allocate_space(egui::vec2(seite, seite));
                            }
                        }
                    }
                    ui.end_row();
                }
            });

        // Der Verlauf: Gesamtschritte inklusive Pausen-Luecken.
        let gesamt = jamcore::maske::verlauf_laenge(&self.verlauf);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Verlauf").strong());
            ui.label(format!(
                "{gesamt} Schritte aus {} Tasten",
                self.aufnahme.len()
            ));
        });
        self.verlauf_streifen(ui, breite);
        if !self.aufnahme.is_empty() {
            ui.label(
                RichText::new(self.zusammenfassung())
                    .small()
                    .color(Color32::from_rgb(200, 210, 225)),
            );
        }

        // Werte der Tasten: Vorgabe sind die Ziffern, aenderbar.
        egui::CollapsingHeader::new("Dauern der Tasten")
            .id_salt("maske2_werte")
            .show(ui, |ui| {
                egui::Grid::new("maske2_werte_gitter")
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        for reihe in 0..4 {
                            for spalte in 0..SPALTEN {
                                match Self::position_der_zelle(reihe, spalte) {
                                    Some(position) => {
                                        let mut wert = self.werte[position];
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut wert)
                                                    .range(0..=127)
                                                    .speed(1.0),
                                            )
                                            .changed()
                                        {
                                            self.werte[position] = wert;
                                        }
                                    }
                                    None => {
                                        ui.label("");
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.label(
                    RichText::new(
                        "Der Feldwert ist die Zahl gleicher Schritte: 2 bedeutet zwei \
                         Schritte bis zum naechsten Druck, 0 ist die Pause (kein Ton, \
                         die Schrittnummer zaehlt weiter). Das Ende kommt von „Ende“ \
                         oder Enter.",
                    )
                    .small()
                    .color(Color32::DARK_GRAY),
                );
            });

        ui.label(
            RichText::new(
                "Nummernblock: Ziffern treffen die Tasten wie auf dem Block \
                 (7-8-9 / 4-5-6 / 1-2-3 / 0), Enter ist „Ende“. Die Pfeil-Modi \
                 Links/Oben/Unten/Rechts waehlen die Randzeitstempel fuer Min/Max; \
                 der Fader steht in der Mitte, die Rampe laeuft von „von“ nach „bis“.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
        // Nach allen Widgets: den Fokus ggf. von der Tab-Leiste zurueckholen.
        self.fokus_zurueck_anwenden(ui);
    }
}
