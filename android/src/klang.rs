//! Polyphoner Tongenerator: kurze Wellen-Toene ueber AudioTrack.
//!
//! Ein Thread mischt die klingenden Stimmen in Bloecken (~10 ms) und schreibt
//! sie in den AudioTrack. Stimmenzahl (1–256) und Tonlaenge (Notenwert 1/64
//! bis 4 Takte, ms haengen am BPM) sind einstellbar; bei Ueberlast weicht die
//! **aelteste** Stimme. Die Mixer-Mathematik liegt in `jamcore::klang`
//! (`Stimme`, `mischen`, `Notenwert`), damit sie ohne Android testbar ist.
//!
//! Dritter Auftragstyp neben Note und Dauerton: fertige PCM-Puffer
//! ([`puffer_spielen`], z. B. Slices aus dem Slicer). Sie klingen **einmal**
//! (keine Schleife), werden auf die Ausgaberate umgerechnet und **übereinander**
//! gemischt: eine Stimme je Aufruf, damit ein neues Sample das vorherige nicht
//! abschneidet. Bei Überlast (mehr als [`PUFFER_MAX`] Stimmen) blendet die
//! älteste in wenigen Millisekunden aus, statt hart zu enden; erst über der
//! harten Grenze fällt eine Stimme wirklich weg. So bleibt der Klang polyphon
//! und der Speicher begrenzt (jede Stimme hält ihren Puffer).
//!
//! Dazu kommen die dauerhaften **Linien-/Bereichsstimmen** des
//! Oszillator-Scans ([`linienton_setzen`]): ein ganzer Satz (Hz, Gain), den
//! jeder Aufruf ersetzt — im Bild-Tab klingen sie, solange die Linie bzw. der
//! x-Bereich steht.

use std::collections::VecDeque;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};

use jamcore::klang::{frequenz, mischen, Notenwert, Stimme, Welle};
use jamcore::lissa::dauerton_mischen;
use log::info;

const RATE: c_int = 44100;
const STREAM_MUSIC: i32 = 3;
const CHANNEL_OUT_STEREO: i32 = 12;
const ENCODING_PCM_16BIT: i32 = 2;
const MODE_STREAM: i32 = 1;
/// Blockgroesse des Mischers: ~10 ms. Fluessig genug und wenige JNI-Aufrufe.
const BLOCK: usize = 441;
/// So viele fertige Puffer klingen hoechstens gleichzeitig (eine Stimme je
/// Aufruf). 16 statt 8: ein Sample, das laenger als acht Schritte klingt, wurde
/// sonst mitten im Klang abgeschnitten, sobald die naechste Note kam.
const PUFFER_MAX: usize = 16;
/// Harte Obergrenze **waehrend des Ausblendens**: verdraengte Stimmen klingen
/// noch ein paar Millisekunden aus, statt sofort zu enden (kein Klick). Erst
/// darueber faellt wirklich eine Stimme weg, damit der Speicher begrenzt bleibt.
const PUFFER_HART: usize = PUFFER_MAX + 8;
/// Ausblendzeit einer verdraengten Stimme.
const AUSBLEND_MS: f64 = 5.0;

/// Ein Auftrag an den Klang-Thread.
enum Auftrag {
    /// Kurzer Wellen-Ton (Tongenerator, MIDI-Note + Wellenform).
    Note(i32, Welle),
    /// Fertiger PCM-Puffer (mono, 16 Bit), einmal abspielen.
    Puffer { pcm: Vec<i16>, rate: u32, gain: f32 },
}

/// Ein Puffer, der gerade klingt: Quelle, Position und Schrittweite fuer die
/// Umrechnung auf die Ausgaberate (lineare Interpolation).
struct KlingenderPuffer {
    pcm: Vec<i16>,
    /// Position in Quell-Samples (gebrochen, fuer die Interpolation).
    pos: f64,
    /// So viele Quell-Samples je Ausgabe-Sample.
    schritt: f64,
    gain: f32,
    /// Rest-Lautstaerke beim Verdraengen (1.0 = unveraendert). Sie faellt je
    /// Ausgabe-Sample um `abnahme`, damit die Stimme weich endet statt zu
    /// klicken, wenn eine neue Note denselben Platz braucht.
    rest: f32,
    /// Abnahme je Ausgabe-Sample; 0.0, solange der Puffer nicht verdraengt ist.
    abnahme: f32,
}

