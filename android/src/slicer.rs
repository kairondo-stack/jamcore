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
//! - **„Marker setzen"** teilt den Slice an der Auswahl und legt damit eine
//!   **neue** Schnittmarke an; **„Marker löschen"** entfernt die Marke an der
//!   Auswahl (die beiden Nachbar-Slices werden wieder eins). Beides ist
//!   rückgängig-fähig (ein Schritt, wie die Werkzeuge).
//! - Die **Nummernknöpfe** 1 … N (und „Vorhören") spielen einen Slice über
//!   `klang::puffer_spielen` — dieselbe Funktion, die auch das Rhythmus-Raster
//!   benutzt (Instrument-Quelle „Slices").

use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Sense};
use jamcore::audioedit;
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

/// Stand vor einer Bearbeitung für „Rückgängig“ (genau ein Schritt).
///
/// Warum die Schnitte mit im Schnappschuss liegen: Löschen und Zuschneiden
/// verschieben alle Proben-Indizes; ohne die alten Schnitte würde ein
/// Rückgängig zwar das Audio, aber nicht die Slice-Grenzen wiederherstellen.
struct Snapshot {
    proben: Vec<f32>,
    schnitte: Vec<Slice>,
    bereich: Option<(usize, usize)>,
    auswahl: usize,
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
    /// Ausdrückliche Bereichsauswahl in Proben `(von, bis)`; hat Vorrang vor
    /// dem gewählten Slice. `None` = der gewählte Slice ist der Bereich.
    bereich: Option<(usize, usize)>,
    /// Laufende Aufnahme (None = keine).
    aufnahme: Option<wavrec::Aufnahme>,
    status: String,
    /// Beim Ziehen: welcher Schnitt (= Ende der Slice i) wandert gerade?
    zug_kante: Option<usize>,
    /// Beim Ziehen im Modus „Bereich“: Startprobe unter dem Finger.
    zug_bereich_start: Option<usize>,
    /// Ziehen in der Wellenform: `true` = Bereich aufziehen, `false` =
    /// nächstgelegenen Schnitt verschieben (bisheriges Verhalten).
    bereich_ziehen: bool,
    /// Dauer der Ein-/Ausblendung in Millisekunden (10 … 1000).
    fade_ms: f32,
    /// Ein-Schritt-Gedächtnis vor der letzten Bearbeitung.
    rueckgaengig: Option<Snapshot>,
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
            bereich: None,
            aufnahme: None,
            status: "Audio öffnen oder aufnehmen — dann schneiden.".to_string(),
            zug_kante: None,
            zug_bereich_start: None,
            bereich_ziehen: true,
            fade_ms: 50.0,
            rueckgaengig: None,
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

    /// Sekundenposition einer Probe (mindestens 1 Hz als Nenner).
    fn sekunden(&self, probe: usize) -> f32 {
        probe as f32 / self.rate().max(1) as f32
    }

    /// Der aktive Bearbeitungsbereich in Proben: ausdrückliche Auswahl vor
    /// dem gewählten Slice, jeweils auf die aktuelle Länge geklemmt.
    ///
    /// Warum eine Stelle: alle Werkzeuge müssen denselben Bereich sehen, und
    /// nach einem Löschen/Zuschneiden kann eine alte Auswahl über das Ende
    /// hinauszeigen.
    fn aktiver_bereich(&self) -> Option<(usize, usize)> {
        let len = self.laenge();
        let (von, bis) = match self.bereich {
            Some((von, bis)) => (von, bis),
            None => {
                let s = self.schnitte.get(self.auswahl)?;
                (s.von, s.bis)
            }
        };
        let von = von.min(len);
        let bis = bis.min(len);
        (bis > von).then_some((von, bis))
    }

    /// Anzeigetext des Bereichs, z. B. „Bereich 1.23–2.45 s (1.22 s)“.
    fn bereich_text(&self, (von, bis): (usize, usize)) -> String {
        let (a, b) = (self.sekunden(von), self.sekunden(bis));
        format!("Bereich {a:.2}–{b:.2} s ({:.2} s)", b - a)
    }

