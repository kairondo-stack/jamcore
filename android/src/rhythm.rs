//! Rhythmus-Screen: Noten-Raster, das auf Module-Takt oder eigene Uhr läuft.
//!
//! Semantik: Das Raster ist ein **Step-Editor**, Spalte = Schritt, Zeile = Ton.
//! Ein Tipp setzt genau die angefasste Stelle; der laufende Cursor (Follow oder
//! eigene Uhr) zeigt nur, wo das Modul gerade ist. Ein Aufnahme-Modus, der
//! jeden Tipp auf dem aktuellen Schritt landen lässt, ist bewusst nicht gebaut
//! (offener Planpunkt, siehe docs/plans/2026-09-30-android-port.md, S6).
//!
//! Der Austausch mit Renoise läuft über `jamcore::grid` (CSV wie im Desktop-
//! Recorder, Clipboard-XML für den Import) und über OSC fürs sofortige Hören.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Grid, RichText, Sense};
use jamcore::euclid;
use jamcore::grid::{note_name, Note, Pattern, MAX_STEPS_PER_CYCLE};
use jamcore::osc::{OscOut, Renoise, DEFAULT_BASE_PORT};
use jamcore::tracker::{klemme_kanal, loeschen, setzen, zelle_text, KANAELE};
use jamcore::transformation;
use log::info;

// Die Tracker-Ansicht steht bewusst hier statt über ein `mod tracker;` in
// `lib.rs`: so bleibt die neue Ansicht auf `rhythm.rs` + `tracker.rs`
// beschränkt (in `lib.rs` arbeitet parallel die Laub-/Video-Aufgabe).
// Kanonisch gehörte die Zeile `mod tracker;` in `lib.rs`.
#[path = "tracker.rs"]
mod tracker;

use self::tracker::{Ansicht, Zustand as TrackerZustand};

const ROWS: [i32; 4] = [36, 48, 60, 72];
/// Notennamen der Schnell-Tastatur im Tracker (C … B). Warum eine eigene
/// Reihe: die Pads liegen unter allen Werkzeugen; nach der Zellwahl soll die
/// Noteneingabe ohne Scrollen erreichbar sein.
const NOTE_KLASSEN: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const CELL: f32 = 22.0;
const BLOCK: u16 = 8;
const DEFAULT_BPM: f32 = 128.0;
/// Tonzeile, auf die das Euklid-Muster schreibt: die erste Rasterzeile
/// (36, Kick/Bass). Die anderen Zeilen bleiben fürs Antippen frei.
const EUCLID_PITCH: i32 = ROWS[0];

/// Tonhöhen-Offset je Laub-Farbe (rot/gelb/braun/grün): Grundton, kleine
/// Terz, Quinte, Oktave. Warum: die Farbe ist nur die Instrumentnummer
/// (0…3), und bei einer Klangquelle mit einem einzigen Sample klänge sonst
/// jede Note gleich. Vier Tonstufen lassen den Laub-Rhythmus melodisch
/// wirken, ohne die Farbzuordnung (Instrument) zu verändern.
const LAUB_STUFEN: [i32; 4] = [0, 3, 7, 12];

/// Tonhöhe einer Laub-Farbe auf der Schlagzeug-Zeile: [`EUCLID_PITCH`] plus
/// Terz/Quinte/Oktave. Dieselbe Abbildung nutzen Pattern und Anhören, damit
/// die Vorschau klingt wie das gerenderte Material.
pub fn laub_pitch(farbe: u8) -> i32 {
    EUCLID_PITCH + LAUB_STUFEN[(farbe as usize).min(LAUB_STUFEN.len() - 1)]
}

/// „125 ms“ zur Tonlaenge — die ms haengen am aktuellen BPM.
fn ms_text(e: crate::klang::Einstellungen) -> String {
    format!("{} ms", e.notenwert.ms(e.bpm))
}

/// Note fuer das Pad-Raster: Pads und Werkzeuge (Rampe, Euklid, Laub,
/// Skalieren, Interpolieren) schreiben auf **Kanal 0**; die Kanäle 2…64
/// gehören der Tracker-Ansicht, die über `jamcore::tracker::setzen` schreibt.
fn raster_note(step: u16, instrument: u8, pitch: i32) -> Note {
    Note {
        step,
        instrument,
        pitch,
        glide: false,
        channel: 0,
    }
}

/// Laub-Note: Farbe = Instrument (0…3), **Kanal = Farbe** (intern 0…3, im
/// Tracker also 1–4). Warum nicht wie die Pads alles auf Kanal 0: getrennte
/// Spalten im Tracker, polyphone Wiedergabe ohne gegenseitiges Überschreiben
/// und später getrennte `.it`-Kanäle je Farbe. Nur der Laub-Weg macht das;
/// Pads und die übrigen Werkzeuge bleiben auf Kanal 0.
fn laub_note(step: u16, farbe: u8) -> Note {
    let farbe = farbe.min(3);
    Note {
        step,
        instrument: farbe,
        pitch: laub_pitch(farbe),
        glide: false,
        channel: farbe.min(63),
    }
}

/// Zahl der verschiedenen Kanäle in einer Notenliste — für Status und Log der
/// Wiedergabe („N Noten auf M Kanälen“). Die Noten stehen als
/// `(Schritt, Tonhöhe, Instrument, Kanal)`, wie sie der Render-Thread spielt.
fn kanaele_anzahl(noten: &[(u16, i32, u8, u8)]) -> usize {
    let mut kanaele: Vec<u8> = noten.iter().map(|(_, _, _, kanal)| *kanal).collect();
    kanaele.sort_unstable();
    kanaele.dedup();
    kanaele.len()
}

/// Dauer eines Notenwerts in Schritten des aktuellen Rasters (mindestens 1).
/// Ein Schritt ist `feinheit/4` einer Viertel; die Notenlänge kommt aus
/// [`jamcore::klang::Notenwert::ms`], damit Raster, Tongenerator und
/// Kanalvergabe dieselbe Zeit meinen (1/16 bei Feinheit 16 = 1 Schritt).
fn dauer_schritte(notenwert: jamcore::klang::Notenwert, feinheit: u16, bpm: f32) -> u32 {
    let bpm = bpm.clamp(20.0, 300.0);
    let schritt_ms = 60_000.0 * 4.0 / (bpm * f32::from(feinheit.max(1)));
    ((notenwert.ms(bpm) as f32 / schritt_ms).round() as u32).max(1)
}

/// Kanäle je Note (gleiche Reihenfolge wie `noten`): Sample = Instrument,
/// Start = Schritt, Dauer = Tonlänge des eingestellten Notenwerts. Die
/// Audioausgabe bleibt polyphon; die Kanäle sind die Ordnung im Tracker und
/// stehen im Log/Status. Regeln: `jamcore::kanal::verteile_mit`.
fn kanaele_je_note(noten: &[Note], feinheit: u16, bpm: f32, sample_zuerst: bool) -> Vec<u8> {
    let dauer = dauer_schritte(crate::klang::einstellungen().notenwert, feinheit, bpm);
    let kandidaten: Vec<jamcore::kanal::Kandidat> = noten
        .iter()
        .map(|n| jamcore::kanal::Kandidat {
            start: u32::from(n.step),
            sample: n.instrument,
            dauer,
        })
        .collect();
    jamcore::kanal::verteile_mit(&kandidaten, KANAELE, sample_zuerst)
}

/// Note ueber das Modul spielen — mit der Zuordnung aus dem Instrument-Tab.
/// `true`, wenn das Modul geklungen hat; sonst faellt der Tongenerator ein.
fn modul_gespielt(modul: &Option<crate::player::ModulQuelle>, pitch: i32) -> bool {
    let Some(quelle) = modul.as_ref() else {
        return false;
    };
    match crate::instr::spielen(quelle, pitch, 0.8) {
        Some(gespielt) => gespielt,
        None => quelle.note_spielen(30, pitch, 0.8).is_some(),
    }
}

/// Woher der Klang des gerenderten Notenmaterials kommt.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Klangquelle {
    /// Samples/Instrumente des geladenen Moduls (ext-API), sonst Tongenerator.
    Modul,
    /// Immer der Tongenerator mit gewaehlter Wellenform.
    Welle,
    /// Slices aus dem Slicer-Tab (Instrument-Index = Slice-Nummer), sonst
    /// Tongenerator.
    Slices,
}

pub struct RhythmPanel {
    slots: Vec<Pattern>,
    slot: usize,
    target: (String, u16),
    velocity: u8,
    bpm: f32,
    follow: bool,
    current_step: u16,
    clock: Instant,
    status: String,
    dir: Option<String>,
    /// "Rendern" (wie Taste o im Desktop-Recorder): Raster abspielen.
    render_laeuft: bool,
    render_stopp: Option<Arc<AtomicBool>>,
    render_schritt: Arc<AtomicU16>,
    render_thread: Option<std::thread::JoinHandle<()>>,
    render_bpm: f32,
    transport_mit: bool,
    /// Aufnahme: Pad-Tipps landen auf dem laufenden Render-Schritt.
    aufnahme: bool,
    /// Presenter-Ansicht (wie Taste J im Desktop-Recorder).
    presenter: bool,
    /// Instrumentnummer fuer neue Noten (Renoise-Instrument).
    instrument: u8,
    /// Feinheit der Uhr: Pattern-Zeilen je Takt (16 = 1/16, 32 = 1/32,
    /// 64 = 1/64). Bestimmt Taktzeit, Raster und Laub-Rendern.
    feinheit: u16,
    /// Laub-Rendern auf zwei Patterns verteilen (erste Hälfte / Rest).
    zwei_patterns: bool,
    /// Wie viele Patterns „Rendern" hintereinander spielt (1 = nur das
    /// aktuelle; macht Taktgruppen/Pools hörbar: Slot 1, 2, …).
    kette: usize,
    /// Anzeige-Buendelung: so viele Schritte fasst eine Rasterzelle zusammen
    /// (1 = ein Schritt; 2, 4, 8, 16, 32 fuer lange Patterns).
    buendel: u16,
    /// Skalenfilter fuer gesetzte Toene (0 = aus).
    skala: usize,
    grundton: i32,
    /// Notenrampe (Desktop-Taste 2).
    rampe_anzahl: i32,
    /// Sinus-Interpolation (Desktop D+3).
    interp_schritte: i32,
    interp_amplitude: f32,
    interp_faktor: f32,
    interp_kurve: i32,
    /// Schrittbereich zum Loeschen (wie „E"/remove im Recorder).
    bereich_von: u16,
    bereich_bis: u16,
    /// Klangquelle fuer gerenderte Noten.
    klangquelle: Klangquelle,
    /// Wellenform des Tongenerators.
    welle: jamcore::klang::Welle,
    /// Handgriff auf das Modul des Players (von JamApp gesetzt).
    modul_quelle: Option<crate::player::ModulQuelle>,
    /// Euklid-Werkzeug (Max-Patch `eucl-midi-in out`): Schläge, Schritte und
    /// Rotation des Musters. Das Muster selbst wird daraus berechnet.
    euclid_schlag: usize,
    euclid_schritte: usize,
    euclid_versatz: usize,
    /// Ansicht des Tabs: Pad-Raster (Step-Editor) oder Tracker (64 Kanäle).
    ansicht: Ansicht,
    /// Gewählte Tracker-Zelle — dorthin schreiben die Pads ihre Note.
    tracker_schritt: u16,
    tracker_kanal: u8,
    /// Zeile, die die Tracker-Ansicht als Nächstes in den Blick holen soll
    /// („Schritt ±“); wird von der Ansicht verbraucht.
    tracker_scroll: Option<u16>,
    /// Beim Abspielen automatisch zur laufenden Zeile scrollen (Standard an).
    tracker_folgen: bool,
    /// Zuletzt gesehener Spielschritt — nur bei Änderung wird gescrollt.
    tracker_letzter_spiel: u16,
    /// Reihenfolge der Kanalvergabe („Sample zuerst“ ist die Vorgabe): bei
    /// „frei zuerst“ gewinnt der niedrigste freie Kanal vor dem gleichen
    /// Sample. Wirkt beim Verteilen und in der Wiedergabe (Log/Status).
    kanal_sample_zuerst: bool,
    /// Ein Schnappschuss des Patterns vor der letzten schreibenden Aktion
    /// (Slot, Pattern). „Rückgängig“ stellt genau diesen Stand wieder her —
    /// eine Stufe, wie im Recorder; weitere Schritte wuerden auf dem Handy
    /// mehr verwirren als helfen.
    rueckgaengig: Option<(usize, Pattern)>,
    /// Grundton der Akkord-Presets (MIDI). Getrennt von `grundton` (Skala),
    /// damit ein Akkord-Grundton den Skalenfilter nicht unbemerkt verschiebt.
    akkord_grundton: i32,
    /// Zuletzt gesetzter Akkord (Schritt, Startkanal, Toene): ein neuer Akkord
    /// auf derselben Stelle raeumt zuerst die alten Toene weg, sonst blieben
    /// bei Moll -> Dur die Terz und bei 7 -> Dur die Septime liegen.
    akkord_letzte: Option<(u16, u8, usize)>,
    /// Ziel-Instrument-Abgleich mit dem Slicer (siehe `ziel_abgleichen`): die
    /// Version merkt, ob die Auswahl zuletzt im Slicer geaendert wurde.
    ziel_version: u64,
    /// Basiston der Schnell-Tastatur im Tracker (C…B), MIDI; mit „Okt ±“
    /// verschiebbar. Warum: die Pads liegen unter allen Werkzeugen — nach der
    /// Zellwahl soll die Noteneingabe ohne Scrollen erreichbar sein.
    tracker_oktav: i32,
}