impl KlingenderPuffer {
    fn neu(pcm: Vec<i16>, rate: u32, gain: f32) -> Self {
        let schritt = if rate == 0 {
            1.0
        } else {
            rate as f64 / RATE as f64
        };
        Self {
            pcm,
            pos: 0.0,
            schritt,
            gain: gain.clamp(0.0, 1.0),
            rest: 1.0,
            abnahme: 0.0,
        }
    }

    /// Sanft ausblenden statt hart abschneiden: die Stimme faellt nach
    /// [`AUSBLEND_MS`] aus der Schlange. Ein zweiter Aufruf startet das
    /// Ausblenden nicht neu — sonst bliebe eine alte Stimme laenger als noetig.
    fn verdraengen(&mut self) {
        if self.abnahme == 0.0 {
            self.abnahme = (1.0 / (RATE as f64 * AUSBLEND_MS / 1000.0)) as f32;
        }
    }

    fn verdraengt(&self) -> bool {
        self.abnahme > 0.0
    }

    fn fertig(&self) -> bool {
        self.rest <= 0.0 || self.pos >= self.pcm.len() as f64
    }

    /// In den Block mischen (addieren) und die Position weiterruecken. Am Ende
    /// des Puffers bleibt der Rest des Blocks still.
    fn mischen(&mut self, block: &mut [i16]) {
        let n = self.pcm.len();
        if n == 0 {
            self.pos = 0.0;
            return;
        }
        for wert in block.iter_mut() {
            if self.pos >= (n - 1) as f64 {
                self.pos = n as f64;
                break;
            }
            let i = self.pos.floor() as usize; // 0 … n-2 (siehe Abbruch oben)
            let anteil = (self.pos - i as f64) as f32;
            let quelle =
                self.pcm[i] as f32 + (self.pcm[i + 1] as f32 - self.pcm[i] as f32) * anteil;
            let add = (quelle * self.gain * self.rest) as i32;
            *wert = (*wert as i32 + add).clamp(-32768, 32767) as i16;
            self.pos += self.schritt;
            if self.abnahme > 0.0 {
                self.rest -= self.abnahme;
                if self.rest <= 0.0 {
                    self.rest = 0.0;
                    self.pos = n as f64;
                    break;
                }
            }
        }
        if self.pos >= (n - 1) as f64 {
            self.pos = n as f64;
        }
    }
}

/// Einstellungen des Tongenerators (Regler im Rhythmus-Tab und in Bild 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Einstellungen {
    /// So viele Stimmen klingen hoechstens gleichzeitig (1–256).
    pub stimmen: usize,
    /// Tonlaenge als Notenwert (ms haengen am BPM).
    pub notenwert: Notenwert,
    /// Tempo fuer die ms-Anzeige und die Tonlaenge.
    pub bpm: f32,
}

impl Default for Einstellungen {
    fn default() -> Self {
        Self {
            stimmen: 8,
            notenwert: Notenwert::Sechzehntel,
            bpm: 120.0,
        }
    }
}

static EINSTELLUNGEN: OnceLock<Mutex<Einstellungen>> = OnceLock::new();

/// Zähler: springt, wenn alle Tongenerator-Stimmen sofort verstummen sollen
/// („Ton aus" im Bild-Tab). Der Klang-Thread leert dann seine Stimmenliste.
static STOP_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Alle klingenden Tongenerator-Stimmen sofort beenden.
pub fn stimmen_aus() {
    STOP_GENERATION.fetch_add(1, Ordering::Relaxed);
}

fn slot() -> &'static Mutex<Einstellungen> {
    EINSTELLUNGEN.get_or_init(|| Mutex::new(Einstellungen::default()))
}

/// Zwei Dauertöne (Lissa-Tab: X- und Y-Stimme der Figur). `None` = aus.
static DAUERTON: OnceLock<Mutex<Option<[f32; 2]>>> = OnceLock::new();

/// Dauerhafte Stimmen des Oszillator-Scans (Bild-Tab: Linie/x-Bereich), je
/// Stimme (Hz, Gain). Jeder Aufruf **ersetzt** den ganzen Satz — die Linie
/// klingt nur solange sie steht; `None`/leer schaltet ab.
static LINIENTON: OnceLock<Mutex<Option<Vec<(f32, f32)>>>> = OnceLock::new();

