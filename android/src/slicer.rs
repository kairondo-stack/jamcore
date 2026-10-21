//! Slicer-Tab: geladenes Audio schneiden (automatisch und von Hand) und die
//! Slices als `.it` sichern.
//!
//! Die Rechenarbeit liegt im Kern (`jamcore::audio`, `jamcore::slice`,
//! `jamcore::itwrite`); hier steht nur die Oberfläche: Wellenform zeichnen,
//! Schnittmarken setzen/ziehen/löschen, Slices vorhören und sichern.
//!
//! Bedienung (Touchscreen):
//! - **Tippen** in die Wellenform setzt den Schnitt an die getippte Stelle
//!   (`slice::setze`; im ersten Slice wandert dessen Ende mit `slice::verschiebe`).
//! - **Ziehen** verschiebt den nächstgelegenen Schnitt (`slice::verschiebe`).
//! - **Langes Tippen** (≥ 0,7 s ohne Bewegung) löscht den Slice unter dem
//!   Finger (`slice::loesche`); der Knopf „× löschen" tut dasselbe für den
//!   gewählten Slice.
//! - Die **Nummernknöpfe** 1 … N (und „Vorhören") spielen einen Slice über
//!   `klang::puffer_spielen` — dieselbe Funktion, die auch das Rhythmus-Raster
//!   benutzt (Instrument-Quelle „Slices").

use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Sense};
use jamcore::slice::{self, Schnittmodus, Slice};
use log::info;

use crate::wavrec;

/// Was die Ansicht von der App braucht (den Datei-Browser öffnet die App).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Aktion {
    Keine,
    /// Der Slicer möchte den Audio-Browser öffnen.
    BrowserOeffnen,
}

/// Höhe der Wellenform in Punkten.
const WELLE_H: f32 = 170.0;
/// So viele Kästen hat die gerechnete Hüllkurve (Anzeige-Raster).
const HUELLEN_KAESTEN: usize = 1024;
/// Ab dieser Breite bekommt eine Slice ihre Nummer in der Wellenform.
const NUMMER_MIN_BREITE: f32 = 16.0;
/// So lange muss der Finger liegen, damit der Slice darunter gelöscht wird.
const LANG_TIPP_S: f32 = 0.7;
/// Höchstens so weit darf der Finger beim Lang-Tipp wandern.
const LANG_TIPP_PX: f32 = 14.0;
/// Vorgabe-Anzahl der automatischen Schnitte.
const ANZAHL_STANDARD: usize = 8;

/// Die Slices des Slicers als fertige PCM-Puffer (mono + Abtastrate) — der
/// Rhythmus-Tab spielt sie über `klang::puffer_spielen`.
static RASTER: OnceLock<Mutex<Vec<(Vec<i16>, u32)>>> = OnceLock::new();

/// Pfad der zuletzt exportierten `.it` (für die Aufnahme im Bild-Tab, „Rec"):
/// der Rec-Knopf schreibt dann in **dieselbe** Datei, statt eine neue anzulegen.
static LETZTER_EXPORT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Merker-Datei im App-Ordner: der letzte Export überlebt so den App-Neustart.
/// Nötig, weil das Raster (die klingenden Slices) nur im Speicher liegt — nach
/// einem Neustart war „Slices" im Rhythmus-Tab sonst still (der Tongenerator
/// sprang ein), obwohl die .it mit den Slices noch da ist.
const EXPORT_MERKER: &str = "slicer-export.txt";

fn raster_slot() -> &'static Mutex<Vec<(Vec<i16>, u32)>> {
    RASTER.get_or_init(|| Mutex::new(Vec::new()))
}

fn export_slot() -> &'static Mutex<Option<String>> {
    LETZTER_EXPORT.get_or_init(|| Mutex::new(None))
}

/// Ein Sample, das im Slicer fuer ein Instrument hinterlegt wurde
/// („Instr N ← Slice“/„Instr N ← Klang“).
///
/// Warum eine eigene Ablage statt direkt in `instr.rs`: der Instrument-Tab
/// arbeitet parallel in einer anderen Datei; diese kleine API ist der
/// gemeinsame Nenner. `instr.rs` kann [`sample_fuer_instrument`] lesen und den
/// Namen anzeigen, der Rhythmus-Tab spielt das Sample ueber
/// [`instrument_sample_spielen`] — auch ohne Modul.
#[derive(Clone, Debug)]
pub struct GetauschtesSample {
    /// Anzeigename, z. B. `Slice 3 (beat.wav)`.
    pub name: String,
    /// Mono-PCM, 16 Bit (gleicher Weg wie die Slices).
    pub pcm: Vec<i16>,
    /// Abtastrate der Quelle.
    pub rate: u32,
}

static GETAUSCHT: OnceLock<Mutex<Vec<Option<GetauschtesSample>>>> = OnceLock::new();

fn tausch_slot() -> &'static Mutex<Vec<Option<GetauschtesSample>>> {
    GETAUSCHT.get_or_init(|| Mutex::new(Vec::new()))
}

/// Ein fertiges PCM-Sample als Sample von Instrument `instrument` hinterlegen
/// (0-basiert wie `Note::instrument`). Leere Puffer/0 Hz werden verworfen.
pub fn setze_sample(instrument: usize, name: &str, pcm: Vec<i16>, rate: u32) {
    if pcm.is_empty() || rate == 0 {
        info!("slicer: Sample fuer Instr {instrument} verworfen (leer oder 0 Hz)");
        return;
    }
    let sample = GetauschtesSample {
        name: name.to_string(),
        pcm,
        rate,
    };
    if let Ok(mut slot) = tausch_slot().lock() {
        if slot.len() <= instrument {
            slot.resize_with(instrument + 1, || None);
        }
        info!(
            "slicer: Sample fuer Instr {instrument} gesetzt ({}, {} Samples, {} Hz)",
            sample.name,
            sample.pcm.len(),
            sample.rate
        );
        slot[instrument] = Some(sample);
    }
}

