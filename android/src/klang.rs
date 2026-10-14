//! Polyphoner Tongenerator: kurze Wellen-Toene ueber AudioTrack.
//!
//! Ein Thread mischt die klingenden Stimmen in Bloecken (~10 ms) und schreibt
//! sie in den AudioTrack. Stimmenzahl (1–256) und Tonlaenge (Notenwert 1/64
//! bis 4 Takte, ms haengen am BPM) sind einstellbar; bei Ueberlast weicht die
//! **aelteste** Stimme. Die Mixer-Mathematik liegt in `jamcore::klang`
//! (`Stimme`, `mischen`, `Notenwert`), damit sie ohne Android testbar ist.

use std::os::raw::c_int;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};

use jamcore::klang::{frequenz, mischen, Notenwert, Stimme, Welle};
use log::info;

const RATE: c_int = 44100;
const STREAM_MUSIC: i32 = 3;
const CHANNEL_OUT_STEREO: i32 = 12;
const ENCODING_PCM_16BIT: i32 = 2;
const MODE_STREAM: i32 = 1;
/// Blockgroesse des Mischers: ~10 ms. Fluessig genug und wenige JNI-Aufrufe.
const BLOCK: usize = 441;

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

fn slot() -> &'static Mutex<Einstellungen> {
    EINSTELLUNGEN.get_or_init(|| Mutex::new(Einstellungen::default()))
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
    tx: Sender<(i32, Welle)>,
}

static SPIELER: OnceLock<Mutex<Option<Spieler>>> = OnceLock::new();

/// Spielt eine MIDI-Note mit gewaehlter Wellenform an.
pub fn spielen_welle(midi: i32, welle: Welle) {
    let spieler = SPIELER.get_or_init(|| Mutex::new(None));
    let mut guard = match spieler.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.is_none() {
        match starten() {
            Ok(neu) => *guard = Some(neu),
            Err(e) => {
                info!("klang: Start fehlgeschlagen: {e}");
                return;
            }
        }
    }
    if let Some(spieler) = guard.as_ref() {
        let _ = spieler.tx.send((midi, welle));
    }
}

fn starten() -> Result<Spieler, String> {
    let app = crate::current_app()?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|e| format!("JavaVM: {e}"))?;
    let vm = std::sync::Arc::new(vm);
    let (tx, rx) = channel::<(i32, Welle)>();
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
fn schleife(vm: &std::sync::Arc<jni::JavaVM>, rx: Receiver<(i32, Welle)>) -> Result<(), String> {
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let (track, array) = create_track(&mut env)?;
    let mut stimmen: Vec<Stimme> = Vec::new();
    let mut puffer = vec![0i16; BLOCK];
    let mut welle = Welle::Sinus;
    loop {
        // Klingt nichts, wird gewartet statt gedreht; klingt etwas, werden
        // alle wartenden Auftraege sofort aufgenommen.
        let erster = if stimmen.is_empty() {
            match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(auftrag) => Some(auftrag),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            rx.try_recv().ok()
        };
        let mut auftrag = erster;
        while let Some((midi, neue_welle)) = auftrag {
            welle = neue_welle;
            let e = einstellungen();
            let laenge_ms = e.notenwert.ms(e.bpm).max(1);
            stimmen.push(Stimme::neu(frequenz(midi), RATE as u32, laenge_ms));
            // Ueberlast: die aelteste Stimme weicht.
            while stimmen.len() > e.stimmen.max(1) {
                stimmen.remove(0);
            }
            auftrag = rx.try_recv().ok();
        }
        if stimmen.is_empty() {
            continue;
        }
        // Kopfraum: wenige Stimmen bleiben laut, viele steuern sich nicht zu.
        let gain = 1.0 / (stimmen.len().max(1) as f32).sqrt();
        mischen(&mut puffer, &mut stimmen, RATE as u32, welle, gain);
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