fn linienton_slot() -> &'static Mutex<Option<Vec<(f32, f32)>>> {
    LINIENTON.get_or_init(|| Mutex::new(None))
}

/// Die Stimmen des Linien-/Bereichstons setzen (Hz, Gain je Stimme) oder mit
/// `None` abschalten. Der nächste Aufruf ersetzt den vorigen Satz; der
/// Klang-Thread mischt sie wie die Lissa-Dauertöne dauerhaft. Startet den
/// Klang-Thread, wenn er noch nicht läuft.
pub fn linienton_setzen(stimmen: Option<Vec<(f32, f32)>>) {
    let stimmen = stimmen.filter(|s| !s.is_empty());
    let an = stimmen.is_some();
    if let Ok(mut l) = linienton_slot().lock() {
        *l = stimmen;
    }
    if an {
        let mut guard = match spieler_slot().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        sicherstellen(&mut guard);
    }
}

fn linienton_aktuell() -> Option<Vec<(f32, f32)>> {
    linienton_slot().lock().ok().and_then(|l| l.clone())
}

/// Die Linien-/Bereichsstimmen auf einen Block addieren. `phasen` wächst mit
/// der Stimmenzahl; die Phasen der schon klingenden Stimmen laufen weiter
/// (kein Klick, wenn die Linie die Note wechselt). Kopfraum über die
/// Stimmenzahl, damit ein Akkord nicht übersteuert.
fn linienton_mischen(
    puffer: &mut [i16],
    phasen: &mut Vec<f64>,
    stimmen: &[(f32, f32)],
    rate: u32,
    gain: f32,
) {
    if rate == 0 || stimmen.is_empty() {
        return;
    }
    phasen.resize(stimmen.len(), 0.0);
    let kopf = 1.0 / (stimmen.len() as f64).sqrt();
    let schritte: Vec<f64> = stimmen
        .iter()
        .map(|(f, _)| (*f as f64 / rate as f64).clamp(0.0, 0.5))
        .collect();
    let staerken: Vec<f64> = stimmen
        .iter()
        .map(|(_, g)| (*g as f64).clamp(0.0, 1.0))
        .collect();
    let g = if gain.is_finite() {
        gain.clamp(0.0, 1.0) as f64 * kopf
    } else {
        0.0
    };
    for wert in puffer.iter_mut() {
        let mut summe = 0.0f64;
        for (i, s) in staerken.iter().enumerate() {
            summe += (phasen[i] * std::f64::consts::TAU).sin() * s;
        }
        let add = (summe * g * 32767.0) as i32;
        *wert = (*wert as i32 + add).clamp(-32768, 32767) as i16;
        for (i, s) in schritte.iter().enumerate() {
            phasen[i] = (phasen[i] + s).fract();
        }
    }
}

fn dauerton_slot() -> &'static Mutex<Option<[f32; 2]>> {
    DAUERTON.get_or_init(|| Mutex::new(None))
}

/// Die zwei Dauertöne setzen (Hz) oder mit `None` abschalten. Startet den
/// Klang-Thread, wenn er noch nicht läuft.
pub fn dauerton_setzen(frequenzen: Option<[f32; 2]>) {
    if let Ok(mut d) = dauerton_slot().lock() {
        *d = frequenzen;
    }
    if frequenzen.is_some() {
        let mut guard = match spieler_slot().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        sicherstellen(&mut guard);
    }
}

fn dauerton_aktuell() -> Option<[f32; 2]> {
    dauerton_slot().lock().ok().and_then(|d| *d)
}

/// Aktuelle Einstellungen (fuer die Anzeige der ms).
pub fn einstellungen() -> Einstellungen {
    slot().lock().map(|e| *e).unwrap_or_default()
}

/// Stimmenzahl setzen (1–256).
pub fn stimmen_setzen(anzahl: usize) {
    if let Ok(mut e) = slot().lock() {
        e.stimmen = anzahl.clamp(1, 256);
    }
}

/// Tonlaenge als Notenwert setzen.
pub fn notenwert_setzen(notenwert: Notenwert) {
    if let Ok(mut e) = slot().lock() {
        e.notenwert = notenwert;
    }
}

/// Tempo fuer die ms-Rechnung setzen.
pub fn bpm_setzen(bpm: f32) {
    if let Ok(mut e) = slot().lock() {
        e.bpm = bpm.clamp(20.0, 300.0);
    }
}

pub struct Spieler {
    tx: Sender<Auftrag>,
}