    /// Stand vor einer Bearbeitung sichern (ersetzt den vorherigen).
    fn snapshot_setzen(&mut self) {
        let Some((proben, _)) = self.audio.as_ref() else {
            return;
        };
        self.rueckgaengig = Some(Snapshot {
            proben: proben.clone(),
            schnitte: self.schnitte.clone(),
            bereich: self.bereich,
            auswahl: self.auswahl,
        });
    }

    /// Wiedergabe anhalten und den Stand für „Rückgängig“ sichern.
    ///
    /// Warum vor jeder Bearbeitung: die klingenden Slice-Puffer gehören zum
    /// alten Audio; ein Schnitt mitten im Ton knackst sonst.
    fn bearbeitung_beginnen(&mut self) {
        crate::klang::stimmen_aus();
        self.snapshot_setzen();
    }

    /// Nach einer Bearbeitung: Hüllkurve, ggf. Schnitte und das Raster für den
    /// Rhythmus-Tab neu rechnen.
    fn nach_bearbeitung(&mut self, laenge_geaendert: bool) {
        if laenge_geaendert {
            // Alte Schnitte zeigen auf verschobene Proben — nach Löschen oder
            // Zuschneiden mit den aktuellen Einstellungen neu legen.
            let neu = match self.audio.as_ref() {
                Some((proben, rate)) => slice::schnitte(
                    proben,
                    *rate,
                    self.modus,
                    self.anzahl,
                    self.empfindlichkeit,
                ),
                None => vec![Slice { von: 0, bis: 0 }],
            };
            self.schnitte = neu;
            self.auswahl = 0;
            self.bereich = None;
        }
        self.huelle = huelle(self.proben(), HUELLEN_KAESTEN);
        self.raster_aktualisieren();
        self.raster_schmutzig = false;
    }

    /// Wo der aktive Bereich liegt — fürs Log (`Bereich` oder `Slice N`).
    fn bereich_herkunft(&self, von: usize) -> String {
        if self.bereich.is_some() {
            "Bereich".to_string()
        } else {
            format!("Slice {}", self.slice_bei(von) + 1)
        }
    }

    /// Umkehren (Reverse) des aktiven Bereichs.
    fn werkzeug_umkehren(&mut self) {
        let Some((von, bis)) = self.aktiver_bereich() else {
            self.status = "Kein Bereich zum Umkehren — Audio laden oder Slice wählen.".to_string();
            return;
        };
        let herkunft = self.bereich_herkunft(von);
        let (a, b) = (self.sekunden(von), self.sekunden(bis));
        self.bearbeitung_beginnen();
        if let Some((proben, _)) = self.audio.as_mut() {
            audioedit::umkehren(proben, von, bis);
        }
        self.nach_bearbeitung(false);
        self.status = format!("Umgekehrt: {a:.2}–{b:.2} s ({herkunft}).");
        info!("slicer: Umkehren {a:.2}-{b:.2} s ({herkunft})");
    }

    /// Nur den aktiven Bereich behalten.
    fn werkzeug_zuschneiden(&mut self) {
        let Some((von, bis)) = self.aktiver_bereich() else {
            self.status = "Kein Bereich zum Zuschneiden — Audio laden oder Slice wählen."
                .to_string();
            return;
        };
        let herkunft = self.bereich_herkunft(von);
        let (a, b) = (self.sekunden(von), self.sekunden(bis));
        self.bearbeitung_beginnen();
        if let Some((proben, _)) = self.audio.as_mut() {
            audioedit::zuschneiden(proben, von, bis);
        }
        self.nach_bearbeitung(true);
        let neu = self.laenge() as f32 / self.rate().max(1) as f32;
        self.status = format!("Zugeschnitten auf {a:.2}–{b:.2} s -> {neu:.2} s ({herkunft}).");
        info!("slicer: Zuschneiden {a:.2}-{b:.2} s -> {neu:.2} s");
    }