/// Ein Sample aus einer Datei laden und als Instrument-Sample setzen.
/// `Ok(Name)` = hinterlegt, `Err` = Datei nicht lesbar.
///
/// Fuer Aufrufer, die einen Pfad statt fertigem PCM haben (z. B. den
/// Instrument-Tab); der Slicer selbst nimmt [`setze_sample`] mit dem Puffer,
/// den er schon im Speicher hat.
pub fn setze_sample_pfad(instrument: usize, pfad: &str) -> Result<String, String> {
    let audio = jamcore::audio::laden(pfad)?;
    let name = Path::new(pfad)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| pfad.to_string());
    let pcm: Vec<i16> = audio
        .proben
        .iter()
        .map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();
    setze_sample(instrument, &name, pcm, audio.rate);
    Ok(name)
}

/// Das getauschte Sample von Instrument `instrument` (Kopie, damit der
/// Aufrufer nicht unter dem Mutex klingen muss).
pub fn sample_fuer_instrument(instrument: usize) -> Option<GetauschtesSample> {
    tausch_slot()
        .lock()
        .ok()
        .and_then(|s| s.get(instrument).cloned().flatten())
}

/// Anzeigename des getauschten Samples (`None` = kein Tausch).
pub fn sample_name(instrument: usize) -> Option<String> {
    sample_fuer_instrument(instrument).map(|s| s.name)
}

/// Liegt fuer Instrument `instrument` ein getauschtes Sample vor?
pub fn hat_getauschtes_sample(instrument: usize) -> bool {
    sample_fuer_instrument(instrument).is_some()
}

/// Alle Tausch-Eintraege `(Instrument, Name)` fuer die Anzeige im Slicer.
pub fn getauschte_liste() -> Vec<(usize, String)> {
    tausch_slot()
        .lock()
        .map(|s| {
            s.iter()
                .enumerate()
                .filter_map(|(i, e)| e.as_ref().map(|e| (i, e.name.clone())))
                .collect()
        })
        .unwrap_or_default()
}

/// Tausch fuer ein Instrument wieder entfernen. `true`, wenn es einen gab.
pub fn tausch_loeschen(instrument: usize) -> bool {
    let entfernt = tausch_slot()
        .lock()
        .map(|mut s| s.get_mut(instrument).map(Option::take).is_some())
        .unwrap_or(false);
    if entfernt {
        info!("slicer: Sample fuer Instr {instrument} entfernt");
    }
    entfernt
}

/// Das getauschte Sample von Instrument `instrument` einmal abspielen —
/// `true`, wenn es eines gibt (sonst faellt der Aufrufer auf Modul/Tongenerator
/// zurueck). `pitch` verschiebt die Abspielrate wie ein Sample im Modul:
/// 60 (C-4) = Originaltonhoehe, je Oktave Faktor 2.
pub fn instrument_sample_spielen(instrument: usize, pitch: i32) -> bool {
    let Some(sample) = sample_fuer_instrument(instrument) else {
        return false;
    };
    let verstimmung = (pitch.clamp(0, 127) - 60) as f32 / 12.0;
    let rate = (sample.rate as f32 * 2.0f32.powf(verstimmung))
        .round()
        .clamp(2_000.0, 192_000.0) as u32;
    info!(
        "slicer: Instr {} spielt getauschtes Sample ({} Hz -> {} Hz, {})",
        instrument,
        sample.rate,
        rate,
        sample.name
    );
    crate::klang::puffer_spielen(sample.pcm, rate, 0.8);
    true
}

/// Ziel-Instrument fuer den Knopf „→ Instr N“ im Slicer. Das Rhythmus-Panel
/// schreibt hier seine aktuelle Instrumentnummer hinein; der Slicer liest und
/// aendert sie (DragValue). Der Versionszaehler sagt dem Rhythmus-Panel, wann
/// die Auswahl von hier kam — so bleibt **ein** Wert sichtbar, egal in welchem
/// Tab er geaendert wurde.
static ZIEL_INSTRUMENT: AtomicU8 = AtomicU8::new(0);
static ZIEL_VERSION: AtomicU64 = AtomicU64::new(0);

/// Aktuelles Ziel-Instrument (0-basiert, 0…15).
pub fn ziel_instrument() -> u8 {
    ZIEL_INSTRUMENT.load(Ordering::Relaxed)
}

/// Versionszaehler der Ziel-Auswahl (steigt bei jeder Aenderung).
pub fn ziel_version() -> u64 {
    ZIEL_VERSION.load(Ordering::Relaxed)
}

/// Ziel-Instrument setzen (0…15 geklemmt); nur eine echte Aenderung zaehlt.
pub fn ziel_instrument_setzen(instrument: u8) {
    let neu = instrument.min(15);
    let alt = ZIEL_INSTRUMENT.swap(neu, Ordering::Relaxed);
    if alt != neu {
        ZIEL_VERSION.fetch_add(1, Ordering::Relaxed);
        info!("slicer: Ziel-Instrument {neu}");
    }
}

/// Pfad der zuletzt exportierten .it (None, wenn keine). Fragt beim ersten Mal
/// den Merker im App-Ordner, damit er einen App-Neustart übersteht.
pub fn letzter_export() -> Option<String> {
    if let Some(pfad) = export_slot().lock().ok().and_then(|p| p.clone()) {
        return Some(pfad);
    }
    let pfad = merker_lesen()?;
    if let Ok(mut slot) = export_slot().lock() {
        *slot = Some(pfad.clone());
    }
    Some(pfad)
}