static SPIELER: OnceLock<Mutex<Option<Spieler>>> = OnceLock::new();

fn spieler_slot() -> &'static Mutex<Option<Spieler>> {
    SPIELER.get_or_init(|| Mutex::new(None))
}

/// Den Klang-Thread starten, falls er noch nicht läuft.
fn sicherstellen(guard: &mut Option<Spieler>) {
    if guard.is_none() {
        match starten() {
            Ok(neu) => *guard = Some(neu),
            Err(e) => info!("klang: Start fehlgeschlagen: {e}"),
        }
    }
}

/// Spielt eine MIDI-Note mit gewaehlter Wellenform an.
pub fn spielen_welle(midi: i32, welle: Welle) {
    let mut guard = match spieler_slot().lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    sicherstellen(&mut guard);
    if let Some(spieler) = guard.as_ref() {
        let _ = spieler.tx.send(Auftrag::Note(midi, welle));
    }
}

/// Einen fertigen PCM-Puffer (mono, 16 Bit) **einmal** abspielen — für Slices
/// aus dem Slicer. Läuft **übereinander** mit den Tongenerator-Stimmen und mit
/// weiteren Puffern (eine Stimme je Aufruf, bis [`PUFFER_MAX`]); ein neuer
/// Aufruf schneidet den vorherigen also nicht ab. Bei Überlast blendet die
/// älteste Stimme kurz aus.
pub fn puffer_spielen(pcm: Vec<i16>, rate: u32, gain: f32) {
    if pcm.is_empty() || rate == 0 {
        info!(
            "klang: Puffer leer/ohne Rate verworfen ({} Samples, {} Hz)",
            pcm.len(),
            rate
        );
        return;
    }
    info!("klang: Puffer an ({} Samples, {} Hz)", pcm.len(), rate);
    let mut guard = match spieler_slot().lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    sicherstellen(&mut guard);
    let Some(spieler) = guard.as_ref() else {
        info!("klang: Puffer ohne Spieler verworfen");
        return;
    };
    // Scheitert das Senden, ist der Klang-Thread beendet (z. B. nach einem
    // Schreibfehler). Dann neu starten und den Puffer noch einmal schicken —
    // sonst bliebe der Ton für den Rest der Sitzung stumm.
    match spieler.tx.send(Auftrag::Puffer { pcm, rate, gain }) {
        Ok(()) => {}
        Err(zurueck) => {
            info!("klang: Thread weg — Neustart");
            *guard = None;
            sicherstellen(&mut guard);
            if let Some(spieler) = guard.as_ref() {
                let _ = spieler.tx.send(zurueck.0);
            }
        }
    }
}

fn starten() -> Result<Spieler, String> {
    let app = crate::current_app()?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|e| format!("JavaVM: {e}"))?;
    let vm = std::sync::Arc::new(vm);
    let (tx, rx) = channel::<Auftrag>();
    std::thread::Builder::new()
        .name("jam-klang".to_string())
        .spawn(move || {
            if let Err(e) = schleife(&vm, rx) {
                info!("klang: Schleife beendet: {e}");
            }
        })
        .map_err(|e| format!("Klang-Thread: {e}"))?;
    info!("klang: polyphoner Spieler bereit ({RATE} Hz, Block {BLOCK})");
    Ok(Spieler { tx })
}