    /// Den aktiven Bereich entfernen (Rest rückt zusammen).
    fn werkzeug_bereich_loeschen(&mut self) {
        let Some((von, bis)) = self.aktiver_bereich() else {
            self.status = "Kein Bereich zum Löschen — Audio laden oder Slice wählen.".to_string();
            return;
        };
        let herkunft = self.bereich_herkunft(von);
        let (a, b) = (self.sekunden(von), self.sekunden(bis));
        let alt = self.laenge();
        self.bearbeitung_beginnen();
        let neu_proben = if let Some((proben, _)) = self.audio.as_mut() {
            audioedit::bereich_loeschen(proben, von, bis)
        } else {
            alt
        };
        self.nach_bearbeitung(true);
        let neu = neu_proben as f32 / self.rate().max(1) as f32;
        self.status = format!("Bereich gelöscht: {a:.2}–{b:.2} s -> {neu:.2} s ({herkunft}).");
        info!("slicer: Bereich loeschen {a:.2}-{b:.2} s -> {neu:.2} s");
    }

    /// Ein- oder Ausblenden über `fade_ms` am aktiven Bereich.
    fn werkzeug_blende(&mut self, ein: bool) {
        let Some((von, bis)) = self.aktiver_bereich() else {
            self.status = "Kein Bereich zum Blenden — Audio laden oder Slice wählen.".to_string();
            return;
        };
        let herkunft = self.bereich_herkunft(von);
        let rate = self.rate().max(1);
        let dauer = ((self.fade_ms.max(1.0) / 1000.0) * rate as f32).round() as usize;
        let dauer = dauer.max(1);
        let (a, b) = (self.sekunden(von), self.sekunden(bis));
        self.bearbeitung_beginnen();
        if let Some((proben, _)) = self.audio.as_mut() {
            if ein {
                audioedit::einblenden(proben, von, bis, dauer);
            } else {
                audioedit::ausblenden(proben, von, bis, dauer);
            }
        }
        self.nach_bearbeitung(false);
        let name = if ein { "Einblenden" } else { "Ausblenden" };
        self.status = format!(
            "{name}: {a:.2}–{b:.2} s über {:.0} ms ({herkunft}).",
            self.fade_ms
        );
        info!(
            "slicer: {name} {a:.2}-{b:.2} s ({:.0} ms)",
            self.fade_ms
        );
    }

    /// Den aktiven Bereich auf Spitze 1.0 normalisieren.
    fn werkzeug_normalisieren(&mut self) {
        let Some((von, bis)) = self.aktiver_bereich() else {
            self.status = "Kein Bereich zum Normalisieren — Audio laden oder Slice wählen."
                .to_string();
            return;
        };
        let herkunft = self.bereich_herkunft(von);
        let (a, b) = (self.sekunden(von), self.sekunden(bis));
        self.bearbeitung_beginnen();
        let faktor = if let Some((proben, _)) = self.audio.as_mut() {
            audioedit::normalisieren(proben, von, bis)
        } else {
            1.0
        };
        self.nach_bearbeitung(false);
        self.status = format!(
            "Normalisiert: {a:.2}–{b:.2} s, Faktor {faktor:.2} ({herkunft})."
        );
        info!("slicer: Normalisieren {a:.2}-{b:.2} s (Faktor {faktor:.2})");
    }