/// Den Pfad des letzten Exports merken (Speicher **und** Merker-Datei).
fn export_merken(pfad: &str) {
    if let Ok(mut slot) = export_slot().lock() {
        *slot = Some(pfad.to_string());
    }
    match app_ordner().map(|o| o.join(EXPORT_MERKER)) {
        Ok(merker) => {
            if let Err(e) = std::fs::write(&merker, pfad) {
                info!("slicer: Merker {} nicht schreibbar: {e}", merker.display());
            }
        }
        Err(e) => info!("slicer: Merker nicht ablegbar: {e}"),
    }
}

/// Den gemerkten Export-Pfad lesen (`None`, wenn keiner gemerkt ist oder die
/// Datei inzwischen fehlt).
fn merker_lesen() -> Option<String> {
    let merker = app_ordner().ok()?.join(EXPORT_MERKER);
    let pfad = std::fs::read_to_string(&merker).ok()?.trim().to_string();
    (!pfad.is_empty() && Path::new(&pfad).is_file()).then_some(pfad)
}

/// Das Raster aus dem letzten Export füllen, wenn es leer ist — z. B. nach
/// einem App-Neustart, ohne den Slicer-Tab zu öffnen. `true`, wenn dafür
/// Slices aus dem Export gelesen wurden (dann klingt „Slices" im Rhythmus-Tab
/// sofort); `false`, wenn schon Slices bereitlagen oder es keinen Export gibt.
pub fn raster_aus_letztem_export() -> bool {
    if slice_anzahl() > 0 {
        return false;
    }
    let Some(pfad) = letzter_export() else {
        info!("slicer: kein letzter Export gemerkt (Raster bleibt leer)");
        return false;
    };
    let Ok(daten) = std::fs::read(&pfad) else {
        info!("slicer: letzter Export nicht lesbar: {pfad}");
        return false;
    };
    match crate::image::it_lesen(&daten) {
        Ok((samples, _)) => {
            let neu: Vec<(Vec<i16>, u32)> = samples
                .into_iter()
                .map(|s| (s.daten, s.rate.max(1)))
                .collect();
            let anzahl = neu.len();
            let rate = neu.first().map(|(_, r)| *r).unwrap_or(0);
            if let Ok(mut slot) = raster_slot().lock() {
                *slot = neu;
            }
            info!("slicer: Raster aus letztem Export {pfad} ({anzahl} Slices, {rate} Hz)");
            anzahl > 0
        }
        Err(e) => {
            info!("slicer: letzter Export nicht gelesen: {e} ({pfad})");
            false
        }
    }
}

/// Slices als 16-Bit-PCM: (Rate, [(Name, Daten)]) für eine neue .it.
///
/// Genau die Puffer, die auch das Rhythmus-Raster spielt („Slices"); `None`,
/// wenn kein Audio geladen ist. Die Namen sind dieselben wie beim Export
/// („Slice 1", „Slice 2", …).
pub fn slices_fuer_it() -> Option<(u32, Vec<(String, Vec<i16>)>)> {
    let guard = raster_slot().lock().ok()?;
    if guard.is_empty() {
        return None;
    }
    let rate = guard
        .iter()
        .map(|(_, r)| *r)
        .find(|r| *r > 0)
        .unwrap_or(44100);
    let slices = guard
        .iter()
        .enumerate()
        .map(|(i, (pcm, _))| (format!("Slice {}", i + 1), pcm.clone()))
        .collect();
    Some((rate, slices))
}

/// Anzahl der Slices, die das Raster spielen kann (0 = nichts geladen).
pub fn slice_anzahl() -> usize {
    raster_slot().lock().map(|v| v.len()).unwrap_or(0)
}

/// Slice `index` einmal abspielen (Rhythmus-Raster). `true`, wenn es ihn gibt.
///
/// Der Aufruf **ersetzt keine laufende Stimme**: jeder Slice ist eine eigene
/// Stimme in `klang::puffer_spielen` (mit eigener Hüllkurve), die vorige klingt
/// zu Ende. Genau das verhindert das Abschneiden des vorherigen Samples; der
/// Tracker-Kanal ist nur die Ordnung im Raster, kein Stimmenschalter.
pub fn slice_spielen(index: usize) -> bool {
    let Ok(guard) = raster_slot().lock() else {
        return false;
    };
    let Some((pcm, rate)) = guard.get(index) else {
        // Kein Klang aus dem Raster (nichts geladen oder Index zu groß): der
        // Aufrufer fällt auf den Tongenerator zurück. Das Log macht sichtbar,
        // warum „Slices" still bleibt.
        info!(
            "slicer: Slice {} fehlt ({} im Raster)",
            index + 1,
            guard.len()
        );
        return false;
    };
    info!(
        "slicer: Slice {} von {} -> Puffer ({} Samples, {} Hz, klingt über der vorigen Stimme)",
        index + 1,
        guard.len(),
        pcm.len(),
        rate
    );
    crate::klang::puffer_spielen(pcm.clone(), *rate, 0.8);
    true
}