/// Die Schleife: neue Auftraege einsammeln, Stimmen pflegen, Bloecke mischen.
fn schleife(vm: &std::sync::Arc<jni::JavaVM>, rx: Receiver<Auftrag>) -> Result<(), String> {
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let (track, array) = create_track(&mut env)?;
    let mut stimmen: Vec<Stimme> = Vec::new();
    let mut puffer_schlange: VecDeque<KlingenderPuffer> = VecDeque::new();
    let mut puffer = vec![0i16; BLOCK];
    let mut welle = Welle::Sinus;
    let mut phasen = [0.0f64; 2];
    let mut dauerton_an = false;
    let mut linien_phasen: Vec<f64> = Vec::new();
    let mut linienton_an = false;
    let mut stop_gesehen = STOP_GENERATION.load(Ordering::Relaxed);
    loop {
        // „Ton aus": alle klingenden Stimmen sofort beenden.
        let stop = STOP_GENERATION.load(Ordering::Relaxed);
        if stop != stop_gesehen {
            stop_gesehen = stop;
            if !stimmen.is_empty() || !puffer_schlange.is_empty() {
                info!(
                    "klang: alle Stimmen aus ({} beendet, {} Puffer verworfen)",
                    stimmen.len(),
                    puffer_schlange.len()
                );
                stimmen.clear();
                puffer_schlange.clear();
            }
        }
        // Dauerton des Lissa-Tabs: an/aus im Log belegen und Phasen zurücksetzen.
        let dauerton = dauerton_aktuell();
        match (dauerton_an, dauerton) {
            (false, Some(f)) => {
                info!("klang: Dauerton an (X {:.1} Hz, Y {:.1} Hz)", f[0], f[1]);
                dauerton_an = true;
            }
            (true, None) => {
                info!("klang: Dauerton aus");
                dauerton_an = false;
                phasen = [0.0; 2];
            }
            _ => {}
        }
        // Linien-/Bereichston des Bild-Tabs (Oszillator-Scan): der Satz wird
        // bei jedem Aufruf ersetzt, hier nur der Übergang protokolliert.
        let linienton = linienton_aktuell();
        match (linienton_an, linienton.as_ref()) {
            (false, Some(s)) => {
                info!("klang: Linienton an ({} Stimmen)", s.len());
                linienton_an = true;
            }
            (true, None) => {
                info!("klang: Linienton aus");
                linienton_an = false;
                linien_phasen.clear();
            }
            _ => {}
        }
        // Klingt nichts, wird gewartet statt gedreht; klingt etwas, werden
        // alle wartenden Auftraege sofort aufgenommen.
        let leer = stimmen.is_empty()
            && dauerton.is_none()
            && linienton.is_none()
            && puffer_schlange.is_empty();
        let erster = if leer {
            match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(auftrag) => Some(auftrag),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            rx.try_recv().ok()
        };
        let mut auftrag = erster;
        while let Some(neu) = auftrag {
            match neu {
                Auftrag::Note(midi, neue_welle) => {
                    welle = neue_welle;
                    let e = einstellungen();
                    let laenge_ms = e.notenwert.ms(e.bpm).max(1);
                    stimmen.push(Stimme::neu(frequenz(midi), RATE as u32, laenge_ms));
                    // Ueberlast: die aelteste Stimme weicht.
                    while stimmen.len() > e.stimmen.max(1) {
                        stimmen.remove(0);
                    }
                }
                Auftrag::Puffer { pcm, rate, gain } => {
                    let laenge = pcm.len();
                    puffer_schlange.push_back(KlingenderPuffer::neu(pcm, rate, gain));
                    // Ueberlast: die aeltesten Stimmen weich ausblenden, damit
                    // das vorherige Sample nicht mitten im Klang abbricht. Nur
                    // ueber der harten Grenze (Ausblendende mitgezaehlt) faellt
                    // wirklich eine weg — der Speicher bleibt begrenzt.
                    while puffer_schlange.len() > PUFFER_HART {
                        puffer_schlange.pop_front();
                    }
                    let ueber = puffer_schlange.len().saturating_sub(PUFFER_MAX);
                    let mut frisch_ausgeblendet = 0;
                    for i in 0..ueber {
                        if let Some(p) = puffer_schlange.get_mut(i) {
                            if !p.verdraengt() {
                                p.verdraengen();
                                frisch_ausgeblendet += 1;
                            }
                        }
                    }
                    info!(
                        "klang: Puffer klingt ({laenge} Samples, {rate} Hz, {} Stimmen)",
                        puffer_schlange.len()
                    );
                    if frisch_ausgeblendet > 0 {
                        info!(
                            "klang: {} Slice-Stimmen (Grenze {PUFFER_MAX}) — \
                             {frisch_ausgeblendet} blendet in {AUSBLEND_MS:.0} ms aus",
                            puffer_schlange.len()
                        );
                    }
                }
            }
            auftrag = rx.try_recv().ok();
        }
        if stimmen.is_empty()
            && dauerton.is_none()
            && linienton.is_none()
            && puffer_schlange.is_empty()
        {
            continue;
        }
        // Kopfraum: wenige Stimmen bleiben laut, viele steuern sich nicht zu.
        let gain = 1.0 / (stimmen.len().max(1) as f32).sqrt();
        if stimmen.is_empty() {
            // Nur der Dauerton oder nur Puffer: den Block leeren (mischen
            // wuerde ihn fuellen).
            puffer.iter_mut().for_each(|s| *s = 0);
        } else {
            mischen(&mut puffer, &mut stimmen, RATE as u32, welle, gain);
        }
        if let Some(f) = dauerton {
            // Zwei Stimmen mit Kopfraum (0,25 je Stimme), additiv zu den Noten.
            dauerton_mischen(&mut puffer, &mut phasen, f, RATE as u32, 0.25);
        }
        if let Some(linie) = linienton.as_ref() {
            // Oszillator-Scan: der ganze Satz klingt dauerhaft (Kopfraum
            // rechnet `linienton_mischen` über die Stimmenzahl).
            linienton_mischen(&mut puffer, &mut linien_phasen, linie, RATE as u32, 0.5);
        }
        // Slices (und andere fertige Puffer) einmal abspielen; fertige fallen
        // aus der Schlange, der Rest rueckt weiter.
        let mut k = 0;
        while k < puffer_schlange.len() {
            puffer_schlange[k].mischen(&mut puffer);
            if puffer_schlange[k].fertig() {
                puffer_schlange.remove(k);
            } else {
                k += 1;
            }
        }
        if !block_schreiben(&mut env, &track, &array, &puffer) {
            return Ok(());
        }
    }
    Ok(())
}

