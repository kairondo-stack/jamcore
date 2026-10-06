//! Kleiner Klang auf dem Handy: kurze Sinus-Töne über `AudioTrack`
//! (dasselbe Java-API-Muster wie der Modul-Player).
//!
//! Ein Thread mit Warteschlange spielt die Töne, damit Zeichnen und OSC
//! nicht blockieren. Die Noten selbst liefern `jamcore::klang`.

use std::os::raw::c_int;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use jamcore::klang::{frequenz, sinus_puffer};
use log::info;

const RATE: c_int = 44100;
const LAENGE_MS: u32 = 120;
const STREAM_MUSIC: i32 = 3;
const CHANNEL_OUT_STEREO: i32 = 12;
const ENCODING_PCM_16BIT: i32 = 2;
const MODE_STREAM: i32 = 1;

pub struct Spieler {
    tx: Sender<i32>,
}

static SPIELER: OnceLock<Mutex<Option<Spieler>>> = OnceLock::new();

/// Spielt eine MIDI-Note kurz an (nicht blockierend, Fehler nur im Log).
pub fn spielen(midi: i32) {
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
        let _ = spieler.tx.send(midi);
    }
}

fn starten() -> Result<Spieler, String> {
    let app = crate::current_app()?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|e| format!("JavaVM: {e}"))?;
    let vm = std::sync::Arc::new(vm);
    let (tx, rx) = channel::<i32>();
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

fn schleife(vm: &std::sync::Arc<jni::JavaVM>, rx: Receiver<i32>) -> Result<(), String> {
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let (track, array) = create_track(&mut env)?;
    while let Ok(midi) = rx.recv() {
        let puffer = sinus_puffer(frequenz(midi), RATE as u32, LAENGE_MS);
        // Stereo PCM16 little-endian, beide Kanäle gleich.
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
            crate::jniutil::clear(&mut env);
            continue;
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
        if wrote.is_err() {
            let detail = crate::jniutil::describe_and_clear(&mut env);
            info!("klang: write fehlgeschlagen: {detail}");
        }
    }
    Ok(())
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
