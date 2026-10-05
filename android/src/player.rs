//! Modul-Wiedergabe auf Android: AudioTrack (Java-API über JNI) mit libopenmpt.
//!
//! Warum nicht AAudio/OpenSL ES: Auf Android 17 (Pixel 10 Pro) ist `libaaudio.so`
//! nur ein Shim ohne C++-Symbole, `AAudio_createStreamBuilder` liefert nullptr,
//! und die `SL_IID_*`-Symbole von `libOpenSLES.so` sind für Apps unbrauchbar.
//! AudioTrack läuft über die normale Java-API, kostet aber etwas Latenz — für
//! einen Modul-Player genau richtig.

use std::os::raw::c_int;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use jamcore::openmpt::Mod;
use log::info;

const STREAM_MUSIC: i32 = 3;
const CHANNEL_OUT_STEREO: i32 = 12;
const ENCODING_PCM_16BIT: i32 = 2;
const MODE_STREAM: i32 = 1;
const FRAMES_PER_BLOCK: usize = 2048;
const BYTES_PER_FRAME: usize = 4;
const BLOCKS: usize = 4;

struct Shared {
    module: Mutex<Mod>,
    sample_rate: c_int,
    looping: bool,
    peak: AtomicU32,
    frames: AtomicU64,
    finished: AtomicBool,
    running: AtomicBool,
    /// Zeile und Zeilen-pro-Beat des aktuellen Patterns. Der Render-Thread
    /// schreibt sie, die UI liest sie — so muss die Oberfläche den Mutex des
    /// Audio-Threads nicht anreifen.
    row: AtomicI32,
    rows_per_beat: AtomicI32,
}

pub struct Player {
    vm: Arc<jni::JavaVM>,
    track: jni::objects::GlobalRef,
    shared: Arc<Shared>,
    worker: Option<std::thread::JoinHandle<()>>,
    path: String,
}

impl Player {
    pub fn play(path: &Path, looping: bool, sample_rate: c_int) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let module = Mod::from_bytes(bytes)?;
        info!(
            "jamandroid: Modul {:?} geladen ({} Kanäle, {} Orders, Titel {:?})",
            path.display(),
            module.num_channels(),
            module.num_orders(),
            module.title()
        );
        if looping {
            module.set_repeat_forever();
        }

        let app = crate::current_app()?;
        let vm = Arc::new(
            unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
                .map_err(|e| format!("JavaVM: {e}"))?,
        );
        let (track, audio_array) = create_track(&vm, sample_rate)?;

        let shared = Arc::new(Shared {
            module: Mutex::new(module),
            sample_rate,
            looping,
            peak: AtomicU32::new(0),
            frames: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            running: AtomicBool::new(true),
            row: AtomicI32::new(0),
            rows_per_beat: AtomicI32::new(4),
        });

        let worker_shared = Arc::clone(&shared);
        let worker_vm = Arc::clone(&vm);
        let worker_track = track.clone();
        let worker_array = audio_array.clone();
        let worker = std::thread::Builder::new()
            .name("jam-audio".to_string())
            .spawn(move || {
                render_loop(&worker_vm, &worker_track, &worker_array, &worker_shared);
            })
            .map_err(|e| format!("Audio-Thread: {e}"))?;