/// Einen gemischten Block als Stereo-PCM16 in den AudioTrack schreiben
/// (blockiert). `false`, wenn der JNI-Aufruf gescheitert ist.
fn block_schreiben(
    env: &mut jni::JNIEnv,
    track: &jni::objects::GlobalRef,
    array: &jni::objects::GlobalRef,
    puffer: &[i16],
) -> bool {
    let mut bytes: Vec<i8> = Vec::with_capacity(puffer.len() * 4);
    for s in puffer {
        let (lo, hi) = (s.to_le_bytes()[0] as i8, s.to_le_bytes()[1] as i8);
        bytes.push(lo);
        bytes.push(hi);
        bytes.push(lo);
        bytes.push(hi);
    }
    let jarray: &jni::objects::JPrimitiveArray<i8> = array.as_obj().into();
    if env.set_byte_array_region(jarray, 0, &bytes).is_err() {
        crate::jniutil::clear(env);
        return true; // nur dieses Array fehlt, weiter versuchen
    }
    let wrote = env.call_method(
        track.as_obj(),
        "write",
        "([BII)I",
        &[
            jni::objects::JValue::Object(array.as_ref()),
            jni::objects::JValue::Int(0),
            jni::objects::JValue::Int(bytes.len() as i32),
        ],
    );
    if let Err(e) = wrote {
        let detail = crate::jniutil::describe_and_clear(env);
        info!("klang: write fehlgeschlagen: {detail} / {e}");
        return false;
    }
    true
}

fn create_track(
    env: &mut jni::JNIEnv,
) -> Result<(jni::objects::GlobalRef, jni::objects::GlobalRef), String> {
    use jni::objects::JValue;

    let class = env
        .find_class("android/media/AudioTrack")
        .map_err(|e| format!("AudioTrack-Klasse: {e}"))?;
    let bytes = (RATE as usize * 4 / 4) as i32; // ~250 ms Puffer
    let track = env
        .new_object(
            &class,
            "(IIIIII)V",
            &[
                JValue::Int(STREAM_MUSIC),
                JValue::Int(RATE),
                JValue::Int(CHANNEL_OUT_STEREO),
                JValue::Int(ENCODING_PCM_16BIT),
                JValue::Int(bytes),
                JValue::Int(MODE_STREAM),
            ],
        )
        .map_err(|e| {
            let detail = crate::jniutil::describe_and_clear(env);
            format!("AudioTrack(): {e} / {detail}")
        })?;
    if env.call_method(&track, "play", "()V", &[]).is_err() {
        let detail = crate::jniutil::describe_and_clear(env);
        return Err(format!("AudioTrack.play: {detail}"));
    }
    // Das Array fasst genau einen Mischblock (stereo PCM16).
    let array_len = BLOCK as i32 * 4;
    let array = env
        .new_byte_array(array_len)
        .map_err(|e| format!("byte[]: {e}"))?;
    let track = env
        .new_global_ref(&track)
        .map_err(|e| format!("global track: {e}"))?;
    let array = env
        .new_global_ref(&array)
        .map_err(|e| format!("global array: {e}"))?;
    Ok((track, array))
}