pub struct SlicerPanel {
    /// Geladenes Audio (Mono, −1 … 1) und seine Abtastrate.
    audio: Option<(Vec<f32>, u32)>,
    /// Anzeige-Hüllkurve (min/max je Kasten), einmal gerechnet.
    huelle: Vec<(f32, f32)>,
    /// Pfad des geladenen Audios (für Datei- und .it-Namen).
    pfad: Option<String>,
    schnitte: Vec<Slice>,
    modus: Schnittmodus,
    anzahl: usize,
    /// Empfindlichkeit 0 … 1 (Anzeige in Prozent).
    empfindlichkeit: f32,
    /// Gewählter Slice (0-basiert).
    auswahl: usize,
    /// Laufende Aufnahme (None = keine).
    aufnahme: Option<wavrec::Aufnahme>,
    status: String,
    /// Beim Ziehen: welcher Schnitt (= Ende der Slice i) wandert gerade?
    zug_kante: Option<usize>,
    /// Lang-Tipp: Beginn des Drucks.
    druck: Option<(Instant, egui::Pos2)>,
    /// Der laufende Druck hat schon gelöscht — der Klick beim Loslassen zählt
    /// dann nicht mehr als „Schnitt setzen".
    lang_geloescht: bool,
    /// Slices haben sich geändert; das Raster (für den Rhythmus-Tab) muss neu
    /// gerechnet werden. Während des Ziehens bleibt es aus (Kosten).
    raster_schmutzig: bool,
}

impl Default for SlicerPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SlicerPanel {
    pub fn new() -> Self {
        Self {
            audio: None,
            huelle: Vec::new(),
            pfad: None,
            schnitte: vec![Slice { von: 0, bis: 0 }],
            modus: Schnittmodus::Transienten,
            anzahl: ANZAHL_STANDARD,
            empfindlichkeit: 0.5,
            auswahl: 0,
            aufnahme: None,
            status: "Audio öffnen oder aufnehmen — dann schneiden.".to_string(),
            zug_kante: None,
            druck: None,
            lang_geloescht: false,
            raster_schmutzig: false,
        }
    }

    fn proben(&self) -> &[f32] {
        self.audio
            .as_ref()
            .map(|(p, _)| p.as_slice())
            .unwrap_or(&[])
    }

    fn rate(&self) -> u32 {
        self.audio.as_ref().map(|(_, r)| *r).unwrap_or(0)
    }

    fn laenge(&self) -> usize {
        self.proben().len()
    }

    /// Audio laden (aus dem Browser oder von der Aufnahme) und die Slices nach
    /// dem eingestellten Modus legen. `Ok` = Kurzbeschreibung für den Status.
    pub fn laden(&mut self, pfad: &str) -> Result<String, String> {
        let audio = jamcore::audio::laden(pfad)?;
        let sekunden = audio.proben.len() as f32 / audio.rate.max(1) as f32;
        self.huelle = huelle(&audio.proben, HUELLEN_KAESTEN);
        self.audio = Some((audio.proben, audio.rate));
        self.pfad = Some(pfad.to_string());
        self.schnitte = slice::schnitte(
            self.proben(),
            self.rate(),
            self.modus,
            self.anzahl,
            self.empfindlichkeit,
        );
        self.auswahl = 0;
        self.raster_schmutzig = true;
        let text = format!(
            "{sekunden:.1} s, {} Hz, {} Slice(s)",
            self.rate(),
            self.schnitte.len()
        );
        info!("slicer: {pfad} geladen ({text})");
        Ok(text)
    }

    /// Die Slices als PCM-Puffer für das Rhythmus-Raster bereitstellen.
    fn raster_aktualisieren(&self) {
        let Some((proben, rate)) = self.audio.as_ref() else {
            if let Ok(mut slot) = raster_slot().lock() {
                slot.clear();
            }
            return;
        };
        let neu: Vec<(Vec<i16>, u32)> = self
            .schnitte
            .iter()
            .map(|s| {
                let von = s.von.min(proben.len());
                let bis = s.bis.min(proben.len()).max(von);
                let pcm: Vec<i16> = proben[von..bis]
                    .iter()
                    .map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .collect();
                (pcm, *rate)
            })
            .collect();
        let anzahl = neu.len();
        if let Ok(mut slot) = raster_slot().lock() {
            *slot = neu;
        }
        info!("slicer: {anzahl} Slices fürs Raster bereit");
    }

    fn aufnahme_starten(&mut self) {
        if self.aufnahme.is_some() {
            return;
        }
        match wavrec::Aufnahme::start() {
            Ok(auf) => {
                self.aufnahme = Some(auf);
                self.status = "Aufnahme läuft … (Stopp beendet und lädt sie)".to_string();
                info!("slicer: Aufnahme gestartet");
            }
            Err(e) => {
                self.status = format!("Aufnahme nicht möglich: {e}");
                info!("slicer: Aufnahme-Start fehlgeschlagen: {e}");
            }
        }
    }

    fn aufnahme_stoppen(&mut self) {
        let Some(auf) = self.aufnahme.take() else {
            return;
        };
        match auf.stoppen() {
            Ok(pfad) => {
                let beschreibung = self.laden(&pfad);
                self.status = match beschreibung {
                    Ok(text) => format!("Aufnahme gespeichert: {pfad} ({text})"),
                    Err(e) => format!("Aufnahme gespeichert: {pfad}, aber nicht geladen: {e}"),
                };
            }
            Err(e) => self.status = format!("Aufnahme-Fehler: {e}"),
        }
    }

    /// Auto-Schnitte nach Modus, Anzahl und Empfindlichkeit legen.
    fn auto_schnitte(&mut self) {
        let neu = {
            let Some((proben, rate)) = self.audio.as_ref() else {
                self.status = "Kein Audio geladen — erst öffnen oder aufnehmen.".to_string();
                return;
            };
            slice::schnitte(proben, *rate, self.modus, self.anzahl, self.empfindlichkeit)
        };
        let anzahl = neu.len();
        self.schnitte = neu;
        self.auswahl = self.auswahl.min(anzahl.saturating_sub(1));
        self.raster_schmutzig = true;
        self.status = format!(
            "Auto: {anzahl} Slices ({} {:.0} %)",
            match self.modus {
                Schnittmodus::Gleichmaessig => "gleichmäßig",
                Schnittmodus::Transienten => "Transienten",
            },
            self.empfindlichkeit * 100.0
        );
        info!("slicer: {anzahl} Auto-Schnitte ({:?})", self.modus);
    }