        let name = path.display().to_string();
        info!(
            "jamandroid: Wiedergabe startet ({name}) via AudioTrack, {FRAMES_PER_BLOCK} Frames x {BLOCKS}"
        );
        Ok(Player {
            vm,
            track,
            shared,
            worker: Some(worker),
            path: name,
        })
    }

    pub fn frames(&self) -> u64 {
        self.shared.frames.load(Ordering::Relaxed)
    }

    pub fn peak(&self) -> f32 {
        f32::from_bits(self.shared.peak.load(Ordering::Relaxed))
    }

    pub fn take_peak(&mut self) -> f32 {
        let p = self.peak();
        self.shared.peak.store(0.0f32.to_bits(), Ordering::Relaxed);
        p
    }

    pub fn position_seconds(&self) -> f64 {
        self.shared
            .module
            .lock()
            .map(|m| m.position_seconds())
            .unwrap_or(0.0)
    }

    pub fn order_row(&self) -> (i32, i32) {
        match self.shared.module.lock() {
            Ok(m) => (m.current_order(), m.current_row()),
            Err(_) => (0, 0),
        }
    }

    pub fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Relaxed)
    }

    /// Aktuelle Modulzeile als Schritt im 16er-Raster (`steps_per_beat` = 4
    /// heisst Sechzehntel). Die Phase beginnt bei der aktuellen Zeile, es wird
    /// also nicht auf den Taktanfang gefangen. Bei anderen Werten für
    /// `rows_per_beat` (z.B. Halbzeilen) sind nicht alle 16 Schritte erreichbar.
    ///
    /// Liest nur Atomics, damit die Oberfläche den Mutex des Audio-Threads
    /// nicht anreifen muss.
    pub fn grid_step(&self, steps_per_beat: i32) -> u16 {
        let rows_per_beat = self.shared.rows_per_beat.load(Ordering::Relaxed).max(1);
        let delta = self.shared.row.load(Ordering::Relaxed).rem_euclid(4096);
        (delta * steps_per_beat / rows_per_beat).rem_euclid(16) as u16
    }

    pub fn stop(&mut self) {
        if !self.shared.running.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = call_void(&self.vm, &self.track, "stop", "()V");
        let _ = call_void(&self.vm, &self.track, "release", "()V");
        info!("jamandroid: Wiedergabe beendet ({})", self.path);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

fn create_track(
    vm: &jni::JavaVM,
    sample_rate: c_int,
) -> Result<(jni::objects::GlobalRef, jni::objects::GlobalRef), String> {
    use jni::objects::JValue;

    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach: {e}"))?;
    let class = env
        .find_class("android/media/AudioTrack")
        .map_err(|e| format!("AudioTrack-Klasse: {e}"))?;

    let bytes = (FRAMES_PER_BLOCK * BLOCKS * BYTES_PER_FRAME) as i32;
    let track = env
        .new_object(
            &class,
            "(IIIIII)V",
            &[
                JValue::Int(STREAM_MUSIC),
                JValue::Int(sample_rate),
                JValue::Int(CHANNEL_OUT_STEREO),
                JValue::Int(ENCODING_PCM_16BIT),
                JValue::Int(bytes),
                JValue::Int(MODE_STREAM),
            ],
        )
        .map_err(|e| format!("AudioTrack(): {e}"))?;
    env.call_method(&track, "play", "()V", &[])
        .map_err(|e| format!("play(): {e}"))?;

    let array_len = (FRAMES_PER_BLOCK * BYTES_PER_FRAME) as i32;
    let array = env
        .new_byte_array(array_len)
        .map_err(|e| format!("float[]: {e}"))?;

    let track = env
        .new_global_ref(track)
        .map_err(|e| format!("global track: {e}"))?;
    let array = env
        .new_global_ref(&array)
        .map_err(|e| format!("global array: {e}"))?;

    info!("jamandroid: AudioTrack {sample_rate} Hz stereo PCM16, Puffer {bytes} Byte");
    Ok((track, array))
}

fn render_loop(
    vm: &jni::JavaVM,
    track: &jni::objects::GlobalRef,
    audio_array: &jni::objects::GlobalRef,
    shared: &Arc<Shared>,
) {
    let frames = FRAMES_PER_BLOCK;
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    let mut pcm: Vec<u8> = vec![0; frames * BYTES_PER_FRAME];

    while shared.running.load(Ordering::Relaxed) {
        let read = match shared.module.lock() {
            Ok(m) => {
                let pattern = m.current_pattern();
                shared.row.store(m.current_row(), Ordering::Relaxed);
                shared
                    .rows_per_beat
                    .store(m.rows_per_beat(pattern).max(1), Ordering::Relaxed);
                m.read_stereo(shared.sample_rate, &mut left, &mut right)
            }
            Err(_) => break,
        };
        let mut peak = 0.0f32;
        for i in 0..frames {
            let l = if i < read { left[i] } else { 0.0 };
            let r = if i < read { right[i] } else { 0.0 };
            peak = peak.max(l.abs()).max(r.abs());
        }
        shared.peak.store(peak.to_bits(), Ordering::Relaxed);
        shared.frames.fetch_add(read as u64, Ordering::Relaxed);

        for i in 0..frames {
            let l = if i < read { left[i] } else { 0.0 };
            let r = if i < read { right[i] } else { 0.0 };
            let l16 = to_i16(l);
            let r16 = to_i16(r);
            pcm[i * 4] = l16 as u8;
            pcm[i * 4 + 1] = (l16 >> 8) as u8;
            pcm[i * 4 + 2] = r16 as u8;
            pcm[i * 4 + 3] = (r16 >> 8) as u8;
        }

        let array: &jni::objects::JPrimitiveArray<i8> = audio_array.as_obj().into();
        let pcm_signed: Vec<i8> = pcm.iter().map(|b| *b as i8).collect();
        if let Err(e) = env.set_byte_array_region(array, 0, &pcm_signed) {
            info!("jamandroid: set_byte_array_region fehlgeschlagen: {e}");
            crate::jniutil::clear(&mut env);
            break;
        }
        let written = env.call_method(
            track.as_obj(),
            "write",
            "([BII)I",
            &[
                jni::objects::JValue::Object(array.as_ref()),
                jni::objects::JValue::Int(0),
                jni::objects::JValue::Int((frames * BYTES_PER_FRAME) as i32),
            ],
        );
        match written {
            Ok(v) => {
                let n = v.i().unwrap_or(0);
                if n <= 0 {
                    info!("jamandroid: AudioTrack.write gab {n}, Wiedergabe gestoppt");
                    shared.finished.store(true, Ordering::Relaxed);
                    break;
                }
            }
            Err(e) => {
                let detail = crate::jniutil::describe_and_clear(&mut env);
                info!("jamandroid: AudioTrack.write fehlgeschlagen: {e} / {detail}");
                shared.finished.store(true, Ordering::Relaxed);
                break;
            }
        }

        if read == 0 && !shared.looping {
            shared.finished.store(true, Ordering::Relaxed);
            break;
        }
    }
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0) as i16
}

fn call_void(
    vm: &jni::JavaVM,
    object: &jni::objects::GlobalRef,
    name: &str,
    sig: &str,
) -> Result<(), String> {
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let res = env.call_method(object.as_obj(), name, sig, &[]).map(|_| ());
    crate::jniutil::clear(&mut env);
    res.map_err(|e| e.to_string())
}