impl Default for RhythmPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RhythmPanel {
    pub fn new() -> Self {
        // Start mit 128 Zeilen statt der Kern-Vorgabe 512: der Laub-Render
        // (ein Takt je 16 Zeilen) passt damit in ein übersichtliches Pattern.
        let mut slots = vec![Pattern::new(); 8];
        for pattern in &mut slots {
            pattern.set_steps_per_cycle(128);
        }
        // Nutzerwunsch 20.10.: die Tracker-Ansicht ist der Standard — nach dem
        // Start steht die Zellwahl samt Schnell-Tastatur sofort da, das
        // Pad-Raster bleibt ueber den Umschalter erreichbar.
        info!("rhythm: Ansicht Tracker (Standard)");
        Self {
            slots,
            slot: 0,
            target: ("127.0.0.1".to_string(), 7003),
            velocity: 100,
            bpm: DEFAULT_BPM,
            follow: false,
            current_step: 0,
            clock: Instant::now(),
            status: "Tracker: Zelle antippen, dann Note mit den Tasten C…B darunter setzen."
                .to_string(),
            dir: None,
            render_laeuft: false,
            render_stopp: None,
            render_schritt: Arc::new(AtomicU16::new(0)),
            render_thread: None,
            render_bpm: DEFAULT_BPM,
            transport_mit: false,
            aufnahme: false,
            presenter: false,
            instrument: 0,
            feinheit: 16,
            zwei_patterns: false,
            kette: 1,
            buendel: 1,
            skala: 0,
            grundton: 60,
            rampe_anzahl: 4,
            interp_schritte: 1,
            interp_amplitude: 12.0,
            interp_faktor: 1.0,
            interp_kurve: 3,
            bereich_von: 0,
            bereich_bis: 127,
            klangquelle: Klangquelle::Modul,
            welle: jamcore::klang::Welle::Sinus,
            modul_quelle: None,
            // Vorgabe aus dem Max-Patch: 5 Schläge auf 16 Schritte.
            euclid_schlag: 5,
            euclid_schritte: 16,
            euclid_versatz: 0,
            ansicht: Ansicht::Tracker,
            tracker_schritt: 0,
            tracker_kanal: 0,
            tracker_scroll: None,
            tracker_folgen: true,
            tracker_letzter_spiel: 0,
            kanal_sample_zuerst: true,
            rueckgaengig: None,
            akkord_grundton: 60,
            akkord_letzte: None,
            ziel_version: crate::slicer::ziel_version(),
            tracker_oktav: 60,
        }
    }

    /// Host/Port des OSC-Tabs übernehmen, damit beide dasselbe Ziel senden.
    pub fn set_target(&mut self, host: &str, port: &str) {
        // Ungueltige Eingabe: gleicher Standard wie der OSC-Tab, sonst senden
        // Rhythmus und OSC-Tab an verschiedene Ports.
        self.target = (
            host.trim().to_string(),
            port.trim().parse().unwrap_or(DEFAULT_BASE_PORT),
        );
    }

    /// Handgriff auf das Player-Modul setzen (JamApp, jedes Frame).
    pub fn set_modul_quelle(&mut self, quelle: Option<crate::player::ModulQuelle>) {
        self.modul_quelle = quelle;
    }

    /// Schnappschuss vor einer schreibenden Aktion. Nur **eine** Stufe: der
    /// naechste Schreibvorgang ersetzt ihn. Warum ein Klon des ganzen
    /// Patterns: die Aktionen (Laub, Euklid, CSV, Kanäle) aendern Laenge,
    /// Instrumente und Kanaele — eine Notliste allein wuerde nicht reichen.
    fn schnappschuss(&mut self) {
        let slot = self.slot.min(self.slots.len().saturating_sub(1));
        self.rueckgaengig = Some((slot, self.slots[slot].clone()));
    }

    /// Den letzten Schnappschuss wiederherstellen (Knopf „Rückgängig“).
    fn rueckgaengig_anwenden(&mut self) {
        let Some((slot, pattern)) = self.rueckgaengig.take() else {
            self.status = "Nichts zum Rückgängigmachen.".to_string();
            return;
        };
        let noten = pattern.notes().len();
        self.slots[slot] = pattern;
        self.slot = slot;
        self.status = format!("Rückgängig: Pattern {} ({} Noten)", slot + 1, noten);
        info!("rhythm: Rueckgaengig ({noten} Noten)");
    }

    /// Ziel-Instrument mit dem Slicer abgleichen (beide Richtungen):
    /// - Im Slicer geaendert (Version gestiegen) → hier uebernehmen, damit
    ///   „→ Instr N“ und die Instr-Anzeige denselben Wert meinen.
    /// - Hier geaendert (Instr ±, Slice-Klemmen) → in den Slicer schreiben.
    /// Der Vergleich laeuft ueber den Versionszaehler, nicht ueber den Wert:
    /// so wird nur eine echte Aenderung geloggt und kein Frame-Log erzeugt.
    fn ziel_abgleichen(&mut self) {
        let version = crate::slicer::ziel_version();
        if version != self.ziel_version {
            self.ziel_version = version;
            let ziel = crate::slicer::ziel_instrument();
            if ziel != self.instrument {
                self.instrument = ziel;
                info!("rhythm: Ziel-Instrument {} aus dem Slicer", ziel);
            }
        } else if crate::slicer::ziel_instrument() != self.instrument {
            crate::slicer::ziel_instrument_setzen(self.instrument);
            self.ziel_version = crate::slicer::ziel_version();
        }
    }

    /// Anzeige des aktuellen Instruments; ein im Slicer getauschtes Sample
    /// wird als `Instr N *Slice` sichtbar (Nutzerwunsch „instr. sampletausch“).
    fn instrument_anzeige(&self) -> String {
        match crate::slicer::sample_name(self.instrument as usize) {
            Some(name) => format!("Instr {} *Slice ({name})", self.instrument),
            None => format!("Instr {}", self.instrument),
        }
    }

    /// Ein Akkord-Preset auf den gewaehlten Tracker-Schritt setzen: je Ton ein
    /// Kanal, fortlaufend ab dem gewaehlten Kanal. Vorher werden die Toene des
    /// zuletzt hier gesetzten Akkords an derselben Stelle entfernt — sonst
    /// blieben Reste des laengeren Akkords liegen.
    fn akkord_setzen(&mut self, preset: usize) {
        let Some((name, _)) = crate::akkord::PRESETS.get(preset) else {
            return;
        };
        let name = (*name).to_string();
        let toene = crate::akkord::noten(preset, self.akkord_grundton);
        if toene.is_empty() {
            return;
        }
        self.schnappschuss();
        let step = self.tracker_schritt.min(self.pattern().steps_per_cycle() - 1);
        let start = self.tracker_kanal;
        // Alten Akkord an genau dieser Stelle raeumen (nur die Zellen, die er
        // belegt hat — fremde Noten auf anderen Kanaelen bleiben stehen).
        if let Some((alt_step, alt_start, alt_anzahl)) = self.akkord_letzte {
            if alt_step == step && alt_start == start {
                for i in 0..alt_anzahl {
                    let kanal = ((start as usize + i) % KANAELE as usize) as u8;
                    loeschen(self.pattern_mut(), step, kanal);
                }
            }
        }
        let anzahl = toene.len();
        let instrument = self.instrument;
        for (i, pitch) in toene.iter().enumerate() {
            let kanal = ((start as usize + i) % KANAELE as usize) as u8;
            setzen(
                self.pattern_mut(),
                Note {
                    step,
                    instrument,
                    pitch: *pitch,
                    glide: false,
                    channel: kanal,
                },
            );
        }
        self.akkord_letzte = Some((step, start, anzahl));
        // Alle Toene lokal hoeren (polyphon) und per OSC senden: so ist der
        // Akkord sofort pruefbar, bevor weitere Schritte dazukommen.
        for (i, pitch) in toene.iter().enumerate() {
            let kanal = ((start as usize + i) % KANAELE as usize) as u8;
            self.play(*pitch, self.velocity);
            self.note_klingen(*pitch, self.instrument, kanal);
        }
        let ende = ((start as usize + anzahl - 1) % KANAELE as usize) + 1;
        self.status = format!(
            "Akkord {name} auf Schritt {} ({anzahl} Toene, Kanäle {}-{ende})",
            step + 1,
            start as usize + 1
        );
        info!(
            "rhythm: Akkord {name} auf Schritt {} ({anzahl} Toene)",
            step + 1
        );
    }