    /// Sample-Index unter der x-Position (geklemmt).
    fn sample_bei(&self, x: f32, rect: egui::Rect) -> usize {
        let len = self.laenge();
        if len == 0 || rect.width() <= 0.0 {
            return 0;
        }
        let anteil = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        ((anteil * len as f32) as usize).min(len - 1)
    }

    /// Slice-Index, in dessen Bereich der Sample-Index liegt.
    fn slice_bei(&self, probe: usize) -> usize {
        self.schnitte
            .iter()
            .position(|s| probe >= s.von && probe < s.bis)
            .unwrap_or(0)
    }

    /// Tippen: den nächstgelegenen Schnitt an die getippte Stelle setzen.
    /// Schnitte sind der Anfang jeder Slice außer der ersten (`slice::setze`)
    /// und das Ende der ersten Slice (`slice::verschiebe`); das Ende der
    /// letzten Slice bleibt am Probenende. Mit nur einem Slice gibt es keinen
    /// Schnitt zum Verschieben — dann sagt der Status, was zu tun ist.
    fn schnitt_setzen(&mut self, probe: usize) {
        let gesamt = self.laenge();
        if gesamt == 0 {
            return;
        }
        // (Index, Abstand, ist das Ende der ersten Slice?)
        let mut best: Option<(usize, usize, bool)> = None;
        if self.schnitte.len() > 1 {
            for i in 1..self.schnitte.len() {
                let abstand = self.schnitte[i].von.abs_diff(probe);
                if best.map(|(_, d, _)| abstand < d).unwrap_or(true) {
                    best = Some((i, abstand, false));
                }
            }
            let abstand = self.schnitte[0].bis.abs_diff(probe);
            if best.map(|(_, d, _)| abstand < d).unwrap_or(true) {
                best = Some((0, abstand, true));
            }
        }
        let Some((index, _, ist_ende)) = best else {
            self.status =
                "Nur ein Slice — zuerst „Auto-Schnitte“ oder die Anzahl erhöhen.".to_string();
            return;
        };
        let sekunden = probe as f32 / self.rate().max(1) as f32;
        if ist_ende {
            slice::verschiebe(&mut self.schnitte, 0, probe, gesamt);
            self.status = format!("Schnitt 1 -> {sekunden:.2} s");
        } else {
            slice::setze(&mut self.schnitte, index, probe);
            self.status = format!("Schnitt {index} -> {sekunden:.2} s");
        }
        self.raster_schmutzig = true;
    }