    /// Die letzte Bearbeitung zurücknehmen (genau ein Schritt).
    fn rueckgaengig(&mut self) {
        let Some(s) = self.rueckgaengig.take() else {
            self.status = "Nichts rückgängig zu machen.".to_string();
            return;
        };
        crate::klang::stimmen_aus();
        let rate = self.rate();
        self.audio = Some((s.proben, rate));
        self.schnitte = s.schnitte;
        self.bereich = s.bereich;
        self.auswahl = s.auswahl.min(self.schnitte.len().saturating_sub(1));
        self.huelle = huelle(self.proben(), HUELLEN_KAESTEN);
        self.raster_aktualisieren();
        self.raster_schmutzig = false;
        self.status = "Rückgängig: vorheriger Stand wiederhergestellt.".to_string();
        info!("slicer: Rueckgaengig");
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
        self.bereich = None;
        self.zug_bereich_start = None;
        self.rueckgaengig = None;
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

    /// Bezugsposition für „Marker setzen“/„Marker löschen“ in Proben
    /// (`None` = keine Auswahl).
    ///
    /// Warum so: der Slicer hat keinen eigenen Cursor/Playhead — „aktuell“ ist
    /// die Auswahl. Ein von Hand aufgezogener Bereich zählt ab seinem Anfang
    /// (dort hat der Finger die Stelle bestimmt, und sie ist fast nie schon
    /// eine Marke). Ohne Bereich ist der gewählte Slice die Auswahl; sein
    /// Anfang ist bereits eine Marke, an der keine neue entstehen könnte,
    /// deshalb die Mitte des Slice.
    fn marker_position(&self) -> Option<usize> {
        let len = self.laenge();
        let (von, bis) = match self.bereich {
            Some((von, bis)) => (von, bis),
            None => {
                let s = self.schnitte.get(self.auswahl)?;
                (s.von, s.bis)
            }
        };
        let (von, bis) = (von.min(len), bis.min(len));
        if bis <= von {
            return None;
        }
        Some(if self.bereich.is_some() {
            von
        } else {
            von + (bis - von) / 2
        })
    }

    /// Eine **neue** Schnittmarke an der Auswahl setzen: der Slice, in dem die
    /// Position liegt, wird geteilt (aus einem Slice werden zwei).
    ///
    /// Warum Teilen und nicht [`Self::schnitt_setzen`]: das verschiebt die
    /// nächstgelegene Marke, die Anzahl bleibt gleich. Der Wunsch war eine
    /// **neue** Marke („noch einen Marker für den Slice“). Der Kern hat kein
    /// `teile`; die Invariante der Liste (sortiert, lückenlos, mindestens ein
    /// Sample je Slice) bleibt hier erhalten, weil nur an einer Stelle
    /// `von < Position < bis` geteilt wird.
    ///
    /// Der Undo-Snapshot entsteht wie bei jeder Bearbeitung in
    /// [`Self::bearbeitung_beginnen`] — Rückgängig nimmt auch die Marke zurück.
    fn marker_setzen(&mut self) {
        if self.laenge() == 0 {
            self.status = "Kein Audio geladen — erst öffnen oder aufnehmen.".to_string();
            return;
        }
        let Some(probe) = self.marker_position() else {
            self.status = "Keine Auswahl für die Marke — erst einen Slice wählen.".to_string();
            return;
        };
        let k = self.slice_bei(probe);
        let Some(s) = self.schnitte.get(k).copied() else {
            return;
        };
        let sekunden = self.sekunden(probe);
        if probe <= s.von || probe >= s.bis {
            // Auf einer vorhandenen Grenze gibt es nichts zu teilen (und an
            // den Rändern der Probe erst recht nicht).
            self.status = format!("Bei {sekunden:.2} s ist schon eine Marke.");
            info!("slicer: Marke bei {sekunden:.2} s ist schon da");
            return;
        }
        self.bearbeitung_beginnen();
        self.schnitte[k].bis = probe;
        self.schnitte.insert(
            k + 1,
            Slice {
                von: probe,
                bis: s.bis,
            },
        );
        // Eine ausdrückliche Bereichsauswahl bleibt Auswahl; der gewählte
        // Slice bleibt der linke Teil (die Marke ist seine neue Grenze).
        if self.auswahl > k {
            self.auswahl += 1;
        }
        self.nach_bearbeitung(false);
        self.status = format!(
            "Marker bei {sekunden:.2} s gesetzt ({} Slices).",
            self.schnitte.len()
        );
        info!(
            "slicer: Marker bei {sekunden:.2} s gesetzt ({} Slices)",
            self.schnitte.len()
        );
    }

    /// Die Marke an der Auswahl löschen (die beiden Nachbar-Slices werden
    /// wieder einer).
    ///
    /// Welche Marke: die inneren Grenzen sind `schnitte[i].bis` für
    /// `i = 0 … n−2` (Anfang und Ende der Probe sind keine Schnittmarken).
    /// - von Hand aufgezogener Bereich: die erste Marke an/über seinem Anfang
    ///   (rechts davon), sonst die letzte;
    /// - gewählter Slice: die Marke an seinem Anfang (`auswahl > 0`), beim
    ///   ersten Slice die an seinem Ende — also immer die Marke, die zur
    ///   Auswahl gehört, und nicht zufällig die Nachbarin.
    ///
    /// [`slice::loesche`] gibt den Bereich des entfernten Slices an den
    /// vorherigen — genau dadurch verschwindet die Marke zwischen beiden.
    fn marker_loeschen(&mut self) {
        if self.laenge() == 0 {
            self.status = "Kein Audio geladen — erst öffnen oder aufnehmen.".to_string();
            return;
        }
        if self.schnitte.len() <= 1 {
            self.status = "Keine Marke zum Löschen — der letzte Slice bleibt.".to_string();
            return;
        }
        let k = if self.bereich.is_some() {
            let Some(probe) = self.marker_position() else {
                self.status = "Keine Auswahl für die Marke — erst einen Slice wählen.".to_string();
                return;
            };
            let mut k = self.schnitte.len() - 2;
            for i in 0..self.schnitte.len() - 1 {
                if self.schnitte[i].bis >= probe {
                    k = i;
                    break;
                }
            }
            k
        } else if self.auswahl > 0 {
            self.auswahl - 1
        } else {
            0
        };
        let sekunden = self.sekunden(self.schnitte[k].bis);
        self.bearbeitung_beginnen();
        slice::loesche(&mut self.schnitte, k + 1);
        self.auswahl = self.auswahl.min(self.schnitte.len() - 1);
        self.nach_bearbeitung(false);
        self.status = format!(
            "Marker bei {sekunden:.2} s gelöscht ({} Slices).",
            self.schnitte.len()
        );
        info!(
            "slicer: Marker bei {sekunden:.2} s geloescht ({} Slices)",
            self.schnitte.len()
        );
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
        // Ausdrückliche Bereichsauswahl: oranger Balken über den Slice-Farben,
        // damit „gewählter Slice“ (grün) und „Bereich“ (orange) unterscheidbar
        // bleiben. Die Feldwerte können nach einem Zuschnitt veraltet sein —
        // deshalb hier klemmen, nicht vertrauen.
        if let Some((von, bis)) = self.bereich {
            let von = von.min(len);
            let bis = bis.min(len);
            if bis > von {
                let x0 = rect.left() + von as f32 / len as f32 * rect.width();
                let x1 = rect.left() + bis as f32 / len as f32 * rect.width();
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, rect.top()),
                        egui::pos2(x1, rect.bottom()),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(250, 170, 60, 60),
                );
                for x in [x0, x1] {
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        egui::Stroke::new(2.0, Color32::from_rgb(250, 190, 80)),
                    );
                }
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
                if self.bereich_ziehen {
                    // Neuer Bereich ab der Startprobe; eine alte Auswahl wird
                    // ersetzt, nicht erweitert (ein Finger, ein Bereich).
                    self.zug_bereich_start = Some(pr);
                    self.bereich = None;
                    self.status = "Bereich aufziehen …".to_string();
                } else {
                    self.zug_kante = self.naechster_schnitt(pr);
                    if let Some(k) = self.zug_kante {
                        self.status = format!("Schnitt {} ziehen …", k + 1);
                    }
                }
            }
        }
        if response.dragged() {
            if let Some(pr) = probe {
                if let Some(start) = self.zug_bereich_start {
                    let (von, bis) = (start.min(pr), start.max(pr));
                    // Ein Punkt ist noch kein Bereich; erst ab zwei Proben
                    // wird die Auswahl sichtbar und bearbeitbar.
                    self.bereich = (bis > von).then_some((von, bis));
                } else if let Some(k) = self.zug_kante {
                    let gesamt = self.laenge();
                    slice::verschiebe(&mut self.schnitte, k, pr, gesamt);
                }
            }
        }
        if response.drag_stopped() {
            if self.zug_bereich_start.take().is_some() {
                self.status = match self.aktiver_bereich() {
                    Some(bereich) => self.bereich_text(bereich),
                    None => "Auswahl aufgehoben (zu kurz gezogen).".to_string(),
                };
            }
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

        // Bereichsanzeige und Zieh-Modus direkt unter der Wellenform: der
        // Bereich ist das Ziel aller Werkzeuge, ohne Auswahl gilt der Slice.
        let aktiver = self.aktiver_bereich();
        let auswahl_aktiv = self.bereich.is_some();
        let bereich_text = aktiver.map(|b| self.bereich_text(b));
        let mut auswahl_aufheben = false;
        ui.horizontal_wrapped(|ui| {
            match &bereich_text {
                Some(text) => {
                    ui.label(
                        RichText::new(text)
                            .small()
                            .color(Color32::from_rgb(245, 195, 100)),
                    );
                }
                None => {
                    ui.label(
                        RichText::new("Kein Bereich — Audio laden oder Slice wählen.")
                            .small()
                            .color(Color32::GRAY),
                    );
                }
            }
            if ui
                .add_enabled(auswahl_aktiv, egui::Button::new("Auswahl aufheben"))
                .clicked()
            {
                auswahl_aufheben = true;
            }
            ui.separator();
            ui.label(RichText::new("Ziehen:").small());
            if ui
                .selectable_label(self.bereich_ziehen, "Bereich")
                .clicked()
            {
                self.bereich_ziehen = true;
            }
            if ui
                .selectable_label(!self.bereich_ziehen, "Schnitt")
                .clicked()
            {
                self.bereich_ziehen = false;
            }
        });
        if auswahl_aufheben {
            self.bereich = None;
            self.status = "Auswahl aufgehoben — der gewählte Slice gilt als Bereich.".to_string();
            info!("slicer: Auswahl aufgehoben");
        }

        // Werkzeug-Zeile: bearbeitet die Auswahl, sonst den gewählten Slice.
        // Auch bei leerem (aber geladenem) Audio sichtbar, damit „Rückgängig“
        // nach dem Löschen des letzten Rests erreichbar bleibt.
        let hat_audio = self.audio.is_some();
        let bereich_da = aktiver.is_some();
        if hat_audio {
            let (mut umkehren, mut zuschneiden, mut loeschen) = (false, false, false);
            let (mut einblenden, mut ausblenden, mut normalisieren) = (false, false, false);
            let mut rueckgaengig = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Bearbeiten").small());
                if ui
                    .add_enabled(bereich_da, egui::Button::new("Umkehren"))
                    .clicked()
                {
                    umkehren = true;
                }
                if ui
                    .add_enabled(bereich_da, egui::Button::new("Zuschneiden"))
                    .clicked()
                {
                    zuschneiden = true;
                }
                if ui
                    .add_enabled(bereich_da, egui::Button::new("Bereich löschen"))
                    .clicked()
                {
                    loeschen = true;
                }
                ui.separator();
                if ui
                    .add_enabled(bereich_da, egui::Button::new("Einblenden"))
                    .clicked()
                {
                    einblenden = true;
                }
                if ui
                    .add_enabled(bereich_da, egui::Button::new("Ausblenden"))
                    .clicked()
                {
                    ausblenden = true;
                }
                ui.add(
                    egui::DragValue::new(&mut self.fade_ms)
                        .range(10.0..=1000.0)
                        .speed(5.0)
                        .suffix(" ms"),
                )
                .on_hover_text("Dauer für Ein-/Ausblenden");
                ui.separator();
                if ui
                    .add_enabled(bereich_da, egui::Button::new("Normalisieren"))
                    .clicked()
                {
                    normalisieren = true;
                }
                if ui
                    .add_enabled(self.rueckgaengig.is_some(), egui::Button::new("Rückgängig"))
                    .clicked()
                {
                    rueckgaengig = true;
                }
            });
            // Erst nach der Zeile ausführen: die Werkzeuge tauschen Audio und
            // Raster, das soll nicht mitten in egui-Aufrufen passieren.
            if umkehren {
                self.werkzeug_umkehren();
            }
            if zuschneiden {
                self.werkzeug_zuschneiden();
            }
            if loeschen {
                self.werkzeug_bereich_loeschen();
            }
            if einblenden {
                self.werkzeug_blende(true);
            }
            if ausblenden {
                self.werkzeug_blende(false);
            }
            if normalisieren {
                self.werkzeug_normalisieren();
            }
            if rueckgaengig {
                self.rueckgaengig();
            }
        }

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
                        // Slice-Wahl ersetzt eine ausdrückliche Auswahl — sonst
                        // bliebe der alte Bereich unsichtbar aktiv.
                        self.bereich = None;
                        self.vorhoeren(i);
                    }
                }
            });
        }
        let bereit = !self.schnitte.is_empty();
        // Schnittmarken von Hand: „Marker setzen“ teilt die Auswahl (eine
        // Marke mehr), „Marker löschen“ nimmt die Marke an der Auswahl weg
        // (eine Marke weniger). Beide ändern kein Audio, sichern aber wie die
        // Werkzeuge den Undo-Stand.
        ui.horizontal_wrapped(|ui| {
            let hat_audio = self.laenge() > 0;
            if ui
                .add_enabled(hat_audio, egui::Button::new("Marker setzen"))
                .on_hover_text("Neue Schnittmarke an der Auswahl (Bereichsanfang bzw. Mitte des Slice)")
                .clicked()
            {
                self.marker_setzen();
            }
            if ui
                .add_enabled(
                    hat_audio && self.schnitte.len() > 1,
                    egui::Button::new("Marker löschen"),
                )
                .on_hover_text("Marke an der Auswahl entfernen (Nachbar-Slices werden eins)")
                .clicked()
            {
                self.marker_loeschen();
            }
            ui.label(
                RichText::new("Marke an der Auswahl")
                    .small()
                    .color(Color32::GRAY),
            );
        });
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
                "Wellenform: tippen setzt den Schnitt, ziehen zieht einen Bereich auf (Modus \
                 „Bereich“) oder verschiebt den nächsten Schnitt (Modus „Schnitt“), langes \
                 Tippen löscht den Slice. „Marker setzen“ teilt die Auswahl an einer neuen \
                 Marke (Bereichsanfang bzw. Mitte des Slice), „Marker löschen“ entfernt die \
                 Marke an der Auswahl; „× löschen“ entfernt dagegen den ganzen Slice. \
                 „Bearbeiten“ wirkt auf die Auswahl, sonst auf den \
                 gewählten Slice; „Rückgängig“ nimmt den letzten Schritt zurück. „Als .it \
                 sichern“ legt die Slices als Samples in <App-Ordner>/module/<Name>.it ab \
                 (im Player-Tab ladbar); das Rhythmus-Raster spielt sie über die Klangquelle \
                 „Slices“ (Instr = Slice-Nummer). „→ Instr N“ macht den gewählten Slice zum \
                 Sample von Instrument N.",
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