    /// „Alles hören“: Kette auf alle belegten Patterns setzen und rendern.
    /// Belegt = bis zum letzten Slot mit mindestens einer Note (Luecken
    /// dazwischen spielen leer, wie die Kette sie ohnehin behandelt).
    fn alles_hoeren(&mut self) {
        let belegt = self
            .slots
            .iter()
            .rposition(|p| !p.notes().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        if belegt == 0 {
            self.status = "Kein Pattern belegt — erst Noten setzen.".to_string();
            return;
        }
        self.kette = belegt;
        let noten: usize = self.slots.iter().take(belegt).map(|p| p.notes().len()).sum();
        info!("rhythm: Alles hoeren: Kette {belegt} ({noten} Noten)");
        self.rendern_starten();
        if self.render_laeuft {
            self.status = format!(
                "Alles hören: {belegt} Pattern(s), {noten} Noten — Kette läuft"
            );
        }
    }

    /// Klangquelle, Wellenform und Modul fuer andere Panels (der Bild-Scan
    /// klingt mit derselben Quelle). So gibt es **einen** Schalter fuer Bild
    /// und Pattern.
    pub fn klang_zustand(
        &self,
    ) -> (
        Klangquelle,
        jamcore::klang::Welle,
        Option<crate::player::ModulQuelle>,
    ) {
        (self.klangquelle, self.welle, self.modul_quelle.clone())
    }

    /// Eine Note klingen lassen: Modul-Sample, Slice oder Tongenerator.
    /// Ausserhalb von Threads; fuer die Threads wird die Logik dort wiederholt.
    /// `instrument` ist die Instrumentnummer des Rasters — bei der Klangquelle
    /// „Slices" ist sie der Slice-Index. `kanal` ist der Tracker-Kanal der
    /// Note: **alle** Kanäle klingen (polyphon) — das Modul vergibt je Note
    /// einen freien Modul-Kanal, der Tongenerator-Mixer je Note eine Stimme.
    /// Die Klangwege kennen keinen Kanalparameter, deshalb steht er hier nur
    /// im Log (welche Note auf welchem Kanal liegt, zeigt der Status).
    fn note_klingen(&self, pitch: i32, instrument: u8, kanal: u8) {
        log::debug!(
            "Klang: Kanal {} {} (Instr {})",
            kanal as usize + 1,
            note_name(pitch),
            instrument
        );
        if self.klangquelle == Klangquelle::Slices
            && crate::slicer::slice_spielen(instrument as usize)
        {
            return;
        }
        if self.klangquelle == Klangquelle::Modul {
            // Ein im Slicer getauschtes Sample hat Vorrang vor dem Modul:
            // „Instr N ← Slice“ soll sofort klingen, auch ohne Modul.
            if crate::slicer::instrument_sample_spielen(instrument as usize, pitch) {
                return;
            }
            if let Some(quelle) = self.modul_quelle.as_ref() {
                // Zuordnung aus dem Instrument-Tab: mehrere Bereiche klingen
                // zusammen, ausserhalb der Bereiche bleibt es still.
                match crate::instr::spielen(quelle, pitch, 0.8) {
                    Some(_) => return,
                    None => {
                        if quelle.note_spielen(30, pitch, 0.8).is_some() {
                            return;
                        }
                    }
                }
            }
        }
        crate::klang::spielen_welle(pitch, self.welle);
    }

    /// Das eine Tempo der App (Follow gewinnt) fuer das Rendern setzen.
    pub fn set_tempo(&mut self, bpm: f32) {
        self.render_bpm = bpm.clamp(20.0, 300.0);
    }

    /// Eigenuhr-BPM (der Regler im Rhythmus-Tab) — ein zentraler Wert: die App
    /// liest ihn zurueck und schreibt ihn in alle anderen Tabs.
    pub fn eigenuhr_bpm(&self) -> f32 {
        self.bpm
    }

    /// Uhr-Tempo für das Anhören im Laub-Tab (dieselbe Eigenuhr).
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Feinheit der Uhr: Pattern-Zeilen je Takt (16, 32 oder 64). Das
    /// Laub-Anhören spielt damit im selben Zeitraster wie das Pattern.
    pub fn feinheit(&self) -> u16 {
        self.feinheit
    }

    /// Laub-Rendern auf zwei Patterns verteilen?
    pub fn zwei_patterns(&self) -> bool {
        self.zwei_patterns
    }

    /// „2 Patterns“ aus dem Laub-Tab setzen.
    pub fn set_zwei_patterns(&mut self, an: bool) {
        self.zwei_patterns = an;
    }

    /// Die Eigenuhr von aussen stellen (App-Tempo).
    pub fn set_eigenuhr(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(40.0, 220.0);
    }

    /// Notenrampe wie Taste `2` im Desktop-Recorder: `n + 1` Noten von 59
    /// aufwärts, gleichmäßig über den Zyklus verteilt.
    fn rampe_anwenden(&mut self) {
        self.schnappschuss();
        let steps = self.pattern().steps_per_cycle().max(1);
        let rampe = transformation::notenrampe(self.rampe_anzahl);
        let instrument = self.instrument;
        self.pattern_mut().clear();
        for (t, pitch) in rampe {
            let step = ((t * steps as f32).round() as i64).rem_euclid(steps as i64) as u16;
            self.pattern_mut()
                .insert(raster_note(step, instrument, pitch.clamp(0, 127)));
        }
        self.status = format!(
            "Rampe: {} Noten (59 … {})",
            self.pattern().notes().len(),
            59 + self.rampe_anzahl
        );
    }

    /// Euklidisches Muster (0/1 je Zelle) aus den Reglern, inklusive Rotation
    /// („Rotieren +1“ zählt `euclid_versatz` hoch).
    fn euclid_muster(&self) -> Vec<bool> {
        let basis = euclid::muster(self.euclid_schritte, self.euclid_schlag);
        euclid::rotiere(&basis, self.euclid_versatz)
    }

    /// Das Euklid-Muster als Noten ins aktuelle Pattern schreiben
    /// (Max-Patch `eucl-midi-in out`): 1 = Note auf den Schritt.
    ///
    /// Abbildung auf die Patternlänge: Das Muster wird **wiederholt** — Schritt
    /// `i` liest die Zelle `i % muster.len()`. Damit liegt 5/16 auch in einer
    /// 512er-Pattern als durchlaufendes Riff (32 Wiederholungen); ist die
    /// Patternlänge kürzer als das Muster, ist es entsprechend nur dessen
    /// Anfang (Klemmen). Wie die übrigen Werkzeuge (Rampe, Skalieren,
    /// Interpolieren) ersetzt es das Raster: erst `clear`, dann `insert` mit
    /// dem eingestellten Instrument. Die Tonzeile ist [`EUCLID_PITCH`], der
    /// Skalenfilter bleibt außen vor — die Zeile ist ein Schlagzeug-Schritt,
    /// kein Ton (sonst wanderten die Noten auf eine andere Rasterzeile).
    fn euclid_setzen(&mut self) {
        let muster = self.euclid_muster();
        if muster.iter().all(|zelle| !*zelle) {
            self.status = "Euklid: kein Schlag im Muster — Schläge auf 1 stellen.".to_string();
            return;
        }
        self.schnappschuss();
        let schritte = self.pattern().steps_per_cycle().max(1);
        let instrument = self.instrument;
        self.pattern_mut().clear();
        for step in 0..schritte {
            if muster[step as usize % muster.len()] {
                self.pattern_mut()
                    .insert(raster_note(step, instrument, EUCLID_PITCH));
            }
        }
        // Nur ein Probe-Ton: das Muster kann hunderte Noten setzen, alle
        // gleichzeitig wären auf dem Handy nur ein Knall.
        self.play(EUCLID_PITCH, self.velocity);
        self.note_klingen(EUCLID_PITCH, instrument, 0);
        let noten = self.pattern().notes().len();
        self.status = format!(
            "Euklid {}/{}: {noten} Noten (Muster wiederholt, {})",
            self.euclid_schlag,
            self.euclid_schritte,
            note_name(EUCLID_PITCH)
        );
    }

    /// Den Laub-Rhythmus (Laub-Tab) ins aktuelle Raster schreiben — wie das
    /// Euklid-Muster: erst `clear`, dann je Schritt eine Note.
    ///
    /// `takt` ist je Schritt `(Schrittnummer im Takt, Instrument)`: die
    /// Schrittnummern kommen aus `jamcore::laub::schritte_aus_abstaenden`
    /// (0..Raster), das Instrument aus der Blattfarbe (Farbklasse 0…3 →
    /// Instrument 0…3; bei der Klangquelle „Slices" sind das die ersten vier
    /// Slices). Die Note bekommt dieselbe Farbklasse auch als **Kanal**
    /// ([`laub_note`]): 0…3 → Tracker-Kanal 1–4.
    ///
    /// **Zeitraster:** Ein Takt belegt [`RhythmPanel::feinheit`] Zeilen (16 =
    /// 1/16, 32 = 1/32, 64 = 1/64), nicht mehr eine Zeile je Rasterzelle —
    /// sonst wäre ein 128er-Raster acht Takte lang (der „nur jede vierte
    /// Zeile“-Klang). Die Abbildung macht `jamcore::laub::raster_auf_zeilen`;
    /// der Takt wird alle `feinheit` Zeilen bis zur Patternlänge wiederholt.
    ///
    /// **2 Patterns:** Ist [`RhythmPanel::zwei_patterns`] an, geht die erste
    /// Hälfte der Zeilen in den aktuellen Slot und der Rest in den nächsten
    /// (Slot+1, bei 8 → 0). Der zweite Slot wird vorher geleert. Rückgabe:
    /// Gesamtzahl der Noten beider Slots.
    pub fn laub_setzen(&mut self, takt: &[(u32, u8)], raster: u16) -> usize {
        self.schnappschuss();
        let laenge = self.pattern().steps_per_cycle().max(1);
        let feinheit = self.feinheit.max(1);
        let zwei = self.zwei_patterns;
        let slot = self.slot.min(self.slots.len().saturating_sub(1));
        let naechster = (slot + 1) % self.slots.len();
        let haelfte = if zwei { laenge / 2 } else { laenge };
        self.slots[slot].clear();
        if zwei {
            // Beide Slots gleich lang: eine alte kürzere Länge im Nachbarslot
            // würde die zweite Hälfte sonst umbrechen statt aufnehmen.
            self.slots[naechster].set_steps_per_cycle(laenge);
            self.slots[naechster].clear();
        }
        let (mut erste, mut zweite) = (0usize, 0usize);
        // Takt für Takt an die Zeile `start` schreiben; bei „2 Patterns“ wird
        // jede Note an der Halbmarke in den nächsten Slot umgehängt.
        for start in (0..laenge).step_by(feinheit as usize) {
            for (zeile, instrument) in
                jamcore::laub::raster_auf_zeilen(takt, raster, feinheit, laenge, start)
            {
                let (ziel_slot, ziel_zeile) = if zwei && zeile >= haelfte {
                    (naechster, zeile - haelfte)
                } else {
                    (slot, zeile)
                };
                // Farbe → Instrument **und** Kanal: jede Farbklasse bekommt
                // ihre Tracker-Spalte (0…3 → Kanal 1–4), siehe `laub_note`.
                self.slots[ziel_slot].insert(laub_note(ziel_zeile, instrument));
                if ziel_slot == slot {
                    erste += 1;
                } else {
                    zweite += 1;
                }
            }
        }
        if zwei {
            info!(
                "laub: 2 Patterns: {erste} + {zweite} Noten (Slots {}, {})",
                slot + 1,
                naechster + 1
            );
        }
        // Klare Rueckmeldung (Nutzerwunsch): Pattern, Kette und der naechste
        // Schritt. Ohne den Hinweis blieb nach dem Laub-Rendern unklar, wo die
        // Noten liegen und wie man sie hoert.
        let ziel = if zwei {
            format!("Patterns {} + {}", slot + 1, naechster + 1)
        } else {
            format!("Pattern {}", slot + 1)
        };
        self.status = format!(
            "Laub: {} Noten in {ziel} (Kette {}) — „Rendern“ oder „Alles hören“ spielt sie",
            erste + zweite,
            self.kette
        );
        erste + zweite
    }

    /// Schreibt je Gruppe einen Takt ins jeweilige Pattern (Slot `start + i`,
    /// 0…7; nach Slot 8 geht es bei Slot 0 weiter). Rückgabe: `(Slot, Noten)`
    /// je Gruppe — der Slot **0-basiert**, wie die Slots im Panel; die Anzeige
    /// zählt 1-basiert.
    ///
    /// Jeder Ziel-Slot wird vorher geleert und auf die aktuelle Patternlänge
    /// gesetzt. Geschrieben wird **ein** Takt ab Zeile 0, nicht wie bei
    /// [`RhythmPanel::laub_setzen`] über die ganze Länge wiederholt: die
    /// Gruppen sind gedacht als mehrere kurze Patterns, die der Nutzer einzeln
    /// hört und weiterverarbeitet. Noten bekommen wie beim Laub-Rendern ihre
    /// Farbklasse als Kanal (siehe [`laub_note`]).
    pub fn laub_gruppen_setzen(
        &mut self,
        gruppen: &[Vec<(u32, u8)>],
        raster: u16,
    ) -> Vec<(usize, usize)> {
        self.schnappschuss();
        let laenge = self.pattern().steps_per_cycle().max(1);
        let feinheit = self.feinheit.max(1);
        let start = self.slot.min(self.slots.len().saturating_sub(1));
        let mut raus = Vec::with_capacity(gruppen.len());
        for (i, takt) in gruppen.iter().take(self.slots.len()).enumerate() {
            let slot = (start + i) % self.slots.len();
            self.slots[slot].set_steps_per_cycle(laenge);
            self.slots[slot].clear();
            for (zeile, farbe) in jamcore::laub::raster_auf_zeilen(takt, raster, feinheit, laenge, 0)
            {
                self.slots[slot].insert(laub_note(zeile, farbe));
            }
            raus.push((slot, self.slots[slot].notes().len()));
        }
        if !raus.is_empty() {
            let teile: Vec<String> = raus
                .iter()
                .map(|(slot, noten)| format!("Pattern {} ({noten})", slot + 1))
                .collect();
            let gesamt: usize = raus.iter().map(|(_, n)| *n).sum();
            self.status = format!(
                "Laub-Gruppen: {gesamt} Noten in {} (Kette {}) — „Rendern“ oder „Alles hören“ spielt sie",
                teile.join(", "),
                self.kette
            );
        }
        raus
    }

    /// Laub-Taktgruppe: jeder Takt ist `feinheit` Zeilen lang und folgt
    /// direkt auf den vorigen (Nutzer: „Takt besteht aus verschiedenen
    /// Abständen, Gruppen zu Taktgruppen zuordnen“). Passt die Folge nicht
    /// mehr in ein Pattern, geht es im **nächsten Slot** weiter (Ring 8);
    /// ein Takt wird nie über die Pattern-Grenze zerschnitten.
    /// Rückgabe: (Slot, Noten) je benutztem Slot, in Schreibreihenfolge.
    pub fn laub_taktgruppe_setzen(
        &mut self,
        takte: &[Vec<(u32, u8)>],
        raster: u16,
    ) -> Vec<(usize, usize)> {
        let laenge = self.pattern().steps_per_cycle().max(1);
        let feinheit = self.feinheit.max(1).min(laenge);
        let anzahl = self.slots.len();
        let start = self.slot.min(anzahl - 1);
        if takte.is_empty() || laenge == 0 {
            return Vec::new();
        }
        self.schnappschuss();
        let mut raus: Vec<(usize, usize)> = Vec::new();
        let mut slot_index = start;
        let mut versatz: u16 = 0;
        for takt in takte {
            // Kein Takt über die Pattern-Grenze: dann beginnt der nächste
            // Takt im nächsten Slot (Ring).
            if versatz + feinheit > laenge {
                slot_index += 1;
                versatz = 0;
            }
            let slot = slot_index % anzahl;
            if !raus.iter().any(|(s, _)| *s == slot) {
                self.slots[slot].set_steps_per_cycle(laenge);
                self.slots[slot].clear();
                raus.push((slot, 0));
            }
            for (zeile, farbe) in
                jamcore::laub::raster_auf_zeilen(takt, raster, feinheit, laenge, versatz)
            {
                self.slots[slot].insert(laub_note(zeile, farbe));
            }
            let idx = raus.iter().position(|(s, _)| *s == slot).unwrap_or(0);
            raus[idx].1 = self.slots[slot].notes().len();
            versatz += feinheit;
            if versatz >= laenge {
                slot_index += 1;
                versatz = 0;
            }
        }
        if !raus.is_empty() {
            let teile: Vec<String> = raus
                .iter()
                .map(|(slot, noten)| format!("Pattern {} ({noten})", slot + 1))
                .collect();
            let gesamt: usize = raus.iter().map(|(_, n)| *n).sum();
            self.status = format!(
                "Laub-Taktgruppe: {gesamt} Noten in {} (Kette {}) — „Rendern“ oder „Alles hören“ spielt sie",
                teile.join(", "),
                self.kette
            );
        }
        raus
    }

    /// Sinus-Interpolation wie `D`+`3` im Desktop-Recorder: zwischen den
    /// bestehenden Noten schwingende Zwischentöne einfügen.
    /// Alle Noten im Schrittbereich löschen (Desktop: `E`/remove-Teil).
    fn bereich_loeschen_anwenden(&mut self) {
        let (von, bis) = (self.bereich_von, self.bereich_bis);
        // Nur schnappschussen, wenn wirklich etwas im Bereich liegt — sonst
        // wuerde ein wirkungsloser Klick den letzten echten Undo-Stand
        // ueberschreiben.
        let betroffen = self
            .pattern()
            .notes()
            .iter()
            .any(|n| n.step >= von && n.step <= bis);
        if betroffen {
            self.schnappschuss();
        }
        let geloescht = self.pattern_mut().bereich_loeschen(von, bis);
        self.status = format!(
            "{geloescht} Noten gelöscht (Schritte {}–{})",
            von + 1,
            bis + 1
        );
    }

    /// Noten auf den ganzen Zyklus strecken (Desktop: `scale_to_range`).
    fn skalieren(&mut self) {
        let steps = self.pattern().steps_per_cycle().max(1);
        let noten: Vec<transformation::Klangpunkt> = self
            .pattern()
            .notes()
            .iter()
            .map(|n| (n.step as f32 / steps as f32, n.pitch))
            .collect();
        if noten.len() < 2 {
            self.status = "Skalieren braucht mindestens zwei Noten.".to_string();
            return;
        }
        self.schnappschuss();
        let out = transformation::skalieren_auf_bereich(&noten, 0.0, 1.0);
        let instrument = self.instrument;
        self.pattern_mut().clear();
        for (t, pitch) in out {
            let step = ((t * steps as f32).round() as i64).rem_euclid(steps as i64) as u16;
            self.pattern_mut()
                .insert(raster_note(step, instrument, pitch.clamp(0, 127)));
        }
        self.status = format!(
            "Auf den Zyklus gestreckt: {} Noten",
            self.pattern().notes().len()
        );
    }

    fn interpolieren(&mut self) {
        let steps = self.pattern().steps_per_cycle().max(1);
        let noten: Vec<transformation::Klangpunkt> = self
            .pattern()
            .notes()
            .iter()
            .map(|n| (n.step as f32 / steps as f32, n.pitch))
            .collect();
        if noten.len() < 2 {
            self.status = "Interpolation braucht mindestens zwei Noten.".to_string();
            return;
        }
        self.schnappschuss();
        let out = transformation::sinus_interpolieren(
            &noten,
            0,
            noten.len() - 1,
            self.interp_schritte.max(1),
            self.interp_faktor,
            self.interp_amplitude,
            self.interp_kurve,
        );
        let instrument = self.instrument;
        self.pattern_mut().clear();
        for (t, pitch) in out {
            let step = ((t * steps as f32).round() as i64).rem_euclid(steps as i64) as u16;
            self.pattern_mut()
                .insert(raster_note(step, instrument, pitch.clamp(0, 127)));
        }
        self.status = format!("Interpoliert: {} Noten", self.pattern().notes().len());
    }

    pub fn follow(&self) -> bool {
        self.follow
    }

    /// Aktuelles Raster (fuer das Tagebuch und spaetere Exporte).
    pub fn pattern(&self) -> &Pattern {
        &self.slots[self.slot.min(self.slots.len().saturating_sub(1))]
    }

    /// Aktuelles Raster veraenderbar (fuer die Takt-Maske).
    pub fn pattern_mut(&mut self) -> &mut Pattern {
        let slot = self.slot.min(self.slots.len().saturating_sub(1));
        &mut self.slots[slot]
    }

    /// Schritt aus dem laufenden Modul (Follow-Modus).
    pub fn set_follow_step(&mut self, step: u16) {
        self.current_step = step % self.pattern().steps_per_cycle().max(1);
    }

    /// Eigene Uhr, wenn der Follow-Modus aus ist.
    /// "Rendern" wie die Taste o im Desktop-Recorder: spielt das Raster in
    /// Schleife im Raster der eingestellten Feinheit (1/16, 1/32, 1/64), sendet
    /// je Note `note_on` per OSC und lässt sie lokal klingen. Optional
    /// startet/stoppt es den Renoise-Transport.
    fn rendern_starten(&mut self) {
        if self.render_laeuft {
            return;
        }
        let kette = self.kette.clamp(1, self.slots.len());
        let noten_roh: Vec<Note> = {
            // Kette: die ersten `kette` Patterns hintereinander (Taktgruppen
            // hörbar machen). Jedes Pattern bringt seine eigene Länge mit.
            if kette == 1 {
                self.pattern().notes().to_vec()
            } else {
                let mut alle = Vec::new();
                let mut versatz = 0u16;
                for slot in self.slots.iter().take(kette) {
                    let laenge = slot.steps_per_cycle().max(1);
                    for n in slot.notes() {
                        let mut n = n.clone();
                        n.step = n.step.saturating_add(versatz);
                        alle.push(n);
                    }
                    versatz = versatz.saturating_add(laenge);
                }
                alle
            }
        };
        // Die Laenge des Zyklus fuer den Render-Thread: die gemeinsame
        // Patternlaenge (frueher fest 16) — bei einer Kette die Summe.
        let schritte = self
            .slots
            .iter()
            .take(kette)
            .map(|p| p.steps_per_cycle().max(1))
            .fold(0u16, |a, b| a.saturating_add(b))
            .max(1);
        if noten_roh.is_empty() {
            self.status = "Raster ist leer - nichts zu rendern.".to_string();
            return;
        }
        // Kanaele nach `jamcore::kanal::verteile`: gleiches Sample bleibt im
        // Kanal, ein freier Kanal geht der Uebernahme vor. Die Audioausgabe
        // bleibt polyphon; die Kanaele stehen im Log/Status und in der
        // Tracker-Spalte der Noten.
        let sample_zuerst = self.kanal_sample_zuerst;
        let zuordnung = kanaele_je_note(
            &noten_roh,
            self.feinheit,
            self.render_bpm,
            sample_zuerst,
        );
        let noten: Vec<(u16, i32, u8, u8)> = noten_roh
            .iter()
            .zip(&zuordnung)
            .map(|(n, kanal)| (n.step, n.pitch, n.instrument, *kanal))
            .collect();
        let anzahl = noten.len();
        let kanaele = kanaele_anzahl(&noten);
        let (host, port) = self.target.clone();
        let bpm = self.render_bpm.max(20.0);
        let transport = self.transport_mit;
        let klangquelle = self.klangquelle;
        let welle = self.welle;
        // Feinheit: 16 Zeilen je Takt = 16tel, 32 = 32tel, 64 = 64tel. Die
        // Zeilendauer ist 60/bpm / (feinheit/4) — bei 1/16 wie bisher 16tel.
        let feinheit = self.feinheit.max(1);
        let modul = self.modul_quelle.clone();
        let stopp = Arc::new(AtomicBool::new(false));
        let schritt = Arc::new(AtomicU16::new(0));
        let stopp_c = Arc::clone(&stopp);
        let schritt_c = Arc::clone(&schritt);
        let thread = std::thread::Builder::new()
            .name("jam-render".to_string())
            .spawn(move || {
                let out = OscOut::new(&host, port).ok();
                if transport {
                    if let Some(o) = out.as_ref() {
                        let _ = Renoise::new(o).transport_start(7000);
                    }
                }
                let mut i: u16 = 0;
                let mut klang_gemeldet = false;
                info!(
                    "tracker: Wiedergabe {anzahl} Noten auf {kanaele} Kanälen (polyphon, {})",
                    if sample_zuerst {
                        "Sample zuerst"
                    } else {
                        "frei zuerst"
                    }
                );
                while !stopp_c.load(Ordering::Relaxed) {
                    let start = Instant::now();
                    // Alle Noten dieses Schritts klingen — auch mehrere auf
                    // demselben Schritt aus verschiedenen Kanälen. Die
                    // Klangwege nehmen keinen Kanal entgegen: das Modul vergibt
                    // je Note einen freien Modul-Kanal, der Mixer je Note eine
                    // Stimme; deshalb steht der Kanal nur im Log/Status.
                    for (s, pitch, instrument, kanal) in &noten {
                        if *s == i {
                            let gespielt = match klangquelle {
                                Klangquelle::Modul => {
                                    crate::slicer::instrument_sample_spielen(
                                        *instrument as usize,
                                        *pitch,
                                    ) || modul_gespielt(&modul, *pitch)
                                }
                                Klangquelle::Slices => {
                                    crate::slicer::slice_spielen((*instrument).max(0) as usize)
                                }
                                Klangquelle::Welle => false,
                            };
                            if !gespielt {
                                crate::klang::spielen_welle(*pitch, welle);
                            }
                            if !klang_gemeldet {
                                let name = match (gespielt, klangquelle) {
                                    (true, Klangquelle::Modul) => "Modul-Samples",
                                    (true, Klangquelle::Slices) => "Slices",
                                    _ => "Tongenerator",
                                };
                                info!(
                                    "Klang: {name} (erster Ton auf Kanal {})",
                                    *kanal as usize + 1
                                );
                                klang_gemeldet = true;
                            }
                            if let Some(o) = out.as_ref() {
                                let _ = Renoise::new(o).note_on(7000, *pitch, 100);
                            }
                        }
                    }
                    schritt_c.store(i, Ordering::Relaxed);
                    i = (i + 1) % schritte;
                    // Zeilendauer bei bpm: 60/bpm / (feinheit/4) Sekunden.
                    let ziel = (60_000.0 * 4.0 / (bpm * feinheit as f32)) as u64;
                    let rest = ziel.saturating_sub(start.elapsed().as_millis() as u64);
                    std::thread::sleep(Duration::from_millis(rest));
                }
                if transport {
                    if let Some(o) = out.as_ref() {
                        let _ = Renoise::new(o).transport_stop(7000);
                    }
                }
            });
        match thread {
            Ok(thread) => {
                self.render_thread = Some(thread);
                self.render_stopp = Some(stopp);
                self.render_schritt = schritt;
                self.render_laeuft = true;
                self.status = format!(
                    "Rendern läuft ({anzahl} Noten auf {kanaele} Kanälen, {kette} Pattern(s), {bpm:.0} BPM)"
                );
                info!(
                    "Rendern gestartet ({anzahl} Noten auf {kanaele} Kanälen, {kette} Pattern(s))"
                );
            }
            Err(e) => self.status = format!("Rendern-Thread: {e}"),
        }
    }

    /// Spielt den aktuellen Slot **einmal** ab, auf die nächste Rasterzeile
    /// (1/16, 1/32 oder 1/64) der eigenen Uhr quantisiert (Desktop: `H` →
    /// `play4`). Läuft parallel, damit die Oberfläche bedienbar bleibt.
    fn slot_spielen(&mut self) {
        if self.render_laeuft {
            self.rendern_stoppen();
        }
        let noten_roh: Vec<Note> = self.pattern().notes().to_vec();
        if noten_roh.is_empty() {
            self.status = "Slot ist leer.".to_string();
            return;
        }
        // Kanäle wie beim Rendern vergeben (Sample bleibt im Kanal, freier
        // Kanal vor Übernahme); die Ausgabe selbst bleibt polyphon.
        let zuordnung = kanaele_je_note(
            &noten_roh,
            self.feinheit,
            self.render_bpm,
            self.kanal_sample_zuerst,
        );
        let mut noten: Vec<(u16, i32, u8, u8)> = noten_roh
            .iter()
            .zip(&zuordnung)
            .map(|(n, kanal)| (n.step, n.pitch, n.instrument, *kanal))
            .collect();
        // Nach (Schritt, Kanal) sortieren: die Noten eines Schritts aus allen
        // Kanälen werden nacheinander **alle** gespielt (polyphon).
        noten.sort_by_key(|(s, _, _, kanal)| (*s, *kanal));
        let bpm = self.render_bpm.max(20.0);
        let (host, port) = self.target.clone();
        let basis = self.clock;
        let klangquelle = self.klangquelle;
        let welle = self.welle;
        let modul = self.modul_quelle.clone();
        // Eine Zeile ist 1/feinheit eines Takts (1/16, 1/32 oder 1/64).
        let schrittzeit = 60.0 / bpm / (self.feinheit.max(1) as f32 / 4.0);
        let anzahl = noten.len();
        let kanaele = kanaele_anzahl(&noten);
        let ergebnis = std::thread::Builder::new()
            .name("jam-slot".to_string())
            .spawn(move || {
                // Auf die naechste Rasterzeile des eigenen Rasters warten.
                let verstrichen = basis.elapsed().as_secs_f32();
                let warten = schrittzeit - (verstrichen % schrittzeit);
                if warten > 0.0 && warten < schrittzeit {
                    std::thread::sleep(Duration::from_secs_f32(warten));
                }
                let out = OscOut::new(&host, port).ok();
                let start = Instant::now();
                // Alle Noten, auch mehrere auf demselben Schritt (Kanäle):
                // die Wiedergabe ist polyphon, der Kanal steht im Log/Status.
                for (step, pitch, instrument, kanal) in &noten {
                    let ziel = *step as f32 * schrittzeit;
                    let rest = ziel - start.elapsed().as_secs_f32();
                    if rest > 0.0 {
                        std::thread::sleep(Duration::from_secs_f32(rest));
                    }
                    let gespielt = match klangquelle {
                        Klangquelle::Modul => {
                            crate::slicer::instrument_sample_spielen(
                                *instrument as usize,
                                *pitch,
                            ) || modul_gespielt(&modul, *pitch)
                        }
                        Klangquelle::Slices => {
                            crate::slicer::slice_spielen((*instrument).max(0) as usize)
                        }
                        Klangquelle::Welle => false,
                    };
                    if !gespielt {
                        crate::klang::spielen_welle(*pitch, welle);
                    }
                    if let Some(o) = out.as_ref() {
                        let _ = Renoise::new(o).note_on(7000, *pitch, 100);
                    }
                    log::debug!(
                        "Slot: Schritt {} Kanal {} {}",
                        *step + 1,
                        *kanal as usize + 1,
                        note_name(*pitch)
                    );
                }
            });
        match ergebnis {
            Ok(_) => {
                self.status = format!(
                    "Slot {} gespielt ({} Noten auf {} Kanälen, quantisiert)",
                    self.slot + 1,
                    anzahl,
                    kanaele
                );
                info!("Slot {} einmal gespielt", self.slot + 1);
            }
            Err(e) => self.status = format!("Slot-Thread: {e}"),
        }
    }

    /// „Kanäle verteilen“: die vorhandenen Noten des aktuellen Patterns
    /// nachträglich neu ordnen — Schritte und Tonhöhen bleiben, nur der Kanal
    /// ändert sich. Sample = Instrument, Dauer = Tonlänge des Notenwerts; die
    /// Regeln stehen in `jamcore::kanal`. Die Audioausgabe ist davon
    /// unabhängig (polyphon); der Tracker zeigt danach je Sample möglichst
    /// eine Spalte, und ein Kanal bleibt bei seinem Sample.
    fn kanaele_verteilen(&mut self) {
        let noten: Vec<Note> = self.pattern().notes().to_vec();
        if noten.is_empty() {
            self.status = "Keine Noten — nichts zu verteilen.".to_string();
            return;
        }
        self.schnappschuss();
        let zuordnung = kanaele_je_note(
            &noten,
            self.feinheit,
            self.render_bpm,
            self.kanal_sample_zuerst,
        );
        // Kennzahlen für Status/Log: verschiedene Samples und belegte Kanäle.
        let mut samples: Vec<u8> = noten.iter().map(|n| n.instrument).collect();
        samples.sort_unstable();
        samples.dedup();
        let mut kanaele: Vec<u8> = zuordnung.clone();
        kanaele.sort_unstable();
        kanaele.dedup();
        // Notenliste neu aufbauen statt `Pattern::insert` zu benutzen:
        // `insert` räumt den **ganzen** Schritt (auch andere Kanäle) und würde
        // Akkorde zerlegen. `tracker::setzen` trifft nur die Zelle
        // (Schritt + Kanal), die Notenreihenfolge bleibt (step, pitch).
        let slot = self.slot;
        self.slots[slot].clear();
        for (note, kanal) in noten.iter().zip(&zuordnung) {
            setzen(
                &mut self.slots[slot],
                Note {
                    channel: *kanal,
                    ..*note
                },
            );
        }
        let reihenfolge = if self.kanal_sample_zuerst {
            "Sample zuerst"
        } else {
            "frei zuerst"
        };
        self.status = format!(
            "Kanäle verteilt ({} Noten, {} Samples, {} Kanäle, {reihenfolge})",
            noten.len(),
            samples.len(),
            kanaele.len()
        );
        info!(
            "rhythm: Kanaele verteilt ({} Noten, {} Samples, {} Kanaele)",
            noten.len(),
            samples.len(),
            kanaele.len()
        );
    }

    fn rendern_stoppen(&mut self) {
        if let Some(stopp) = self.render_stopp.take() {
            stopp.store(true, Ordering::Relaxed);
        }
        if let Some(thread) = self.render_thread.take() {
            let _ = thread.join();
        }
        self.render_laeuft = false;
        self.status = "Rendern gestoppt.".to_string();
        info!("Rendern gestoppt");
    }

    fn tick_own_clock(&mut self) {
        // Beim Rendern zeigt der Playhead die laufende Abspielposition.
        let schritte = self.pattern().steps_per_cycle().max(1);
        if self.render_laeuft {
            self.current_step = self.render_schritt.load(Ordering::Relaxed) % schritte;
            return;
        }
        if self.follow {
            return;
        }
        let elapsed = self.clock.elapsed().as_secs_f32();
        let beats = elapsed * self.bpm.max(20.0) / 60.0;
        // Modulo vor dem Cast: ein `as u16` wuerde nach zwei Stunden
        // sattigieren und die Uhr einfrieren. Zeilen je Takt 16/32/64
        // (feinheit/4 Schritte je Schlag).
        let steps = (beats * (self.feinheit.max(1) as f32 / 4.0)).max(0.0);
        self.current_step = if steps.is_finite() {
            (steps as i64 % schritte as i64) as u16
        } else {
            0
        };
    }

    fn play(&mut self, pitch: i32, velocity: u8) {
        let (host, port) = self.target.clone();
        let result = OscOut::new(&host, port)
            .and_then(|out| Renoise::new(&out).note_on(7000, pitch, velocity as i32));
        match result {
            Ok(used) => info!(
                "Note {} vel {} -> {}:{}",
                note_name(pitch),
                velocity,
                host,
                used
            ),
            Err(e) => {
                self.status = format!("OSC-Fehler: {e}");
                info!("OSC-Fehler: {e}");
            }
        }
    }

    /// Stufen der gewaehlten Tonleiter (None = kein Filter).
    fn skala_stufen(&self) -> Option<&'static [i32]> {
        match self.skala {
            1 => Some(&transformation::MOLL),
            2 => Some(&transformation::DUR),
            3 => Some(&transformation::PENTATONIK_MOLL),
            4 => Some(&transformation::PENTATONIK_DUR),
            _ => None,
        }
    }