    /// Beim Ziehen: der nächstgelegene Schnitt. Schnitte sind die Enden der
    /// Slices 0 … n−2 (das Ende der letzten bleibt am Probenende).
    fn naechster_schnitt(&self, probe: usize) -> Option<usize> {
        if self.schnitte.len() < 2 {
            return None;
        }
        let mut best: Option<(usize, usize)> = None;
        for i in 0..self.schnitte.len() - 1 {
            let abstand = self.schnitte[i].bis.abs_diff(probe);
            if best.map(|(_, d)| abstand < d).unwrap_or(true) {
                best = Some((i, abstand));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Slice löschen (der Bereich geht an die vorherige Slice). `true`, wenn
    /// wirklich gelöscht wurde (der letzte Slice bleibt stehen).
    fn slice_loeschen(&mut self, k: usize) -> bool {
        if self.schnitte.len() <= 1 {
            self.status = "Der letzte Slice bleibt — die Probe muss abgedeckt bleiben.".to_string();
            return false;
        }
        if k >= self.schnitte.len() {
            return false;
        }
        slice::loesche(&mut self.schnitte, k);
        self.auswahl = self.auswahl.min(self.schnitte.len() - 1);
        self.raster_schmutzig = true;
        self.status = format!("Slice {} gelöscht ({} übrig)", k + 1, self.schnitte.len());
        info!("slicer: Slice {} gelöscht", k + 1);
        true
    }

    /// Einen Slice vorhören — derselbe Weg wie die Raster-Noten
    /// (`klang::puffer_spielen`, einmal, mit der Rate des Audios).
    fn vorhoeren(&mut self, k: usize) {
        let Some((proben, rate)) = self.audio.as_ref() else {
            return;
        };
        let Some(s) = self.schnitte.get(k) else {
            return;
        };
        let von = s.von.min(proben.len());
        let bis = s.bis.min(proben.len()).max(von);
        if bis <= von {
            self.status = "Slice ist leer.".to_string();
            return;
        }
        let pcm: Vec<i16> = proben[von..bis]
            .iter()
            .map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        let rate = *rate;
        crate::klang::puffer_spielen(pcm, rate, 0.8);
        self.status = format!(
            "Slice {} vorgehört ({:.2} s, {} Hz)",
            k + 1,
            (bis - von) as f32 / rate.max(1) as f32,
            rate
        );
    }

    /// Den gewählten Slice als Sample von Instrument `instrument` hinterlegen
    /// („Instr N ← Slice“). Der Rhythmus-Tab zeigt danach `*Slice` an und
    /// spielt das Sample bei der Klangquelle „Modul“ statt des Modul-Samples —
    /// der Weg funktioniert also auch ohne geladenes Modul.
    fn instr_tauschen(&mut self, instrument: usize) {
        let Some((proben, rate)) = self.audio.as_ref() else {
            self.status = "Kein Audio geladen — erst öffnen oder aufnehmen.".to_string();
            return;
        };
        let Some(s) = self.schnitte.get(self.auswahl) else {
            self.status = "Kein Slice gewählt.".to_string();
            return;
        };
        let von = s.von.min(proben.len());
        let bis = s.bis.min(proben.len()).max(von);
        if bis <= von {
            self.status = "Der gewählte Slice ist leer.".to_string();
            return;
        }
        let pcm: Vec<i16> = proben[von..bis]
            .iter()
            .map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();
        let anzahl = pcm.len();
        let rate = *rate;
        let quelle = self
            .pfad
            .as_deref()
            .map(kurz)
            .unwrap_or_else(|| "Audio".to_string());
        let name = format!("Slice {} ({quelle})", self.auswahl + 1);
        setze_sample(instrument, &name, pcm, rate);
        self.status = format!(
            "Slice {} -> Instr {instrument} ({anzahl} Samples, {rate} Hz) — im Rhythmus-Tab hörbar",
            self.auswahl + 1
        );
        info!(
            "slicer: Slice {} -> Instr {instrument} ({anzahl} Samples, {rate} Hz)",
            self.auswahl + 1
        );
    }

    /// Slices als `.it` sichern (Slices als Samples, Pattern leer) — im
    /// App-Ordner `module/`, wo der Player-Tab sucht.
    fn als_it_sichern(&mut self) {
        let Some((proben, rate)) = self.audio.as_ref() else {
            self.status = "Kein Audio geladen.".to_string();
            return;
        };
        let name = self
            .pfad
            .as_deref()
            .and_then(|p| {
                Path::new(p)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "slices".to_string());
        let rate = *rate;
        let samples: Vec<jamcore::itwrite::Sample> = self
            .schnitte
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let von = s.von.min(proben.len());
                let bis = s.bis.min(proben.len()).max(von);
                let daten: Vec<i16> = proben[von..bis]
                    .iter()
                    .map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .collect();
                jamcore::itwrite::Sample {
                    name: format!("Slice {}", i + 1),
                    daten,
                    rate,
                    c5speed: 0,
                }
            })
            .collect();
        let anzahl = samples.len();
        let ordner = match app_module_ordner() {
            Ok(ordner) => ordner,
            Err(e) => {
                self.status = format!("Kein App-Ordner: {e}");
                return;
            }
        };
        let ziel = ordner.join(format!("{name}.it"));
        match jamcore::itwrite::schreibe(&ziel.display().to_string(), &samples, &name) {
            Ok(()) => {
                // Für die Aufnahme im Bild-Tab („Rec"): die nächste Aufnahme
                // schreibt in genau diese Datei, nicht in eine neue. Der Merker
                // im App-Ordner überlebt den App-Neustart (das Raster nicht).
                export_merken(&ziel.display().to_string());
                self.status = format!(
                    "{anzahl} Slices -> {} (im Player-Tab ladbar)",
                    ziel.display()
                );
                info!("slicer: {anzahl} Slices -> {}", ziel.display());
            }
            Err(e) => self.status = format!("IT-Fehler: {e}"),
        }
    }

    /// Die Wellenform mit Schnittmarken, Nummern und Bedienung.
    fn wellenform(&mut self, ui: &mut egui::Ui) {
        let breite = ui.available_width();
        let (response, painter) =
            ui.allocate_painter(egui::vec2(breite, WELLE_H), Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 3.0, Color32::from_rgb(18, 18, 22));
        let len = self.laenge();
        if len == 0 {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "kein Audio",
                egui::FontId::proportional(14.0),
                Color32::GRAY,
            );
            return;
        }

        // Nullinie.
        let mitte = rect.center().y;
        painter.line_segment(
            [
                egui::pos2(rect.left(), mitte),
                egui::pos2(rect.right(), mitte),
            ],
            egui::Stroke::new(1.0, Color32::from_rgb(60, 60, 66)),
        );

        // Wellenform: je Pixelspalte min/max aus der gerechneten Hüllkurve.
        let halbe = WELLE_H / 2.0 - 4.0;
        let kaesten = self.huelle.len().max(1) as f32;
        let wellen_farbe = Color32::from_rgb(120, 190, 240);
        let mut x = rect.left();
        while x < rect.right() {
            let k0 = ((x - rect.left()) / rect.width().max(1.0) * kaesten) as usize;
            let k1 = ((((x + 1.0) - rect.left()) / rect.width().max(1.0) * kaesten) as usize)
                .max(k0 + 1)
                .min(self.huelle.len());
            let (mut mn, mut mx) = (f32::MAX, f32::MIN);
            for k in k0..k1 {
                if let Some((a, b)) = self.huelle.get(k) {
                    mn = mn.min(*a);
                    mx = mx.max(*b);
                }
            }
            if mn <= mx {
                let y0 = mitte - mx.clamp(-1.0, 1.0) * halbe;
                let y1 = mitte - mn.clamp(-1.0, 1.0) * halbe;
                painter.line_segment(
                    [egui::pos2(x, y0), egui::pos2(x, y1.max(y0 + 1.0))],
                    egui::Stroke::new(1.0, wellen_farbe),
                );
            }
            x += 1.0;
        }

        // Slices: Auswahl hinterlegen, Schnittmarken und Nummern zeichnen.
        for (i, s) in self.schnitte.iter().enumerate() {
            let x0 = rect.left() + s.von as f32 / len as f32 * rect.width();
            let x1 = rect.left() + s.bis as f32 / len as f32 * rect.width();
            if i == self.auswahl {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, rect.top()),
                        egui::pos2(x1, rect.bottom()),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(90, 200, 120, 30),
                );
            }
            if i > 0 {
                painter.line_segment(
                    [egui::pos2(x0, rect.top()), egui::pos2(x0, rect.bottom())],
                    egui::Stroke::new(2.0, Color32::from_rgb(240, 200, 90)),
                );
            }
            if x1 - x0 >= NUMMER_MIN_BREITE {
                painter.text(
                    egui::pos2((x0 + x1) / 2.0, rect.top() + 9.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(220, 220, 230),
                );
            }
        }
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(3),
            egui::Stroke::new(1.0, Color32::from_rgb(70, 70, 78)),
            egui::StrokeKind::Inside,
        );

