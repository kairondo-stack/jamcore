//! Takt-Maske: Tastenfeld, jeder Tastendruck ein Zeitabstand.
//!
//! Der Tab macht **nur Zeitabstaende**. Was hier aufgezeichnet wird, geht an den
//! Bild-Scan und wird wie `KeyEventRender.txt` auf die X-Aufloesung des Bilds
//! gemappt (gruener Balken ueber dem Bild). Noten kommen aus dem Rhythmus-Tab.
//!
//! Getippt wird mit dem Finger: Aufsetzen = erster Zeitpunkt, Loslassen = der
//! letzte. Ein Wisch ueber mehrere Felder zaehlt beide Felder als zwei
//! Zeitpunkte mit dem Abstand dazwischen; die Felder dazwischen sind nur
//! Durchgang.
//!
//! Der Feldwert ist das **Indexnum** (Unterteilungen je Schritt, wie im
//! pattern_recorder): die Schrittbreite des Abschnitts ist `1/Indexnum`, also
//! ein kleines Feld = breiter Schritt. Zwischen zwei Feldern mit
//! unterschiedlichem Wert entsteht daraus eine Rampe (Kern
//! `schrittweite::timestamps`); die Wischgeschwindigkeit moduliert deren
//! Krümmung.

use eframe::egui::{self, Color32, RichText};
use jamcore::grid::Pattern;
use log::info;

use crate::touch::Mehrfinger;

/// Ab dieser Pause zwischen zwei Felddruecken beginnt die Aufnahme neu. Die
/// Zahl steht im Kern (`schrittweite::NEU_AFTER_SEKUNDEN`), weil die Regel
/// `schrittweite::beginnt_aufnahme` dort getestet wird.
pub use jamcore::schrittweite::NEU_AFTER_SEKUNDEN;

/// Unter- und Obergrenze des Zoom vom Tastenfeld (Prozent, wie im Bild-Tab).
pub const ZOOM_MIN: f32 = 0.2;
/// Siehe [`ZOOM_MIN`].
pub const ZOOM_MAX: f32 = 4.0;