    /// Rundet einen Ton auf die gewaehlte Skala (ohne Filter unveraendert).
    fn skaliere_pitch(&self, pitch: i32) -> i32 {
        match self.skala_stufen() {
            Some(stufen) => {
                let toene = transformation::skala_toene(self.grundton, stufen, 4);
                transformation::auf_skala(pitch, &toene)
            }
            None => pitch,
        }
    }

    /// Wendet eine Transformation auf den aktuellen Slot an.
    fn noten_transformieren(&mut self, f: impl FnOnce(&[Note]) -> Vec<Note>) {
        self.schnappschuss();
        let neu = f(self.pattern().notes());
        let slot = self.slot;
        self.slots[slot].clear();
        for note in neu {
            self.slots[slot].insert(note);
        }
        self.status = format!(
            "Pattern {}: {} Noten",
            self.slot + 1,
            self.pattern().notes().len()
        );
    }

    /// Presenter-Ansicht wie die Taste J im Desktop-Recorder: Tempo groß,
    /// Notenliste und eine Zeitachse mit allen Noten (x = Schritt, y = Tonhöhe).
    fn presenter_zeichnen(&mut self, ui: &mut egui::Ui) {
        use egui::{pos2, Color32, Stroke};

        ui.separator();
        ui.label(
            RichText::new(format!("{:.0} BPM", self.render_bpm))
                .size(26.0)
                .strong(),
        );
        let noten: Vec<(u16, i32, String)> = self
            .pattern()
            .notes()
            .iter()
            .map(|n| (n.step, n.pitch, note_name(n.pitch)))
            .collect();
        let liste: Vec<&str> = noten.iter().map(|(_, _, name)| name.as_str()).collect();
        ui.label(format!(
            "Noten: {} (Pattern {})",
            if liste.is_empty() {
                "—".to_string()
            } else {
                liste.join(" ")
            },
            self.slot + 1
        ));

        let breite = ui.available_width();
        let hoehe = 130.0;
        let (response, painter) =
            ui.allocate_painter(egui::vec2(breite, hoehe), egui::Sense::hover());
        let rect = response.rect;
        let grau = Color32::from_rgb(60, 60, 66);
        // Die Rasterweite ist die gewaehlte Patternlaenge, nicht die Vorgabe 16.
        let schritte = self.pattern().steps_per_cycle().max(1) as f32;
        // Rasterlinien: bei langer Patternlaenge engere Staerke, sonst waeren
        // 128 Linien nur noch ein Grauschmier.
        let linien_abstand = ((schritte / 16.0).round() as u32).max(1);
        for s in (0..=schritte as u32).step_by(linien_abstand as usize) {
            let x = rect.left() + breite * s as f32 / schritte;
            painter.line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                Stroke::new(1.0, grau),
            );
        }
        if !noten.is_empty() {
            let (min_p, max_p) = noten
                .iter()
                .fold((127, 0), |(mn, mx), (_, p, _)| (mn.min(*p), mx.max(*p)));
            let spanne = (max_p - min_p).max(1) as f32;
            for (step, pitch, _) in &noten {
                let x = rect.left() + breite * *step as f32 / schritte;
                let y = rect.bottom() - (*pitch - min_p) as f32 / spanne * (hoehe - 16.0) - 8.0;
                painter.circle_filled(pos2(x, y), 5.0, Color32::from_rgb(90, 200, 120));
            }
        }
        // Abspielposition.
        let x = rect.left() + breite * self.current_step as f32 / schritte;
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(2.0, Color32::from_rgb(245, 245, 255)),
        );
    }

    fn toggle_pad(&mut self, step: u16, pitch: i32) {
        let pitch = self.skaliere_pitch(pitch);
        // Aufnahme: der Tipp landet auf dem gerade laufenden Schritt (wie
        // die Recorder-Tasten im Desktop), nicht auf der angefassten Zelle.
        if self.aufnahme && self.render_laeuft {
            let ziel = self.render_schritt.load(Ordering::Relaxed)
                % self.pattern().steps_per_cycle().max(1);
            if !self.pattern_mut().has_at(ziel, pitch) {
                self.schnappschuss();
                let instrument = self.instrument;
                self.pattern_mut().insert(raster_note(ziel, instrument, pitch));
                self.play(pitch, self.velocity);
                self.note_klingen(pitch, instrument, 0);
                self.status = format!(
                    "Aufnahme: Schritt {} <- {} ({} Noten)",
                    ziel + 1,
                    note_name(pitch),
                    self.pattern().notes().len()
                );
            }
            return;
        }
        // Normalfall: Note setzen (mit gewaehltem Instrument) oder entfernen.
        self.schnappschuss();
        let instrument = self.instrument;
        let added = if self.pattern_mut().has_at(step, pitch) {
            self.pattern_mut().toggle(step, pitch)
        } else {
            self.pattern_mut().insert(raster_note(step, instrument, pitch));
            true
        };
        if added {
            self.play(pitch, self.velocity);
            // Der Ton klingt auch lokal (im Demo-Video hoerbar) — mit der
            // gewaehlten Klangquelle.
            self.note_klingen(pitch, instrument, 0);
            self.status = format!(
                "Schritt {}: {} ({} Noten)",
                step + 1,
                note_name(pitch),
                self.pattern().notes().len()
            );
        } else {
            self.status = format!(
                "Schritt {} frei ({} Noten)",
                step + 1,
                self.pattern().notes().len()
            );
        }
    }

    /// Rasterzelle mit Buendelung: setzt oder loescht **alle** Schritte des
    /// Buendels (bei Buendelung 1 genau wie [`Self::toggle_pad`]). In der
    /// Aufnahme zaehlt weiter der laufende Schritt.
    fn pad_buendel(&mut self, von: u16, bis: u16, pitch: i32) {
        // Tracker-Ansicht: die Pads sind die Notentastatur — der Ton landet in
        // der **gewählten** Zelle (Schritt/Kanal), nicht auf dem angetippten
        // Rasterschritt.
        if self.ansicht == Ansicht::Tracker {
            self.tracker_setzen(pitch);
            return;
        }
        if von == bis || (self.aufnahme && self.render_laeuft) {
            self.toggle_pad(von, pitch);
            return;
        }
        let pitch = self.skaliere_pitch(pitch);
        self.schnappschuss();
        let gesetzt = self.pattern_mut().toggle_buendel(von, bis, pitch);
        if gesetzt {
            self.play(pitch, self.velocity);
            self.note_klingen(pitch, self.instrument, 0);
        }
        self.status = format!(
            "{} Schritte {}: {} ({} Noten)",
            bis - von + 1,
            if gesetzt { "gesetzt" } else { "geloescht" },
            note_name(pitch),
            self.pattern().notes().len()
        );
    }

    /// Ansicht des Rhythmus-Tabs umschalten (Umschalter „Raster ⇄ Tracker“).
    fn ansicht_umschalten(&mut self, ansicht: Ansicht) {
        if self.ansicht == ansicht {
            return;
        }
        self.ansicht = ansicht;
        self.tracker_scroll = Some(self.tracker_schritt);
        match ansicht {
            Ansicht::Raster => {
                info!("tracker: Ansicht Raster");
                self.status = "Ansicht: Raster (Step-Editor, Pads setzen Rasterzellen)".to_string();
            }
            Ansicht::Tracker => {
                info!("tracker: Ansicht Tracker");
                self.status = format!(
                    "Ansicht: Tracker — Zelle antippen, dann Note mit den Tasten C…B (Kanal {})",
                    self.tracker_kanal as usize + 1
                );
            }
        }
    }

    /// Die Note der gewählten Tracker-Zelle (`None` = leer).
    fn tracker_zelle(&self) -> Option<Note> {
        self.pattern()
            .zelle(self.tracker_schritt, self.tracker_kanal)
            .copied()
    }

    /// Zellwahl aus der Tracker-Ansicht (Tipp auf eine Zelle).
    fn tracker_zelle_waehlen(&mut self, step: u16, kanal: u8) {
        self.tracker_schritt = step % self.pattern().steps_per_cycle().max(1);
        self.tracker_kanal = kanal;
        let text = zelle_text(self.tracker_zelle().as_ref());
        info!(
            "tracker: Zelle Schritt {} Kanal {} gewählt ({})",
            self.tracker_schritt + 1,
            kanal as usize + 1,
            text
        );
        self.status = format!(
            "Zelle Schritt {} Kanal {} ({}) — Note: Tasten C…B direkt darunter, kein Scrollen",
            self.tracker_schritt + 1,
            kanal as usize + 1,
            text
        );
    }

    /// Eine Note in die **gewählte** Tracker-Zelle setzen — die Pads und
    /// Tasten des Tabs sind dort die Notentastatur (Instrument = aktuelle
    /// Auswahl, Skalenfilter wie im Raster). Eine belegte Zelle wird ersetzt.
    fn tracker_setzen(&mut self, pitch: i32) {
        self.schnappschuss();
        let pitch = self.skaliere_pitch(pitch);
        let note = Note {
            step: self.tracker_schritt,
            instrument: self.instrument,
            pitch,
            glide: false,
            channel: self.tracker_kanal,
        };
        // `setzen` ersetzt die Zelle; die Rückgabe sagt nur, ob sie vorher
        // leer war („angelegt“) — kein Fehlerfall.
        let war_leer = setzen(self.pattern_mut(), note);
        // Sofort hören (wie beim Pad im Raster) und per OSC nach draussen.
        self.play(pitch, self.velocity);
        self.note_klingen(pitch, note.instrument, note.channel);
        // Den Text aus dem Pattern lesen: er zeigt, was wirklich steht.
        let text = zelle_text(self.tracker_zelle().as_ref());
        info!(
            "tracker: Zelle Schritt {} Kanal {} <- {}",
            note.step + 1,
            note.channel as usize + 1,
            text
        );
        self.status = format!(
            "Zelle Schritt {} Kanal {} {} {} ({} Noten)",
            note.step + 1,
            note.channel as usize + 1,
            if war_leer { "gesetzt" } else { "ersetzt" },
            text,
            self.pattern().notes().len()
        );
    }

    /// Die gewählte Tracker-Zelle löschen.
    fn tracker_loeschen(&mut self) {
        let (step, kanal) = (self.tracker_schritt, self.tracker_kanal);
        if self.tracker_zelle().is_some() {
            self.schnappschuss();
        }
        if loeschen(self.pattern_mut(), step, kanal) {
            info!(
                "tracker: Zelle Schritt {} Kanal {} gelöscht",
                step + 1,
                kanal as usize + 1
            );
            self.status = format!(
                "Zelle Schritt {} Kanal {} gelöscht",
                step + 1,
                kanal as usize + 1
            );
        } else {
            self.status = format!(
                "Zelle Schritt {} Kanal {} war leer",
                step + 1,
                kanal as usize + 1
            );
        }
    }

    /// Gewählten Schritt verschieben (Knopf „Schritt ±“); die Ansicht holt die
    /// Zeile danach in den Blick.
    fn tracker_schritt_verschieben(&mut self, delta: i32) {
        let schritte = self.pattern().steps_per_cycle().max(1) as i32;
        self.tracker_schritt = (self.tracker_schritt as i32 + delta).rem_euclid(schritte) as u16;
        self.tracker_scroll = Some(self.tracker_schritt);
        self.tracker_status();
    }

    /// Gewählten Kanal verschieben (Knopf „Kanal ±“), 0…63 geklemmt.
    fn tracker_kanal_verschieben(&mut self, delta: i32) {
        self.tracker_kanal = klemme_kanal(self.tracker_kanal as i32 + delta);
        self.tracker_status();
    }

    /// Statuszeile auf die gewählte Tracker-Zelle setzen.
    fn tracker_status(&mut self) {
        let text = zelle_text(self.tracker_zelle().as_ref());
        self.status = format!(
            "Zelle Schritt {} Kanal {} ({}) — Note: Tasten C…B direkt darunter, kein Scrollen",
            self.tracker_schritt + 1,
            self.tracker_kanal as usize + 1,
            text
        );
    }

    /// Werkzeugzeile der Tracker-Ansicht: Zellwahl, löschen, verschieben,
    /// Instrument (die Instr-Knöpfe stehen weiter unten wie im Raster).
    fn tracker_werkzeuge(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let zelle = zelle_text(self.tracker_zelle().as_ref());
            ui.label(
                RichText::new(format!(
                    "Zelle Schritt {} · Kanal {} · {}",
                    self.tracker_schritt + 1,
                    self.tracker_kanal as usize + 1,
                    zelle
                ))
                .strong(),
            );
            if ui.button("Zelle löschen").clicked() {
                self.tracker_loeschen();
            }
            if ui
                .selectable_label(self.tracker_folgen, "Scrollen")
                .on_hover_text("Beim Abspielen zur laufenden Zeile scrollen")
                .clicked()
            {
                self.tracker_folgen = !self.tracker_folgen;
                info!(
                    "tracker: Scrollen {}",
                    if self.tracker_folgen { "an" } else { "aus" }
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Schritt -").clicked() {
                self.tracker_schritt_verschieben(-1);
            }
            if ui.button("Schritt +").clicked() {
                self.tracker_schritt_verschieben(1);
            }
            if ui.button("Kanal -").clicked() {
                self.tracker_kanal_verschieben(-1);
            }
            if ui.button("Kanal +").clicked() {
                self.tracker_kanal_verschieben(1);
            }
            ui.label(format!("Kanal {} / {}", self.tracker_kanal as usize + 1, KANAELE));
            // Getauschtes Sample sichtbar machen („Instr N *Slice“) und bei
            // Bedarf wieder entfernen — der Tausch kommt aus dem Slicer.
            ui.label(self.instrument_anzeige());
            if crate::slicer::hat_getauschtes_sample(self.instrument as usize)
                && ui
                    .small_button("×Sample")
                    .on_hover_text("Getauschtes Sample entfernen (zurück zum Modul-Sample)")
                    .clicked()
            {
                crate::slicer::tausch_loeschen(self.instrument as usize);
                self.status = format!("Instr {}: Sample-Tausch entfernt", self.instrument);
            }
            let noten = self
                .pattern()
                .notes()
                .iter()
                .filter(|n| n.channel == self.tracker_kanal)
                .count();
            ui.label(
                RichText::new(format!("{noten} Noten im Kanal"))
                    .small()
                    .color(Color32::GRAY),
            );
        });
        // Schnell-Tastatur: ein Tipp setzt die Note in die gewaehlte Zelle
        // (Instrument = aktuelle Auswahl). Sie steht direkt unter der Zellwahl,
        // damit die Noteneingabe ohne Scrollen zu den Pads funktioniert.
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Okt -").clicked() {
                self.tracker_oktav = (self.tracker_oktav - 12).clamp(12, 108);
            }
            ui.label(
                RichText::new(note_name(self.tracker_oktav))
                    .small()
                    .color(Color32::GRAY),
            );
            if ui.small_button("Okt +").clicked() {
                self.tracker_oktav = (self.tracker_oktav + 12).clamp(12, 108);
            }
            for (stufe, name) in NOTE_KLASSEN.iter().enumerate() {
                if ui
                    .add(egui::Button::new(*name).small())
                    .on_hover_text("Note in die gewählte Zelle setzen")
                    .clicked()
                {
                    self.tracker_setzen(self.tracker_oktav + stufe as i32);
                }
            }
        });
    }

    /// Akkord-Mini-Presets („gespeichert“ = fest im Code, siehe
    /// `akkord::PRESETS`): ein Klick setzt die Toene auf den gewaehlten
    /// Tracker-Schritt, je Ton ein Kanal. Der Grundton ist einstellbar; die
    /// Auswahl bleibt beim Umschalten erhalten.
    fn akkord_zeile(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Akkord");
            let mut grundton = self.akkord_grundton;
            if ui
                .add(
                    egui::DragValue::new(&mut grundton)
                        .range(36..=84)
                        .speed(0.5),
                )
                .changed()
            {
                self.akkord_grundton = grundton;
            }
            ui.label(RichText::new(note_name(self.akkord_grundton)).strong());
            for (i, (name, _)) in crate::akkord::PRESETS.iter().enumerate() {
                if ui
                    .add(egui::Button::new(*name).small())
                    .on_hover_text("Akkord auf den gewählten Tracker-Schritt setzen")
                    .clicked()
                {
                    self.akkord_setzen(i);
                }
            }
            ui.label(
                RichText::new(format!("auf Schritt {}", self.tracker_schritt + 1))
                    .small()
                    .color(Color32::GRAY),
            );
        });
    }

    /// Tracker-Ansicht: Zellwahl per Tipp, Werkzeugzeile und Schnell-Tastatur
    /// darüber, Akkord-Presets, dann das Raster. Steht bewusst **oben** im Tab
    /// (vor den vielen Werkzeugzeilen), damit die Noteneingabe nach der
    /// Zellwahl ohne Scrollen erreichbar ist.
    fn tracker_bereich(&mut self, ui: &mut egui::Ui) {
        // Die Patternlaenge kann sich inzwischen geaendert haben (Laenge-
        // Regler, CSV/XML): die Zellwahl auf vorhandene Schritte klemmen,
        // sonst zeigte die Ansicht keine Auswahl mehr, während ein Pad
        // seine Note umgebrochen woandershin setzt.
        self.tracker_schritt = self
            .tracker_schritt
            .min(self.pattern().steps_per_cycle().saturating_sub(1));
        self.tracker_werkzeuge(ui);
        self.akkord_zeile(ui);
        // Beim Abspielen zur laufenden Zeile scrollen (an/aus) — nur beim
        // Schrittwechsel, sonst ließe sich nicht mehr selbst scrollen.
        if self.tracker_folgen && self.current_step != self.tracker_letzter_spiel {
            self.tracker_letzter_spiel = self.current_step;
            self.tracker_scroll = Some(self.current_step);
        }
        let zustand = TrackerZustand {
            schritt: self.tracker_schritt,
            kanal: self.tracker_kanal,
            spiel_schritt: self.current_step,
        };
        let mut scroll = self.tracker_scroll.take();
        let getippt = tracker::ansicht(ui, self.pattern(), &zustand, &mut scroll);
        self.tracker_scroll = scroll;
        if let Some((step, kanal)) = getippt {
            self.tracker_zelle_waehlen(step, kanal);
        }
    }

    /// Musterordner im externen App-Verzeichnis (ohne Berechtigung, per adb erreichbar).
    fn pattern_dir(&mut self) -> Option<String> {
        if self.dir.is_none() {
            let app = crate::current_app().ok()?;
            let base = crate::insets::external_files_dir(&app)?;
            let dir = std::path::PathBuf::from(base).join("patterns");
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Pattern-Ordner {}: {e}", dir.display()))
                .ok()?;
            self.dir = Some(dir.display().to_string());
        }
        self.dir.clone()
    }

    fn save_csv(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis für Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join(format!("pattern {}.csv", self.slot));
        let csv = self.pattern().to_csv();
        match self.write_atomic(&path, &csv) {
            Ok(()) => {
                self.status = format!(
                    "{} Noten -> {} ({} Bytes)",
                    self.pattern().notes().len(),
                    path.display(),
                    csv.len()
                );
                info!("CSV gesichert: {}", path.display());
            }
            Err(e) => self.status = format!("CSV-Fehler: {e}"),
        }
    }

    fn save_xml(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis für Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join(format!("pattern {}.xml", self.slot));
        let xml = self.pattern().to_renoise_xml();
        match self.write_atomic(&path, &xml) {
            Ok(()) => {
                self.status = format!("XML für Renoise -> {}", path.display());
                info!("XML geschrieben: {}", path.display());
            }
            Err(e) => self.status = format!("XML-Fehler: {e}"),
        }
    }

    /// Neben die Zieldatei schreiben und dann umbenennen: bricht das Schreiben
    /// ab, bleibt das vorherige Muster unangetastet.
    fn write_atomic(&self, path: &std::path::Path, content: &str) -> std::io::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, content)?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn load_xml(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis fuer Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join(format!("pattern {}.xml", self.slot));
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                match Pattern::from_renoise_xml(&content, self.pattern().steps_per_cycle()) {
                    Ok(loaded) => {
                        self.schnappschuss();
                        *self.pattern_mut() = loaded;
                        self.status = format!(
                            "{} Noten aus {}",
                            self.pattern().notes().len(),
                            path.display()
                        );
                        info!("XML geladen: {}", path.display());
                    }
                    Err(e) => self.status = format!("XML kaputt: {e}"),
                }
            }
            Err(e) => self.status = format!("XML fehlt: {e}"),
        }
    }

    fn load_csv(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis fuer Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join(format!("pattern {}.csv", self.slot));
        match std::fs::read_to_string(&path) {
            Ok(content) => match Pattern::from_csv(&content, self.pattern().steps_per_cycle()) {
                Ok(loaded) => {
                    self.schnappschuss();
                    *self.pattern_mut() = loaded;
                    self.status = format!(
                        "{} Noten aus {}",
                        self.pattern().notes().len(),
                        path.display()
                    );
                    info!("CSV geladen: {}", path.display());
                }
                Err(e) => self.status = format!("CSV kaputt: {e}"),
            },
            Err(e) => self.status = format!("CSV fehlt: {e}"),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.tick_own_clock();
        self.ziel_abgleichen();
        ui.ctx().request_repaint_after(Duration::from_millis(50));

        // Umschalter „Raster ⇄ Tracker“: dieselbe Pattern, zwei Ansichten.
        // Raster = Step-Editor wie bisher (Euklid, Laub, Maske, CSV/XML
        // arbeiten unverändert darauf), Tracker = 64 Kanal-Spalten wie im
        // Impulse Tracker.
        ui.horizontal(|ui| {
            ui.label("Ansicht");
            if ui
                .selectable_label(self.ansicht == Ansicht::Raster, "Raster")
                .clicked()
            {
                self.ansicht_umschalten(Ansicht::Raster);
            }
            if ui
                .selectable_label(self.ansicht == Ansicht::Tracker, "Tracker")
                .clicked()
            {
                self.ansicht_umschalten(Ansicht::Tracker);
            }
            if self.ansicht == Ansicht::Tracker {
                ui.label(
                    RichText::new("Noten: Tasten C…B unter der Zelle")
                        .small()
                        .color(Color32::GRAY),
                );
            }
            // Rückgängig steht ganz oben: nach einem Fehlgriff beim Bauen
            // (Pad, Laub, Euklid) soll man nicht erst zur Aktion zurueck.
            if ui
                .add_enabled(
                    self.rueckgaengig.is_some(),
                    egui::Button::new("Rückgängig"),
                )
                .on_hover_text("Letzte Pattern-Änderung zurücknehmen")
                .clicked()
            {
                self.rueckgaengig_anwenden();
            }
        });

        ui.horizontal(|ui| {
            // Acht Slots wie im Desktop-Recorder; gespeichert wird je Slot
            // als "pattern N.csv" (Zaehlung ab 0 wie dort).
            ui.label("Pattern");
            for i in 0..self.slots.len() {
                let text = format!("{}", i + 1);
                if ui.selectable_label(self.slot == i, text).clicked() {
                    self.slot = i;
                    self.status = format!(
                        "Pattern {} gewählt ({} Noten)",
                        i + 1,
                        self.pattern().notes().len()
                    );
                }
            }
        });
        // Austausch/Export gleich oben: im Querformat ist unten kein Platz,
        // ein Scrollen waere auf dem Touchscreen unzuverlaessig.
        ui.horizontal_wrapped(|ui| {
            if ui.button("CSV sichern").clicked() {
                self.save_csv();
            }
            if ui.button("CSV laden").clicked() {
                self.load_csv();
            }
            if ui.button("XML schreiben").clicked() {
                self.save_xml();
            }
            if ui.button("XML laden").clicked() {
                self.load_xml();
            }
            if ui.button("Schism kopieren").clicked() {
                let schism = self.pattern().to_schism();
                ui.ctx().copy_text(schism.clone());
                self.status = format!(
                    "Schism-Pattern ({} Zeilen) in der Zwischenablage",
                    self.pattern().steps_per_cycle()
                );
                info!("Schism-Pattern kopiert");
            }
        });
        // Die Tracker-Ansicht steht vor den Werkzeugzeilen (Nutzerwunsch:
        // Noteneingabe nach der Zellwahl ohne Scrollen). Raster-Werkzeuge
        // darunter aendern dasselbe Pattern; die Ansicht zeigt es sofort.
        if self.ansicht == Ansicht::Tracker {
            self.tracker_bereich(ui);
        }
        // Werkzeuge aus dem Recorder: Notenrampe (Taste 2) und
        // Sinus-Interpolation (D+3).
        ui.horizontal_wrapped(|ui| {
            ui.label("Rampe");
            ui.add(egui::DragValue::new(&mut self.rampe_anzahl).range(1..=31));
            if ui.button("Rampe").clicked() {
                self.rampe_anwenden();
            }
            ui.separator();
            ui.label("Interp.");
            ui.add(
                egui::DragValue::new(&mut self.interp_schritte)
                    .range(1..=8)
                    .prefix("n "),
            );
            ui.add(
                egui::DragValue::new(&mut self.interp_amplitude)
                    .range(0.0..=24.0)
                    .suffix(" HT"),
            );
            ui.add(egui::DragValue::new(&mut self.interp_faktor).range(0.5..=4.0));
            let kurve = ["K1", "K2", "K3"][(self.interp_kurve.clamp(1, 3) - 1) as usize];
            if ui.button(kurve).clicked() {
                self.interp_kurve = self.interp_kurve.clamp(1, 3) % 3 + 1;
            }
            if ui.button("Interpolieren").clicked() {
                self.interpolieren();
            }
        });
        // Patternlaenge in eigener Zeile — im Querformat brach sie sonst
        // mitten in den Werkzeugen um (Bündelung wurde zur Buchstabensäule).
        ui.horizontal_wrapped(|ui| {
            ui.label("Laenge");
            let mut laenge = self.pattern().steps_per_cycle() as i32;
            if ui
                .add(egui::DragValue::new(&mut laenge).range(1..=MAX_STEPS_PER_CYCLE as i32))
                .changed()
            {
                self.pattern_mut().set_steps_per_cycle(laenge as u16);
                self.status = format!("Patternlaenge: {laenge} Schritte");
            }
            // Schnellwahl für lange Laub-Renderings: 128 (Vorgabe) und
            // Vielfache davon; der aktuelle Wert bleibt hervorgehoben.
            for wert in [64u16, 128, 192, 256, 384, 512] {
                if ui
                    .selectable_label(
                        self.pattern().steps_per_cycle() == wert,
                        wert.to_string(),
                    )
                    .clicked()
                {
                    self.pattern_mut().set_steps_per_cycle(wert);
                    self.status = format!("Patternlaenge: {wert} Schritte");
                }
            }
        });
        // Anzeige-Buendelung: lange Patterns in weniger Zellen, ohne die
        // Patternlaenge zu aendern (Nutzerentscheidung 2026-10-14).
        ui.horizontal_wrapped(|ui| {
            ui.label("Bündelung");
            for wert in [1u16, 2, 4, 8, 16, 32] {
                if ui
                    .selectable_label(self.buendel == wert, wert.to_string())
                    .clicked()
                {
                    self.buendel = wert;
                    self.status = format!("Bündelung: {wert} Schritt(e) je Zelle");
                    info!("Buendelung: {wert}");
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Bereich");
            ui.add(egui::DragValue::new(&mut self.bereich_von).range(0..=MAX_STEPS_PER_CYCLE - 1));
            ui.add(egui::DragValue::new(&mut self.bereich_bis).range(0..=MAX_STEPS_PER_CYCLE - 1));
            if ui.button("löschen").clicked() {
                self.bereich_loeschen_anwenden();
            }
            ui.separator();
            if ui.button("Skalieren").clicked() {
                self.skalieren();
            }
            if ui.button("Slot spielen").clicked() {
                self.slot_spielen();
            }
        });
        // Euklid (Max-Patch `eucl-midi-in out`): „N Schläge / M Schritte“ als
        // Regler, das Muster steht als Text daneben. „Muster setzen“ schreibt
        // es als Noten (1 = Note) auf die Tonzeile EUCLID_PITCH, „Rotieren +1“
        // dreht das Muster weiter, „löschen“ räumt das Raster wieder.
        ui.horizontal_wrapped(|ui| {
            ui.label("Euklid:");
            let max = euclid::MAX_SCHRITTE as i32;
            let mut schlag = self.euclid_schlag as i32;
            if ui
                .add(egui::DragValue::new(&mut schlag).range(1..=max))
                .changed()
            {
                self.euclid_schlag = (schlag.max(1) as usize).min(euclid::MAX_SCHRITTE);
                self.status = format!(
                    "Euklid: {} Schläge / {} Schritte",
                    self.euclid_schlag, self.euclid_schritte
                );
            }
            ui.label("Schläge /");
            let mut schritte = self.euclid_schritte as i32;
            if ui
                .add(egui::DragValue::new(&mut schritte).range(1..=max))
                .changed()
            {
                self.euclid_schritte = (schritte.max(1) as usize).min(euclid::MAX_SCHRITTE);
                self.status = format!(
                    "Euklid: {} Schläge / {} Schritte",
                    self.euclid_schlag, self.euclid_schritte
                );
            }
            ui.label("Schritte");
            if ui.button("Muster setzen").clicked() {
                self.euclid_setzen();
            }
            if ui.button("Rotieren +1").clicked() {
                self.euclid_versatz = self.euclid_versatz.wrapping_add(1);
                self.status = format!(
                    "Euklid: Muster rotiert (Versatz {} Zellen) — „Muster setzen“ schreibt es ins Raster",
                    self.euclid_versatz
                );
            }
            if ui.button("löschen").clicked() {
                self.pattern_mut().clear();
                self.status = "Euklid: Raster geleert (Muster bleibt erhalten)".to_string();
            }
            ui.label(
                RichText::new(format!("Muster: {}", euclid::als_text(&self.euclid_muster())))
                    .small()
                    .color(Color32::GRAY),
            );
        });
        // Klangquelle für das gerenderte Material: Modul-Samples, Slices aus
        // dem Slicer oder Tongenerator mit Wellenform.
        ui.horizontal_wrapped(|ui| {
            ui.label("Klang");
            if ui
                .selectable_label(self.klangquelle == Klangquelle::Modul, "Modul-Samples")
                .clicked()
            {
                self.klangquelle = Klangquelle::Modul;
            }
            if ui
                .selectable_label(self.klangquelle == Klangquelle::Welle, "Tongenerator")
                .clicked()
            {
                self.klangquelle = Klangquelle::Welle;
            }
            if ui
                .selectable_label(self.klangquelle == Klangquelle::Slices, "Slices")
                .clicked()
            {
                self.klangquelle = Klangquelle::Slices;
                // Nach einem App-Neustart ist das Raster leer (es liegt nur im
                // Speicher): die Slices aus dem letzten Export nachladen, damit
                // „Slices" sofort klingt statt auf den Tongenerator zu fallen.
                let aus_export = crate::slicer::raster_aus_letztem_export();
                let anzahl = crate::slicer::slice_anzahl();
                // Die Instrumentnummer ist der Slice-Index: liegt sie hinter
                // den geladenen Slices (z. B. noch von Modul-Samples), klingt
                // sonst der Tongenerator statt des Slices — auf den letzten
                // vorhandenen Slice klemmen.
                if anzahl > 0 && self.instrument as usize >= anzahl {
                    self.instrument = (anzahl - 1).min(255) as u8;
                }
                self.status = if aus_export {
                    format!("Klang: Slices ({anzahl} aus letztem Export geladen)")
                } else {
                    format!("Klang: Slices ({anzahl} im Slicer geladen)")
                };
            }
            if self.klangquelle == Klangquelle::Welle && ui.button(self.welle.name()).clicked() {
                let alle = jamcore::klang::Welle::ALLE;
                let idx = alle.iter().position(|w| *w == self.welle).unwrap_or(0);
                self.welle = alle[(idx + 1) % alle.len()];
            }
            if self.klangquelle == Klangquelle::Modul && self.modul_quelle.is_none() {
                ui.label(
                    egui::RichText::new("kein Modul geladen — es klingt der Tongenerator")
                        .small()
                        .color(Color32::GRAY),
                );
            }
            if self.klangquelle == Klangquelle::Slices {
                let anzahl = crate::slicer::slice_anzahl();
                if anzahl == 0 {
                    ui.label(
                        egui::RichText::new("keine Slices — im Slicer-Tab Audio laden")
                            .small()
                            .color(Color32::GRAY),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!("{anzahl} Slices, Instr = Slice-Nummer"))
                            .small()
                            .color(Color32::GRAY),
                    );
                    if ui.button("Slice hören").clicked() {
                        crate::slicer::slice_spielen(self.instrument as usize);
                    }
                    // Nutzerwunsch „Slices kann noch max vol“: globale
                    // Lautstaerke 0…200 % (Vorgabe 100 %) fuer **alle**
                    // Slice-Wiedergaben. Der Wert liegt in `klang`, damit er
                    // Tab-Wechsel und den Render-Thread ueberlebt.
                    let mut prozent = crate::klang::slices_lautstaerke() as f32;
                    if ui
                        .add(
                            egui::Slider::new(&mut prozent, 0.0..=200.0)
                                .suffix(" %")
                                .text("Slices-Lautstärke"),
                        )
                        .changed()
                    {
                        crate::klang::slices_lautstaerke_setzen(prozent);
                        self.status = format!("Slices-Lautstärke: {:.0} %", prozent.round());
                    }
                    if ui
                        .small_button("Max")
                        .on_hover_text("Slices-Lautstärke auf 200 %")
                        .clicked()
                    {
                        crate::klang::slices_lautstaerke_setzen(200.0);
                        self.status = "Slices-Lautstärke: 200 % (max)".to_string();
                    }
                }
            }
        });
        // Kanalvergabe (Nutzerwunsch 2026-10-20): das Sample soll im Kanal
        // bleiben und nicht bei jeder Note wechseln. Die Reihenfolge der
        // Regeln ist wählbar; sie wirkt beim Verteilen und in der Wiedergabe
        // (Log/Status), nicht auf die polyphone Audioausgabe.
        ui.horizontal_wrapped(|ui| {
            ui.label("Kanäle");
            if ui
                .selectable_label(self.kanal_sample_zuerst, "Sample zuerst")
                .clicked()
            {
                self.kanal_sample_zuerst = true;
                self.status = "Kanalvergabe: Sample zuerst".to_string();
            }
            if ui
                .selectable_label(!self.kanal_sample_zuerst, "frei zuerst")
                .clicked()
            {
                self.kanal_sample_zuerst = false;
                self.status = "Kanalvergabe: frei zuerst".to_string();
            }
            if ui.button("Kanäle verteilen").clicked() {
                self.kanaele_verteilen();
            }
            ui.label(
                RichText::new("Noten des Patterns auf Tracker-Kanäle verteilen")
                    .small()
                    .color(Color32::GRAY),
            );
        });
        // Tongenerator: Stimmen und Tonlaenge (Nutzerentscheidung 2026-10-14).
        // Gilt fuer alle lokalen Klaenge (Rhythmus, Bild-Scan, Rendern).
        ui.horizontal(|ui| {
            let e = crate::klang::einstellungen();
            ui.label("Stimmen");
            let mut stimmen = e.stimmen as i32;
            if ui
                .add(egui::DragValue::new(&mut stimmen).range(1..=256))
                .changed()
            {
                crate::klang::stimmen_setzen(stimmen as usize);
            }
            ui.label("Länge");
            egui::ComboBox::from_id_salt("klang_laenge")
                .selected_text(format!("{} ({})", e.notenwert.name(), ms_text(e)))
                .show_ui(ui, |ui| {
                    for wert in jamcore::klang::Notenwert::ALLE {
                        let ms = wert.ms(crate::klang::einstellungen().bpm);
                        if ui
                            .selectable_label(
                                e.notenwert == wert,
                                format!("{} ({ms} ms)", wert.name()),
                            )
                            .clicked()
                        {
                            crate::klang::notenwert_setzen(wert);
                        }
                    }
                });
            ui.label(egui::RichText::new(ms_text(e)).small().color(Color32::GRAY));
        });
        ui.label(
            egui::RichText::new(
                "Achtung: Samples und Instrumente von Modulen sind urheberrechtlich geschützt \
                 — vor einer Wiederverwendung (Aufnahme, Export, Weitergabe) die Rechte klären.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
        ui.horizontal(|ui| {
            ui.label(format!(
                "Schritt {}/{}",
                self.current_step + 1,
                self.pattern().steps_per_cycle()
            ));
            ui.label(format!("Noten: {}", self.pattern().notes().len()));
            if self.follow {
                ui.label(RichText::new("Follow: Modul").strong());
            } else {
                ui.label(format!("Eigenuhr: {:.0} BPM", self.bpm));
            }
        });
        ui.add(
            egui::Slider::new(&mut self.bpm, 40.0..=220.0)
                .text("BPM")
                .show_value(true),
        );
        // Feinheit der Uhr: 16 Zeilen je Takt = 1/16, 32 = 1/32, 64 = 1/64.
        // Sie bestimmt das Zeitraster von Uhr, Rendern und Laub-Rendern;
        // bestehende Noten bleiben liegen (nur ihre Zeit wird feiner).
        ui.horizontal_wrapped(|ui| {
            ui.label("Feinheit");
            for (wert, text) in [(16u16, "1/16"), (32, "1/32"), (64, "1/64")] {
                if ui.selectable_label(self.feinheit == wert, text).clicked() {
                    self.feinheit = wert;
                    self.status = format!("Feinheit {text}: {wert} Zeilen je Takt");
                    info!("rhythm: Feinheit {text} ({wert} Zeilen je Takt)");
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Follow").clicked() {
                self.follow = !self.follow;
                self.clock = Instant::now();
                info!("Follow = {}", self.follow);
            }
            if ui.button("Leeren").clicked() {
                self.pattern_mut().clear();
                self.status = "Raster geleert.".to_string();
            }
            if ui.button("Vel -").clicked() {
                self.velocity = self.velocity.saturating_sub(10).max(1);
            }
            ui.label(format!("Vel {}", self.velocity));
            if ui.button("Vel +").clicked() {
                self.velocity = (self.velocity + 10).min(127);
            }
        });
        ui.horizontal(|ui| {
            // "Rendern" wie Taste o im Desktop-Reporder: Raster abspielen.
            if self.render_laeuft {
                if ui.button("Rendern stoppen").clicked() {
                    self.rendern_stoppen();
                }
                ui.label(
                    RichText::new(format!("läuft, Schritt {}", self.current_step + 1))
                        .strong()
                        .color(Color32::from_rgb(90, 200, 120)),
                );
            } else {
                if ui.button("Rendern").clicked() {
                    self.rendern_starten();
                }
                // „Alles hören“: Kette auf alle belegten Patterns setzen und
                // sofort rendern — der schnellste Weg, ein Laub-/Bild-Ergebnis
                // als Ganzes zu pruefen, statt Slots einzeln zu waehlen.
                if ui
                    .button("Alles hören")
                    .on_hover_text("Kette auf alle belegten Patterns setzen und rendern")
                    .clicked()
                {
                    self.alles_hoeren();
                }
            }
            // Kein eigener Regler: `set_tempo` schreibt das Rendertempo jeden
            // Frame aus dem App-Tempo (OSC-Tab „Tempo“), ein Slider hier waere
            // wirkungslos — er zeigte nur einen Wert, den man nicht aendern
            // kann. Also nur anzeigen, woher er kommt.
            ui.label(
                RichText::new(format!("{:.0} BPM (folgt App-Tempo)", self.render_bpm))
                    .color(Color32::GRAY),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Kette").small().color(Color32::GRAY));
            for wert in [1usize, 2, 3, 4, 8] {
                if ui
                    .selectable_label(self.kette == wert, wert.to_string())
                    .on_hover_text("So viele Patterns spielt „Rendern“ hintereinander")
                    .clicked()
                {
                    self.kette = wert;
                    self.status = format!("Kette: {wert} Pattern(s)");
                }
            }
            ui.label(
                RichText::new(format!("{} / 8", self.kette))
                    .small()
                    .color(Color32::GRAY),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.transport_mit, "Transport");
            ui.checkbox(&mut self.aufnahme, "Aufnahme");
            ui.label(
                RichText::new(if self.aufnahme && self.render_laeuft {
                    "Tipp landet auf dem laufenden Schritt"
                } else if self.aufnahme {
                    "Aufnahme wirkt beim Rendern"
                } else {
                    ""
                })
                .small()
                .color(Color32::GRAY),
            );
        });
        ui.horizontal_wrapped(|ui| {
            // Transformationen wie im Desktop-Recorder.
            if ui.button("-12").clicked() {
                self.noten_transformieren(|n| transformation::transponieren(n, -12));
            }
            if ui.button("+12").clicked() {
                self.noten_transformieren(|n| transformation::transponieren(n, 12));
            }
            if ui.button("Umkehren").clicked() {
                let laenge = self.pattern().steps_per_cycle();
                self.noten_transformieren(|noten| transformation::umkehren(noten, laenge));
            }
            if ui.button("Q2").clicked() {
                self.noten_transformieren(|n| transformation::quantisieren(n, 2));
            }
            if ui.button("Q4").clicked() {
                self.noten_transformieren(|n| transformation::quantisieren(n, 4));
            }
            if ui.button("Glide").clicked() {
                let an = !self.pattern().notes().iter().all(|n| n.glide);
                self.noten_transformieren(|n| transformation::glide_setzen(n, an));
            }
            let skala_name = [
                "Skala: aus",
                "Skala: Moll",
                "Skala: Dur",
                "Skala: Pent-moll",
                "Skala: Pent-dur",
            ][self.skala.min(4)];
            if ui.button(skala_name).clicked() {
                self.skala = (self.skala + 1) % 5;
            }
            if self.skala > 0 {
                ui.label(format!("Grundton {}", note_name(self.grundton)));
                ui.add(egui::Slider::new(&mut self.grundton, 48..=72));
            }
            ui.checkbox(&mut self.presenter, "Presenter");
            if ui.button("Instr -").clicked() {
                self.instrument = self.instrument.saturating_sub(1);
            }
            if self.klangquelle == Klangquelle::Slices {
                // Ein getauschtes Sample bleibt auch bei „Slices“ sichtbar:
                // der Tausch gehoert zum Instrument, nicht zur Klangquelle.
                let zusatz = if crate::slicer::hat_getauschtes_sample(self.instrument as usize) {
                    " *Slice"
                } else {
                    ""
                };
                ui.label(format!(
                    "Slice {} / {}{zusatz}",
                    self.instrument as usize + 1,
                    crate::slicer::slice_anzahl()
                ));
            } else {
                ui.label(self.instrument_anzeige());
            }
            if ui.button("Instr +").clicked() {
                // Bei der Klangquelle „Slices" ist die Instrumentnummer der
                // Slice-Index; mehr als geladene Slices gibt es nicht.
                let max = if self.klangquelle == Klangquelle::Slices {
                    crate::slicer::slice_anzahl().saturating_sub(1).min(255) as u8
                } else {
                    15
                };
                self.instrument = (self.instrument + 1).min(max);
            }
            if ui.button("Instr auf alle").clicked() {
                let inst = self.instrument;
                self.noten_transformieren(move |n| transformation::instrument_setzen(n, inst));
            }
        });

        if self.presenter {
            self.presenter_zeichnen(ui);
        }

        // Ein Block je 8 Zellen (16 passen nicht nebeneinander auf die
        // Handy-Breite). Die Blockzahl folgt der **Patternlaenge** und der
        // Buendelung: bei der Vorgabe 512 und Buendelung 1 sind das 64
        // Bloecke, die ScrollArea traegt sie. Eine Zelle fasst `buendel`
        // Schritte zusammen und leuchtet, wenn darin eine Note liegt.
        // Im Tracker-Modus sind diese Pads die Notentastatur: der Tipp geht in
        // die dort gewählte Zelle (Schritt/Kanal), nicht in die Rasterzelle.
        let schritte = self.pattern().steps_per_cycle().max(1);
        let buendel = self.buendel.max(1);
        let je_block = BLOCK * buendel;
        let bloecke = schritte.div_ceil(je_block);
        for block in 0..bloecke {
            Grid::new(format!("rhythm_block{block}"))
                .spacing([2.0, 2.0])
                .show(ui, |ui| {
                    let erster = block * je_block;
                    let letzter = (erster + je_block).min(schritte);
                    ui.label(
                        RichText::new(format!("{}-{}", erster + 1, letzter))
                            .small()
                            .color(Color32::GRAY),
                    );
                    for i in 0..BLOCK {
                        let zelle = block * BLOCK + i;
                        let anfang = zelle as u32 * buendel as u32;
                        if anfang >= schritte as u32 {
                            _ = ui.allocate_painter(egui::vec2(CELL, 12.0), Sense::hover());
                        } else if i % 2 == 0 {
                            ui.label(
                                RichText::new(format!("{}", anfang + 1))
                                    .small()
                                    .color(Color32::GRAY),
                            );
                        } else {
                            _ = ui.allocate_painter(egui::vec2(CELL, 12.0), Sense::hover());
                        }
                    }
                    ui.end_row();
                    for pitch in ROWS {
                        ui.label(RichText::new(note_name(pitch)).small());
                        for i in 0..BLOCK {
                            let zelle = block * BLOCK + i;
                            let anfang = zelle as u32 * buendel as u32;
                            if anfang >= schritte as u32 {
                                // Laenge kein Vielfaches: Restzelle leer, aber
                                // gleiche Breite wie die anderen.
                                _ = ui.allocate_painter(egui::vec2(CELL, CELL), Sense::hover());
                                continue;
                            }
                            let (von, bis) =
                                jamcore::grid::buendel_spanne(zelle as usize, buendel, schritte);
                            let on = self.pattern().hat_im_buendel(von, bis, pitch);
                            let played = self.current_step >= von && self.current_step <= bis;
                            let color = if on {
                                Color32::from_rgb(90, 200, 120)
                            } else if played {
                                Color32::from_rgb(115, 125, 155)
                            } else {
                                Color32::from_rgb(45, 48, 58)
                            };
                            let (cell, painter) =
                                ui.allocate_painter(egui::vec2(CELL, CELL), Sense::click());
                            painter.rect_filled(cell.rect, 3.0, color);
                            if on {
                                painter.rect_stroke(
                                    cell.rect,
                                    egui::CornerRadius::same(3),
                                    egui::Stroke::new(1.0, Color32::from_rgb(150, 240, 170)),
                                    egui::StrokeKind::Middle,
                                );
                            }
                            // Der Taktcursor ist ein heller Rahmen, damit er auch
                            // auf einer belegten Zelle sichtbar bleibt.
                            if played {
                                painter.rect_stroke(
                                    cell.rect,
                                    egui::CornerRadius::same(3),
                                    egui::Stroke::new(2.0, Color32::from_rgb(245, 245, 255)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if cell.clicked() {
                                self.pad_buendel(von, bis, pitch);
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
        }

        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        ui.label(
            RichText::new("CSV (Desktop-Format) und Clipboard-XML liegen im App-Ordner patterns/.")
                .small()
                .color(Color32::DARK_GRAY),
        );
    }
}