        // Bedienung. Während des Drucks neu zeichnen lassen: der Lang-Tipp
        // braucht Frames, auch wenn der Finger still liegt.
        if response.is_pointer_button_down_on() {
            ui.ctx().request_repaint_after(Duration::from_millis(80));
        }
        let probe = response
            .interact_pointer_pos()
            .map(|p| self.sample_bei(p.x, rect));

        if response.is_pointer_button_down_on() {
            if let Some(p) = response.interact_pointer_pos() {
                match self.druck {
                    None => {
                        self.druck = Some((Instant::now(), p));
                        self.lang_geloescht = false;
                    }
                    Some((start, p0)) => {
                        if !self.lang_geloescht
                            && start.elapsed().as_secs_f32() >= LANG_TIPP_S
                            && (p - p0).length() <= LANG_TIPP_PX
                        {
                            if let Some(pr) = probe {
                                let k = self.slice_bei(pr);
                                // Ein Druck löscht höchstens einen Slice: der
                                // Merker bleibt bis zum Loslassen stehen.
                                if self.slice_loeschen(k) {
                                    self.lang_geloescht = true;
                                }
                            }
                        }
                    }
                }
            }
        } else if self.druck.is_some() {
            self.druck = None;
        }

        if response.drag_started() {
            if let Some(pr) = probe {
                self.zug_kante = self.naechster_schnitt(pr);
                if let Some(k) = self.zug_kante {
                    self.status = format!("Schnitt {} ziehen …", k + 1);
                }
            }
        }
        if response.dragged() {
            if let (Some(k), Some(pr)) = (self.zug_kante, probe) {
                let gesamt = self.laenge();
                slice::verschiebe(&mut self.schnitte, k, pr, gesamt);
            }
        }
        if response.drag_stopped() {
            if self.zug_kante.is_some() {
                self.raster_schmutzig = true;
                self.status = format!("Schnitt verschoben: {} Slices", self.schnitte.len());
            }
            self.zug_kante = None;
        }
        if response.clicked() {
            if self.lang_geloescht {
                self.lang_geloescht = false;
            } else if let Some(pr) = probe {
                self.schnitt_setzen(pr);
            }
        }
        if !response.is_pointer_button_down_on() && !response.clicked() {
            self.lang_geloescht = false;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Aktion {
        let mut aktion = Aktion::Keine;
        // Während der Aufnahme laufen Pegel und Dauer weiter.
        if self.aufnahme.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        // Gestörte Aufnahme (Kanal weg): abräumen und melden, kein Panic.
        if let Some(f) = self.aufnahme.as_ref().and_then(|a| a.fehler()) {
            self.aufnahme_stoppen();
            self.status = format!("Aufnahme gestört: {f}");
        }

        ui.heading("Slicer");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Audio öffnen …").clicked() {
                aktion = Aktion::BrowserOeffnen;
            }
            if self.aufnahme.is_some() {
                if ui.button("Stopp").clicked() {
                    self.aufnahme_stoppen();
                }
            } else if ui.button("Aufnehmen").clicked() {
                self.aufnahme_starten();
            }
            if let Some(auf) = self.aufnahme.as_ref() {
                ui.label(
                    RichText::new(format!("● {:.1} s", auf.dauer_s()))
                        .color(Color32::from_rgb(230, 120, 120)),
                );
                ui.add(egui::ProgressBar::new(auf.pegel()).desired_width(120.0));
            }
        });

        match (self.pfad.as_deref(), &self.audio) {
            (Some(pfad), Some((proben, rate))) => {
                let rate = *rate;
                ui.label(format!(
                    "{} — {:.1} s, {} Hz, {} Proben, {} Slices",
                    kurz(pfad),
                    proben.len() as f32 / rate.max(1) as f32,
                    rate,
                    proben.len(),
                    self.schnitte.len()
                ));
            }
            _ => {
                ui.label(RichText::new("Kein Audio geladen.").color(Color32::GRAY));
            }
        }

        // Automatik: Modus, Anzahl (÷/×), Empfindlichkeit, Auto-Schnitte.
        ui.horizontal_wrapped(|ui| {
            ui.label("Modus");
            if ui
                .selectable_label(self.modus == Schnittmodus::Gleichmaessig, "Gleichmäßig")
                .clicked()
            {
                self.modus = Schnittmodus::Gleichmaessig;
            }
            if ui
                .selectable_label(
                    self.modus == Schnittmodus::Transienten,
                    "Transienten/Energie",
                )
                .clicked()
            {
                self.modus = Schnittmodus::Transienten;
            }
            ui.separator();
            ui.label("Anzahl");
            let mut anzahl = self.anzahl as i32;
            if ui
                .add(egui::DragValue::new(&mut anzahl).range(1..=64))
                .changed()
            {
                self.anzahl = anzahl.clamp(1, 64) as usize;
            }
            if ui.small_button("÷4").clicked() {
                self.anzahl = (self.anzahl / 4).max(1);
            }
            if ui.small_button("÷2").clicked() {
                self.anzahl = (self.anzahl / 2).max(1);
            }
            if ui.small_button("×2").clicked() {
                self.anzahl = (self.anzahl * 2).min(64);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let mut prozent = (self.empfindlichkeit * 100.0).round();
            if ui
                .add(egui::Slider::new(&mut prozent, 0.0..=100.0).text("Empfindlichkeit %"))
                .changed()
            {
                self.empfindlichkeit = (prozent / 100.0).clamp(0.0, 1.0);
            }
            if ui.button("Auto-Schnitte").clicked() {
                self.auto_schnitte();
            }
        });

        self.wellenform(ui);

        // Slices nummeriert: Tippen wählt und hört vor.
        if !self.schnitte.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label("Slices");
                for i in 0..self.schnitte.len() {
                    let gewaehlt = self.auswahl == i;
                    if ui
                        .selectable_label(gewaehlt, format!("{}", i + 1))
                        .clicked()
                    {
                        self.auswahl = i;
                        self.vorhoeren(i);
                    }
                }
            });
        }
        let bereit = !self.schnitte.is_empty();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(bereit, egui::Button::new("Vorhören"))
                .clicked()
            {
                self.vorhoeren(self.auswahl);
            }
            if ui
                .add_enabled(
                    bereit && self.schnitte.len() > 1,
                    egui::Button::new("× löschen"),
                )
                .clicked()
            {
                let _ = self.slice_loeschen(self.auswahl);
            }
            if ui
                .add_enabled(bereit, egui::Button::new("Als .it sichern"))
                .clicked()
            {
                self.als_it_sichern();
            }
        });
        // Instrument-Sample-Tausch aus dem Slicer: der gewaehlte Slice wird das
        // Sample von Instrument N. N ist das Ziel-Instrument aus dem
        // Rhythmus-Tab (dort „Instr ±“); hier laesst es sich ebenfalls stellen.
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Sample-Tausch").small());
            let mut ziel = ziel_instrument() as i32;
            if ui
                .add(
                    egui::DragValue::new(&mut ziel)
                        .range(0..=15)
                        .prefix("Instr "),
                )
                .changed()
            {
                ziel_instrument_setzen(ziel as u8);
            }
            if ui
                .add_enabled(
                    bereit,
                    egui::Button::new(format!("→ Instr {}", ziel_instrument())),
                )
                .on_hover_text("Gewählten Slice als Sample des Instruments setzen")
                .clicked()
            {
                self.instr_tauschen(ziel_instrument() as usize);
            }
            ui.label(
                RichText::new("ersetzt dort das Modul-Sample (im Rhythmus hörbar)")
                    .small()
                    .color(Color32::GRAY),
            );
        });
        let getauscht = getauschte_liste();
        if !getauscht.is_empty() {
            let liste: Vec<String> = getauscht
                .iter()
                .map(|(instrument, name)| format!("Instr {instrument} ← {name}"))
                .collect();
            ui.label(
                RichText::new(format!("Getauscht: {}", liste.join(" · ")))
                    .small()
                    .color(Color32::from_rgb(150, 210, 170)),
            );
        }

        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        ui.label(
            RichText::new(
                "Wellenform: tippen setzt den Schnitt, ziehen verschiebt ihn, langes Tippen \
                 löscht den Slice. „Als .it sichern“ legt die Slices als Samples in \
                 <App-Ordner>/module/<Name>.it ab (im Player-Tab ladbar); das Rhythmus-Raster \
                 spielt sie über die Klangquelle „Slices“ (Instr = Slice-Nummer). \
                 „→ Instr N“ macht den gewählten Slice zum Sample von Instrument N.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );

        // Geänderte Slices einmal ins Raster übernehmen (nicht während des
        // Ziehens — das spart die Kopie je Frame).
        if self.raster_schmutzig && self.zug_kante.is_none() {
            self.raster_aktualisieren();
            self.raster_schmutzig = false;
        }
        aktion
    }
}