/// Regenbogenfarbe fuer den Touch-Verlauf (0.0 = rot, 1.0 = violett).
fn regenbogen(anteil: f32) -> Color32 {
    egui::ecolor::Hsva::new(anteil.clamp(0.0, 1.0) * 0.8, 0.85, 0.95, 1.0).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feldart {
    /// Eine Taste je Wert (gut zum gezielten Anschlagen).
    JeWert,
    /// Festes Feld, Werte gleichmäßig verteilt (kompakt).
    Verteilt,
}

/// Ein aufgezeichneter Feldruck: der Feldwert als Indexnum, der Zeitpunkt
/// relativ zum ersten Druck und das Geschwindigkeitsprofil des Wischs (Wege je
/// Frame, leer bei einem blossen Tipp).
#[derive(Clone, Debug, PartialEq)]
pub struct Aufzeichnung {
    pub zeit: f32,
    pub indexnum: f32,
    pub tempo: Vec<f32>,
}

impl Aufzeichnung {
    /// Als Kern-`Tipp` fuer [`jamcore::schrittweite::timestamps`].
    pub fn als_tipp(&self) -> jamcore::schrittweite::Tipp {
        jamcore::schrittweite::Tipp::new(self.zeit, self.indexnum)
    }
}

pub struct MaskePanel {
    feldart: Feldart,
    spalten: usize,
    reihen: usize,
    wert_max: i32,
    status: String,
    /// Getastete Felder in Druckreihenfolge (Index und Mitte) fuer den Regenbogen.
    spur: Vec<(usize, egui::Pos2)>,
    /// Mehrfinger: jeder neu aufgesetzte Finger ist ein weiterer Zeitpunkt.
    finger: Mehrfinger,
    /// Erst- und Endtaste je Finger (Fingernummer -> (Start, Ende)).
    wisch: std::collections::HashMap<u64, (Option<usize>, Option<usize>)>,
    /// Die aufgezeichnete Folge (Zeitabstaende) — das, was der Bild-Scan mapped.
    folge: Vec<Aufzeichnung>,
    /// Die **aufgeklappte** Folge: die Handdruecke plus die Zwischenschritte
    /// aus den Schrittweiten-Abschnitten. Das ist das, was der Desktop am Ende
    /// als `notes4` hat — nicht nur die getippten Toene. Ohne sie zeigte die
    /// Maske „Zeitpunkte: 0“, waehrend die Zwischenschritte nur beim Umrechnen
    /// auf die Bildspalen kurz auftauchten und wieder verschwanden.
    zwischenschritte: Vec<f32>,
    /// Regel je Abschnitt (min/max Schritte aus der Zeitleiste); ohne Eintrag
    /// gelten die Feldwerte der beiden Tipps.
    regeln: Vec<Option<jamcore::schrittweite::Abschnitt>>,
    /// In der Zeitleiste angetippter Abschnitt (Index des linken Tipps).
    auswahl: Option<usize>,
    /// Nach „Ende" ist die Folge festgehalten: ein weiterer Felddruck darf sie
    /// nicht wegschmeissen. Erst „neu" (oder „leeren") nimmt wieder auf.
    fest: bool,
    /// Zeitpunkt des ersten Drucks als Anker (wie der C++-Recorder: der Timer
    /// startet beim ersten Tastendruck, alles weitere ist relativ dazu).
    anker: Option<std::time::Instant>,
    /// Wischweg je Frame je Finger, fuer die Krümmungs-Modulation.
    wege: std::collections::HashMap<u64, Vec<f32>>,
    /// Letzte Position und Zeit je Finger, um den Weg je Frame zu messen.
    spurpunkt: std::collections::HashMap<u64, (egui::Pos2, std::time::Instant)>,
    /// Zoom des Tastenfelds in Prozent (20..400), wie im Bild-Tab.
    zoom: f32,
    /// Verschiebung des Tastenfelds in Punkten.
    pan: egui::Vec2,
    /// Bildschirmgrenze des Fensters (für Zoom um die Fingerposition).
    sichtbar: egui::Rect,
}

impl Default for MaskePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl MaskePanel {
    pub fn new() -> Self {
        Self {
            feldart: Feldart::JeWert,
            spalten: 8,
            reihen: 4,
            wert_max: 32,
            status: "Feld tippen: Zeitabstaende aufzeichnen; wischen = Schrittweite.".to_string(),
            spur: Vec::new(),
            finger: Mehrfinger::neu(),
            wisch: std::collections::HashMap::new(),
            folge: Vec::new(),
            zwischenschritte: Vec::new(),
            regeln: Vec::new(),
            auswahl: None,
            fest: false,
            anker: None,
            wege: std::collections::HashMap::new(),
            spurpunkt: std::collections::HashMap::new(),
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            sichtbar: egui::Rect::NOTHING,
        }
    }

    /// Die aufgezeichnete Folge (Zeitabstaende) fuer den Bild-Scan.
    pub fn folge(&self) -> &[Aufzeichnung] {
        &self.folge
    }

    /// Die Zeitpunkte der Folge **mit** den Abschnitts-Regeln (min/max
    /// Schritte aus der Zeitleiste). Der Bild-Tab baut daraus die Maske.
    pub fn zeiten(&self) -> Vec<f32> {
        let tipps: Vec<jamcore::schrittweite::Tipp> =
            self.folge.iter().map(Aufzeichnung::als_tipp).collect();
        jamcore::schrittweite::timestamps_mit_regeln(&tipps, &self.regeln)
    }

    /// Laufende Aufnahme? Der Bild-Scan zeigt nur waehrenddessen live mit.
    pub fn laeuft(&self) -> bool {
        self.anker.is_some()
    }

    /// Folge verwerfen und wieder aufnehmen. Ohne Ende-Knopf ist das der Weg
    /// aus einer festgehaltenen Folge heraus: der naechste Felddruck beginnt
    /// sonst sofort eine neue und die alte waere weg.
    pub fn neu(&mut self) {
        self.folge.clear();
        self.spur.clear();
        self.regeln.clear();
        self.auswahl = None;
        self.fest = false;
        self.anker = None;
        self.status = "neu — Feld tippen nimmt auf".to_string();
        info!("Maske: neu");
    }

    /// Die Zeit seit dem ersten Druck (Anker). Der erste Druck setzt ihn, wie
    /// beim C++-Recorder, wo der Timer beim ersten Tastendruck startet.
    fn zeit_seit_anker(&self) -> f32 {
        self.anker.map(|a| a.elapsed().as_secs_f32()).unwrap_or(0.0)
    }

    /// Einen Feldruck aufnehmen: Feldwert als Indexnum, Zeit und (beim Wisch)
    /// das Geschwindigkeitsprofil.
    fn druck_aufnehmen(&mut self, taste: usize, pos: egui::Pos2, tempo: Vec<f32>) {
        let zeit = self.zeit_seit_anker();
        let indexnum = self.wert_der_taste(taste) as f32;
        self.spur.push((taste, pos));
        info!("feld {} (indexnum {indexnum}) bei {zeit:.2}s", taste + 1);
        self.folge.push(Aufzeichnung {
            zeit,
            indexnum,
            tempo,
        });
        // Die Zwischenschritte sofort ausklappen und ablegen, wie der
        // pattern_recorder es mit `notes4` tut (nicht erst beim Umrechnen auf
        // die Bildspalten — sonst sind sie nach dem Bild-Tab weg). Die Regeln
        // (min/max aus der Zeitleiste) gehen mit ein.
        self.zwischenschritte = self.zeiten();
        let anzahl = self.folge.len();
        let gesamt = self.zwischenschritte.len();
        self.status = format!("{anzahl} Toene, {gesamt} Zeitpunkte, {zeit:.1} s");
        info!(
            "aufgeklappt: {anzahl} Toene -> {gesamt} Zeitpunkte ({:?})",
            self.zwischenschritte
                .iter()
                .map(|z| (z * 100.0).round() / 100.0)
                .collect::<Vec<_>>()
        );
    }

    fn tasten_anzahl(&self) -> usize {
        match self.feldart {
            Feldart::JeWert => (self.wert_max.max(1) + 1) as usize,
            Feldart::Verteilt => (self.spalten * self.reihen).max(2),
        }
    }

    /// Reihen des Feldes (bei "Je Wert" aus Spalten und Tastenanzahl).
    fn reihen(&self) -> usize {
        match self.feldart {
            Feldart::JeWert => self.tasten_anzahl().div_ceil(self.spalten.max(1)),
            Feldart::Verteilt => self.reihen.max(2),
        }
    }

    /// Wert einer Taste.
    fn wert_der_taste(&self, index: usize) -> i32 {
        match self.feldart {
            Feldart::JeWert => (index as i32).min(self.wert_max),
            Feldart::Verteilt => {
                let n = self.tasten_anzahl();
                if n <= 1 {
                    return 0;
                }
                (index as i32 * self.wert_max / (n as i32 - 1)).clamp(0, 127)
            }
        }
    }

    /// Taste unter einer Position im Feld (None außerhalb).
    /// Mitte einer Taste im Feld. Rechnet ueber den Sichtbereich, damit beim
    /// Zoom die richtige Zelle trifft.
    fn zentrum(&self, rect: egui::Rect, index: usize) -> egui::Pos2 {
        let reihen = self.reihen();
        let zell_w = rect.width() / self.spalten.max(1) as f32;
        let zell_h = rect.height() / reihen.max(1) as f32;
        let spalte = (index % self.spalten) as f32;
        let reihe = (index / self.spalten) as f32;
        egui::pos2(
            rect.left() + (spalte + 0.5) * zell_w,
            rect.top() + (reihe + 0.5) * zell_h,
        )
    }

    fn taste_unter(&self, rect: egui::Rect, pos: egui::Pos2) -> Option<usize> {
        if !rect.contains(pos) {
            return None;
        }
        let rel = pos - rect.min;
        let reihen = self.reihen();
        let spalte = ((rel.x / rect.width()) * self.spalten as f32) as usize;
        let reihe = ((rel.y / rect.height()) * reihen as f32) as usize;
        if spalte >= self.spalten || reihe >= reihen {
            return None;
        }
        let index = reihe * self.spalten + spalte;
        (index < self.tasten_anzahl()).then_some(index)
    }

    /// Den Wischweg je Frame messen: pro Frame der Abstand des Fingers,
    /// normiert auf die Zeit — daraus wird die Krümmungs-Modulation.
    fn weg_messen(&mut self, id: u64, pos: egui::Pos2) {
        let jetzt = std::time::Instant::now();
        if let Some((vorher, zeit)) = self.spurpunkt.insert(id, (pos, jetzt)) {
            let dt = jetzt.duration_since(zeit).as_secs_f32();
            if dt > 1e-4 {
                let weg = (pos - vorher).length();
                self.wege.entry(id).or_default().push(weg / dt);
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, pattern: &mut Pattern) {
        ui.heading("Takt-Maske");
        ui.horizontal(|ui| {
            ui.label("Feld");
            if ui
                .selectable_label(self.feldart == Feldart::JeWert, "je Wert")
                .clicked()
            {
                self.feldart = Feldart::JeWert;
            }
            if ui
                .selectable_label(self.feldart == Feldart::Verteilt, "verteilt")
                .clicked()
            {
                self.feldart = Feldart::Verteilt;
            }
            if ui.button("leeren").clicked() {
                self.folge.clear();
                self.zwischenschritte.clear();
                self.regeln.clear();
                self.auswahl = None;
                self.spur.clear();
                self.fest = false;
                self.anker = None;
                self.status = "Aufzeichnung geleert.".to_string();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Spalten");
            if ui.button("-").clicked() {
                self.spalten = self.spalten.saturating_sub(1).max(2);
            }
            ui.label(format!("{}", self.spalten));
            if ui.button("+").clicked() {
                self.spalten = (self.spalten + 1).min(16);
            }
            if self.feldart == Feldart::Verteilt {
                ui.label("Reihen");
                if ui.button("-").clicked() {
                    self.reihen = self.reihen.saturating_sub(1).max(2);
                }
                ui.label(format!("{}", self.reihen));
                if ui.button("+").clicked() {
                    self.reihen = (self.reihen + 1).min(8);
                }
            }
            ui.label("Wert max");
            if ui.button("-").clicked() {
                self.wert_max = (self.wert_max - 8).max(8);
            }
            ui.label(format!("{}", self.wert_max));
            if ui.button("+").clicked() {
                self.wert_max = (self.wert_max + 8).min(128);
            }
        });
        ui.label(
            RichText::new(format!(
                "{} Tasten (0..{})",
                self.tasten_anzahl(),
                self.wert_max
            ))
            .small()
            .color(Color32::GRAY),
        );

        // Die Touch-Ereignisse zuerst auswerten: die Geste (Pinch/Pan) braucht das
        // Ergebnis vor dem Zeichnen, und die Aufzeichnung nutzt dieselben
        // Fingerdaten.
        let rahmen = self.finger.schritt(ui);

        // Das Tastenfeld: ein Painter fürs Ganze (so kommen die Wischproben mit).
        // Pinch (zwei Finger) zoomt das Feld, drei Finger schieben es — wie im
        // Bild-Tab. Der Sichtbereich wandert mit, damit die Felder unter den
        // Fingern bleiben.
        let mut pan_versatz = self.pan;
        if let Some((zoomfaktor, weg)) = rahmen.geste {
            self.zoom = (self.zoom * zoomfaktor).clamp(ZOOM_MIN, ZOOM_MAX);
            pan_versatz += egui::vec2(weg.0, weg.1);
        }
        // Verschiebung merken, sonst waere der Pan nach jedem Frame weg.
        self.pan = pan_versatz;

        // Der Sichtbereich: normalerweise das ganze Feld, bei Zoom/Schieben ein
        // Ausschnitt. Alles Zeichnen rechnet ueber `sichtbar`.
        let breite = ui.available_width();
        let reihen = self.reihen();
        let hoehe = (breite * reihen as f32 / self.spalten.max(1) as f32).min(430.0);
        // Zoom und Verschiebung uebernehmen (der Wert wird weiter unten beim
        // Anzeigen gebraucht).
        let (response, painter) =
            ui.allocate_painter(egui::vec2(breite, hoehe), egui::Sense::click_and_drag());
        let ganzes = response.rect;
        let sichtbar = if self.zoom > 1.0 || pan_versatz.length() > 0.1 {
            let skaliert =
                egui::Rect::from_min_size(ganzes.min + pan_versatz, ganzes.size() / self.zoom);
            // Nicht aus dem Bild laufen lassen.
            egui::Rect::from_min_max(
                skaliert.min.clamp(ganzes.min, ganzes.max),
                skaliert.max.clamp(ganzes.min, ganzes.max),
            )
        } else {
            ganzes
        };
        self.sichtbar = sichtbar;
        let rect = sichtbar;
        let zell_w = rect.width() / self.spalten.max(1) as f32;
        let zell_h = rect.height() / reihen.max(1) as f32;
        let schrift = (zell_h * 0.45).clamp(9.0, 14.0);
        let aktiv = if response.is_pointer_button_down_on() {
            response
                .interact_pointer_pos()
                .and_then(|pos| self.taste_unter(rect, pos))
        } else {
            None
        };
        for i in 0..self.tasten_anzahl() {
            let spalte = (i % self.spalten) as f32;
            let reihe = (i / self.spalten) as f32;
            let zelle = egui::Rect::from_min_size(
                egui::pos2(
                    rect.left() + spalte * zell_w + 1.0,
                    rect.top() + reihe * zell_h + 1.0,
                ),
                egui::vec2(zell_w - 2.0, zell_h - 2.0),
            );
            let wert = self.wert_der_taste(i);
            // Getouchte Tasten im Regenbogen nach ihrer Reihenfolge.
            let spur_index = self.spur.iter().position(|(t, _)| *t == i);
            let farbe = if let Some(k) = spur_index {
                let anteil = if self.spur.len() <= 1 {
                    0.0
                } else {
                    k as f32 / (self.spur.len() - 1) as f32
                };
                regenbogen(anteil)
            } else if aktiv == Some(i) {
                Color32::from_rgb(0, 200, 255)
            } else if wert == 0 {
                Color32::from_rgb(45, 48, 58)
            } else {
                Color32::from_rgb(60, 70, 90)
            };
            painter.rect_filled(zelle, 3.0, farbe);
            painter.text(
                zelle.center(),
                egui::Align2::CENTER_CENTER,
                format!("{wert}"),
                egui::FontId::proportional(schrift),
                Color32::from_rgb(230, 230, 235),
            );
        }

        // Wischweg als Linien zwischen den getouchten Tasten.
        for (k, fenster) in self.spur.windows(2).enumerate() {
            let anteil = k as f32 / (self.spur.len().max(2) - 1) as f32;
            painter.line_segment(
                [fenster[0].1, fenster[1].1],
                egui::Stroke::new(3.0, regenbogen(anteil)),
            );
        }

        // Alles laeuft ueber die Touch-Ereignisse: so zaehlt ein Finger genau
        // einmal. Aufsetzen = erster Zeitpunkt, Loslassen = der letzte. Ein
        // Die Aufzeichnung selbst: Aufsetzen = erster Zeitpunkt, Loslassen =
        // letzter; ein Wisch ueber mehrere Felder zaehlt Anfangs- und
        // Endtaste als zwei Zeitpunkte mit dem Abstand dazwischen.
        for (id, pos) in rahmen.starts {
            // Solange die Folge festgehalten ist (nach „leeren“), zeichnet ein
            // Felddruck nichts auf — sonst waere die festgehaltene Folge beim
            // naechsten Tippen weg.
            if self.fest {
                continue;
            }
            // Nur ein Druck, der wirklich ein Feld trifft, ist ueberhaupt ein
            // Ton. Das Feld hat rechts neben der letzten Zeile leere Zellen:
            // ein Druck dort loeschte sonst die Folge und setzte den Anker,
            // ohne einen Ton zu liefern, und der naechste echte Tipp kam bei
            // 1,70 s statt bei 0 an (gemessen 2026-10-10).
            let Some(taste) = self.taste_unter(rect, pos) else {
                continue;
            };
            self.wisch.insert(id, (Some(taste), Some(taste)));
            self.wege.entry(id).or_default();
            self.spurpunkt.insert(id, (pos, std::time::Instant::now()));
            // Der allererste Druckoeffnet die Aufnahme: der C++-Recorder
            // startet seinen Timer beim ersten Tastendruck. Danach laeuft die
            // Aufnahme weiter (es gibt kein Ende), bis die Pause
            // `NEU_AFTER_SEKUNDEN` eine neue Folge beginnt — sonst haette der
            // Anker aus der Vorsitzung (548 s) den ersten Tipp um 548 s
            // verschoben.
            let neu = jamcore::schrittweite::beginnt_aufnahme(
                true,
                self.anker.map(|a| a.elapsed().as_secs_f32()),
                NEU_AFTER_SEKUNDEN,
            );
            if neu {
                self.anker = Some(std::time::Instant::now());
                self.folge.clear();
                self.zwischenschritte.clear();
                self.regeln.clear();
                self.auswahl = None;
                self.spur.clear();
                info!("Masken-Aufnahme durch ersten Tipp gestartet");
            }
            self.spur.clear();
            self.druck_aufnehmen(taste, pos, Vec::new());
        }
        for (id, pos) in rahmen.zuege {
            self.weg_messen(id, pos);
            let treffer = self.taste_unter(rect, pos);
            if let Some(gespeichert) = self.wisch.get_mut(&id) {
                gespeichert.1 = treffer.or(gespeichert.1);
            }
        }
        for (id, _) in rahmen.endes {
            let tempo = self.wege.remove(&id).unwrap_or_default();
            self.spurpunkt.remove(&id);
            // Keine Zeitgrenze: ein Wisch darf auch langsam sein, damit sich
            // das Beschleunigungsverhalten fein aufzeichnen laesst. Gescrollt
            // wird im Maske-Tab nur ueber die Leiste, Gesten gehoeren dem Feld.
            let Some((start, ende)) = self.wisch.remove(&id) else {
                continue;
            };
            let Some(start) = start else {
                continue;
            };
            // Der Wisch zaehlt als zweiter Zeitpunkt nur, wenn er wirklich
            // gewandert ist; ein Tippen auf dieselbe Taste bleibt einer.
            let gewandert = ende.filter(|ende| *ende != start);
            if let Some(ende) = gewandert {
                let pos = self.zentrum(rect, ende);
                self.druck_aufnehmen(ende, pos, tempo);
            }
        }

        // Zeitabstaende auf dem Raster der Patternlaenge (kein 16er-Raster: es
        // zaehlt derselbe Wert wie im Rhythmus-Tab). Der aufgezeichnete Weg
        // liegt als Linie darauf, die Zeitpunkte als Punkte mit ihrem Feldwert
        // und — wichtig — dem Abstand zum vorherigen in Sekunden.
        let schritte = pattern.steps_per_cycle().max(2) as usize;
        if !self.folge.is_empty() {
            let (leiste, painter) =
                ui.allocate_painter(egui::vec2(breite, 58.0), egui::Sense::click());
            let rect = leiste.rect;
            let mitte = rect.center().y + 4.0;
            painter.line_segment(
                [
                    egui::pos2(rect.left(), mitte),
                    egui::pos2(rect.right(), mitte),
                ],
                egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
            );
            for s in 0..schritte {
                let x = rect.left() + rect.width() * s as f32 / (schritte - 1) as f32;
                painter.line_segment(
                    [egui::pos2(x, mitte - 3.0), egui::pos2(x, mitte + 3.0)],
                    egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
                );
            }
            let punkte = if self.zwischenschritte.is_empty() {
                self.folge.iter().map(|a| a.zeit).collect::<Vec<f32>>()
            } else {
                self.zwischenschritte.clone()
            };
            let erste = punkte[0].max(1e-4);
            let spanne = (punkte[punkte.len() - 1] - erste).max(1e-4);
            let x_von =
                |z: f32| rect.left() + rect.width() * ((z - erste) / spanne).clamp(0.0, 1.0);
            // Der angetippte Abschnitt liegt als heller Balken unter den
            // Punkten: dort stellen die Regler darunter min/max ein.
            if let Some(i) = self.auswahl {
                if i + 1 < self.folge.len() {
                    let x0 = x_von(self.folge[i].zeit);
                    let x1 = x_von(self.folge[i + 1].zeit);
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(x0, rect.top()),
                            egui::pos2(x1, rect.bottom()),
                        ),
                        2.0,
                        Color32::from_rgba_unmultiplied(90, 140, 220, 48),
                    );
                }
            }
            let mut vorher: Option<egui::Pos2> = None;
            for (i, &z) in punkte.iter().enumerate() {
                let anteil = ((z - erste) / spanne).clamp(0.0, 1.0);
                let x = rect.left() + rect.width() * anteil;
                let p = egui::pos2(x, mitte);
                if let Some(v) = vorher {
                    // Die Zeitabstaende sind das, worum es geht: die Linie
                    // dazwischen zeigt ihren Laengenunterschied.
                    painter.line_segment(
                        [v, p],
                        egui::Stroke::new(2.0, Color32::from_rgb(90, 200, 255)),
                    );
                }
                // Handdruck = grosser Punkt mit Feldwert, Zwischenschritt =
                // kleiner Punkt. So sieht man, was getippt wurde und was die
                // Schrittweite dazwischen erzeugt hat.
                let handdruck = self.folge.get(i).is_some_and(|a| a.zeit == z);
                let radius = if handdruck { 5.0 } else { 2.5 };
                painter.circle_filled(
                    p,
                    radius,
                    if handdruck {
                        Color32::from_rgb(120, 230, 160)
                    } else {
                        Color32::from_rgb(90, 200, 255)
                    },
                );
                if handdruck {
                    painter.text(
                        egui::pos2(x, mitte - 14.0),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", self.folge[i].indexnum as i32),
                        egui::FontId::proportional(10.0),
                        Color32::from_rgb(225, 225, 230),
                    );
                }
                vorher = Some(p);
            }
            // Die Abstaende **aller** Zeitpunkte als Text: bei gedrueckter
            // Breite zaehlt die Zahl mehr als die Zeichnung.
            let abstaende: Vec<String> = punkte
                .windows(2)
                .map(|w| format!("{:.2}", w[1] - w[0]))
                .collect();
            if !abstaende.is_empty() {
                painter.text(
                    egui::pos2(rect.center().x, rect.bottom() - 2.0),
                    egui::Align2::CENTER_BOTTOM,
                    format!("Abstand: {} s", abstaende.join(" / ")),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(160, 160, 170),
                );
            }
            // Antippen waehlt den Abschnitt (zwischen zwei Handdruecken), fuer
            // den die min/max-Regler darunter gelten.
            if leiste.clicked() {
                if let Some(pos) = leiste.interact_pointer_pos() {
                    let anteil = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
                    let t = erste + anteil * spanne;
                    let gewaehlt = (0..self.folge.len().saturating_sub(1))
                        .find(|i| t >= self.folge[*i].zeit && t <= self.folge[*i + 1].zeit);
                    self.auswahl = gewaehlt;
                    if let Some(i) = gewaehlt {
                        info!("Abschnitt {} gewaehlt (Zeitleiste)", i + 1);
                    }
                }
            }
        }

        // Min/max Schritte des angetippten Abschnitts. Die Rampe laeuft dann
        // von `min` nach `max`, statt die Feldwerte der beiden Tipps zu
        // nehmen (Nutzerwunsch 2026-10-12).
        if let Some(i) = self.auswahl {
            if i + 1 < self.folge.len() {
                let tipp_von = self.folge[i].indexnum;
                let tipp_bis = self.folge[i + 1].indexnum;
                let regel = self.regeln.get(i).copied().flatten();
                let grenze = self.wert_max.max(1) as i32;
                let mut von = regel
                    .map(|r| r.von as i32)
                    .unwrap_or(tipp_von as i32)
                    .clamp(1, grenze);
                let mut bis = regel
                    .map(|r| r.bis as i32)
                    .unwrap_or(tipp_bis as i32)
                    .clamp(1, grenze);
                ui.label(
                    RichText::new(format!(
                        "Abschnitt {} (Tipp {:.0} -> {:.0}{})",
                        i + 1,
                        tipp_von,
                        tipp_bis,
                        if regel.is_some() { ", Regel" } else { "" }
                    ))
                    .small()
                    .color(Color32::GRAY),
                );
                let a = ui
                    .horizontal(|ui| {
                        ui.label("min");
                        ui.add(egui::Slider::new(&mut von, 1..=grenze)).changed()
                    })
                    .inner;
                let b = ui
                    .horizontal(|ui| {
                        ui.label("max");
                        ui.add(egui::Slider::new(&mut bis, 1..=grenze)).changed()
                    })
                    .inner;
                if a || b {
                    if self.regeln.len() <= i {
                        self.regeln.resize(i + 1, None);
                    }
                    self.regeln[i] = Some(jamcore::schrittweite::Abschnitt {
                        von: von as f32,
                        bis: bis as f32,
                    });
                    self.zwischenschritte = self.zeiten();
                    self.status = format!("Abschnitt {}: min {von}, max {bis}", i + 1);
                    info!("Abschnitt {}: min {von}, max {bis}", i + 1);
                }
                if regel.is_some() && ui.button("Regel weg").clicked() {
                    self.regeln[i] = None;
                    self.zwischenschritte = self.zeiten();
                    self.status = format!("Abschnitt {}: Regel entfernt", i + 1);
                }
            }
        } else if !self.folge.is_empty() {
            ui.label(
                RichText::new("Zeitleiste antippen: min/max Schritte für einen Abschnitt.")
                    .small()
                    .color(Color32::DARK_GRAY),
            );
        }

        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        // Zoom wie im Bild-Tab: zwei Finger, 20 bis 400 Prozent.
        ui.horizontal(|ui| {
            ui.label(format!("Zoom {:.0} %", self.zoom * 100.0));
            if (self.zoom - 1.0).abs() > 0.01 {
                if ui.button("Zoom weg").clicked() {
                    self.zoom = 1.0;
                    self.pan = egui::Vec2::ZERO;
                }
            }
        });
        ui.label(format!(
            "Toene: {} — Zeitpunkte mit Zwischenschritten: {}",
            self.folge.len(),
            self.zwischenschritte.len()
        ));
        ui.label(
            RichText::new(
                "Der Feldwert ist das Indexnum (Unterteilungen je Schritt): die \
                 Schrittbreite ist 1/Wert, ein kleines Feld also ein breiter \
                 Schritt. Zwischen zwei verschiedenen Feldern entsteht daraus eine \
                 Rampe; ein Wisch zählt als zwei Zeitpunkte mit dem Abstand \
                 dazwischen. Der erste Druck beginnt die Aufnahme.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
    }
}
