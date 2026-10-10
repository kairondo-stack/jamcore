//! Kleiner Klang auf dem Handy: kurze Sinus-Töne über `AudioTrack`
//! (dasselbe Java-API-Muster wie der Modul-Player).
//!
//! Ein Thread mit Warteschlange spielt die Töne, damit Zeichnen und OSC
//! nicht blockieren. Die Noten selbst liefern `jamcore::klang`.

use std::os::raw::c_int;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use jamcore::klang::{frequenz, puffer_welle, Welle};
use log::info;

const RATE: c_int = 44100;
const LAENGE_MS: u32 = 120;
const STREAM_MUSIC: i32 = 3;
const CHANNEL_OUT_STEREO: i32 = 12;
const ENCODING_PCM_16BIT: i32 = 2;
const MODE_STREAM: i32 = 1;
/// So viele Toene duerfen sich hoechstens stauen, bevor die aeltesten
/// verworfen werden. Ohne diese Grenze waechst die Warteschlange beim
/// Bild-Scan unbegrenzt — der Klang hoert nie auf.
const MAX_STAU: usize = 8;

pub struct Spieler {
    tx: Sender<(i32, Welle)>,
}

static SPIELER: OnceLock<Mutex<Option<Spieler>>> = OnceLock::new();

/// Spielt eine MIDI-Note mit gewaehlter Wellenform kurz an.
pub fn spielen_welle(midi: i32, welle: Welle) {
    let slot = SPIELER.get_or_init(|| Mutex::new(None));
    let mut guard = match slot.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.is_none() {
        match starten() {
            Ok(spieler) => *guard = Some(spieler),
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
    info!("klang: Spieler bereit ({RATE} Hz, {LAENGE_MS} ms Töne)");
    Ok(Spieler { tx })
}

fn schleife(vm: &std::sync::Arc<jni::JavaVM>, rx: Receiver<(i32, Welle)>) -> Result<(), String> {
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let (track, array) = create_track(&mut env)?;
    let rx = rx;
    loop {
        let Ok((midi, welle)) = rx.recv() else {
            break;
        };
        // Was sich waehrend des Spielens gesammelt hat, auf die neuesten
        // MAX_STAU kuerzen. Ohne das waechst die Warteschlange unbegrenzt:
        // Der Bild-Scan liefert mehr Noten, als in der Zeit spielbar sind (Ton
        // 120 ms, Spalte wenige ms) — der Klang hoert nie auf und man lauscht
        // auf einen alten Rueckstand statt auf das Bild.
        let mut naechste = vec![(midi, welle)];
        while naechste.len() < MAX_STAU {
            match rx.try_recv() {
                Ok(weitere) => naechste.push(weitere),
                Err(_) => break,
            }
        }
        let verworfen = rx.try_iter().count();
        if verworfen > 0 {
            info!("klang: {verworfen} veraltete Toene verworfen");
        }
        for (midi, welle) in naechste {
            if !ton(&mut env, &track, &array, midi, welle) {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Einen Ton erzeugen und in den AudioTrack schreiben (blockiert). `false`,
/// wenn der JNI-Aufruf gescheitert ist — dann aufgeben statt weiterzuprobieren.
fn ton(
    env: &mut jni::JNIEnv,
    track: &jni::objects::GlobalRef,
    array: &jni::objects::GlobalRef,
    midi: i32,
    welle: Welle,
) -> bool {
    let puffer = puffer_welle(frequenz(midi), RATE as u32, LAENGE_MS, welle);
    // Stereo PCM16 little-endian, beide Kanaele gleich.
    let mut bytes: Vec<i8> = Vec::with_capacity(puffer.len() * 4);
    for s in &puffer {
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
    // Das Array muss den ganzen Ton fassen (LAENGE_MS stereo PCM16).
    let array_len = RATE * LAENGE_MS as i32 / 1000 * 4;
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