/// Anzeige-Hüllkurve: je Kasten `(min, max)` über den zugehörigen Bereich.
fn huelle(proben: &[f32], kaesten: usize) -> Vec<(f32, f32)> {
    let n = kaesten.max(1);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let von = k * proben.len() / n;
        let bis = (((k + 1) * proben.len() / n).max(von + 1)).min(proben.len());
        if von >= proben.len() {
            out.push((0.0, 0.0));
            continue;
        }
        let (mut mn, mut mx) = (f32::MAX, f32::MIN);
        for s in &proben[von..bis] {
            mn = mn.min(*s);
            mx = mx.max(*s);
        }
        out.push((mn, mx));
    }
    out
}

/// `<App-Ordner>/module` — dort sucht der Player seine Module.
fn app_module_ordner() -> Result<std::path::PathBuf, String> {
    let app = crate::current_app()?;
    let basis = crate::insets::external_files_dir(&app)
        .ok_or_else(|| "kein App-Verzeichnis".to_string())?;
    let ordner = std::path::PathBuf::from(basis).join("module");
    std::fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    Ok(ordner)
}

/// `<App-Ordner>` (ohne Unterordner) für kleine Merker-Dateien.
fn app_ordner() -> Result<std::path::PathBuf, String> {
    let app = crate::current_app()?;
    let basis = crate::insets::external_files_dir(&app)
        .ok_or_else(|| "kein App-Verzeichnis".to_string())?;
    let ordner = std::path::PathBuf::from(basis);
    std::fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    Ok(ordner)
}

/// Kurzname einer Datei für die Anzeige.
fn kurz(pfad: &str) -> String {
    Path::new(pfad)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| pfad.to_string())
}
