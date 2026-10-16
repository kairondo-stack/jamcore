//! Mikrofonaufnahme → 16-Bit-PCM-WAV im App-Ordner `recordings/`.
//!
//! Vorbild ist `mic.rs`: dort ist die auf dem Gerät erprobte JNI-Konstruktion
//! dokumentiert (`AudioRecord$Builder` statt der gesperrten Konstruktoren,
//! `AudioRecord.STATE_INITIALIZED == 1`). Statt des BPM-Schätzers sammelt
//! dieser Recorder **alle** Proben für den Slicer und schreibt sie beim Stopp
//! als WAV (`jamcore::wav::wav_mono`).
//!
//! Fehler kommen als Meldung zurück (kein Panic): startet das Mikrofon nicht,
//! steht der Grund im `Err`; bricht der Kanal während der Aufnahme weg, steht
//! er in [`Aufnahme::fehler`] und die Oberfläche räumt den Recorder ab.

use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use log::info;

const SAMPLE_RATE: c_int = 44100;
const CHANNEL_IN_MONO: c_int = 16;
const ENCODING_PCM_16BIT: c_int = 2;
/// `MediaRecorder.AudioSource.VOICE_RECOGNITION` (wenig Eigenbearbeitung).
const SOURCE_VOICE_RECOGNITION: c_int = 6;
/// `MediaRecorder.AudioSource.MIC` (erlaubt 44100 Hz, bringt aber AGC mit).
const SOURCE_MIC: c_int = 1;
/// `AudioRecord.STATE_INITIALIZED` ist 1 (nicht 3!) — siehe `mic.rs`.
const STATE_INITIALIZED: c_int = 1;
const FRAMES_PER_BLOCK: usize = 2048;
const BYTES_PER_FRAME: usize = 2;
const BLOCKS: usize = 4;
/// Obergrenze der Aufnahme in Sekunden (hält den Speicher im Griff).
const MAX_SEKUNDEN: usize = 30 * 60;

pub struct Aufnahme {
    vm: Arc<jni::JavaVM>,
    record: jni::objects::GlobalRef,
    _array: jni::objects::GlobalRef,
    running: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// Aufgenommene 16-Bit-Proben (mono). Der Thread hängt an, die Oberfläche
    /// liest nur Dauer und Pegel.
    proben: Arc<Mutex<Vec<i16>>>,
    /// Spitzenpegel seit dem letzten Abruf (als f32-Bits abgelegt).
    peak: Arc<AtomicI32>,
    /// Meldung des Aufnahmethreads, falls der Kanal wegbricht.
    fehler: Arc<Mutex<Option<String>>>,
    rate: c_int,
}

impl Aufnahme {
    /// Startet die Aufnahme. `Err` heißt: Mikrofon nicht erreichbar
    /// (typisch fehlende Berechtigung oder belegter Kanal).
    pub fn start() -> Result<Self, String> {
        let app = crate::current_app()?;
        let vm = Arc::new(
            unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
                .map_err(|e| format!("JavaVM: {e}"))?,
        );
        let (record, array, rate) = create_record(&vm)?;

        let running = Arc::new(AtomicBool::new(true));
        let proben = Arc::new(Mutex::new(Vec::new()));
        let peak = Arc::new(AtomicI32::new(0));
        let fehler = Arc::new(Mutex::new(None));
        let worker = {
            let vm = Arc::clone(&vm);
            let record = record.clone();
            let array = array.clone();
            let running = Arc::clone(&running);
            let proben = Arc::clone(&proben);
            let peak = Arc::clone(&peak);
            let fehler = Arc::clone(&fehler);
            std::thread::Builder::new()
                .name("jam-wavrec".to_string())
                .spawn(move || {
                    aufnahme_schleife(
                        &vm, &record, &array, &running, &proben, &peak, &fehler, rate,
                    );
                })
                .map_err(|e| format!("Aufnahme-Thread: {e}"))?
        };

        info!("wavrec: Aufnahme startet ({rate} Hz mono PCM16)");
        Ok(Self {
            vm,
            record,
            _array: array,
            running,
            worker: Some(worker),
            proben,
            peak,
            fehler,
            rate,
        })
    }

    /// Spitzenpegel seit dem letzten Abruf (0 … 1) — für die Pegelanzeige.
    pub fn pegel(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed) as u32)
    }

    /// Bisherige Aufnahmedauer in Sekunden.
    pub fn dauer_s(&self) -> f32 {
        let len = self.proben.lock().map(|p| p.len()).unwrap_or(0);
        len as f32 / self.rate.max(1) as f32
    }

    /// Meldung des Aufnahmethreads (Kanal weg o. Ä.), sonst `None`.
    pub fn fehler(&self) -> Option<String> {
        self.fehler.lock().ok().and_then(|f| f.clone())
    }

    /// Aufnahme beenden und als WAV schreiben. `Ok(pfad)` ist die fertige
    /// Datei in `<App-Ordner>/recordings/aufnahme-<nr>.wav`.
    pub fn stoppen(mut self) -> Result<String, String> {
        self.beenden();
        let fehler = self.fehler();
        let proben = self.proben.lock().map(|p| p.clone()).unwrap_or_default();
        if proben.is_empty() {
            return Err(fehler.unwrap_or_else(|| "keine Proben aufgenommen".to_string()));
        }
        if let Some(f) = fehler {
            // Daten sind da, der Kanal ist aber gestört — schreiben und den
            // Grund ins Log, die Oberfläche hat ihn schon gemeldet.
            info!("wavrec: Kanal gestört ({f}), schreibe die bisherigen Proben");
        }
        let ordner = recordings_dir()?;
        let pfad = naechster_pfad(&ordner);
        let bytes = jamcore::wav::wav_mono(&proben, self.rate.max(1) as u32);
        std::fs::write(&pfad, bytes).map_err(|e| format!("{}: {e}", pfad.display()))?;
        info!(
            "wavrec: {} Proben ({:.1} s) -> {}",
            proben.len(),
            proben.len() as f32 / self.rate.max(1) as f32,
            pfad.display()
        );
        Ok(pfad.display().to_string())
    }

    /// Thread anhalten und AudioRecord freigeben (idempotent).
    fn beenden(&mut self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        // Erst AudioRecord.stop(), damit ein blockierendes read() zurückkehrt,
        // dann den Thread einsammeln.
        let _ = call_void(&self.vm, &self.record, "stop", "()V");
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = call_void(&self.vm, &self.record, "release", "()V");
        info!("wavrec: Aufnahme beendet");
    }
}

impl Drop for Aufnahme {
    fn drop(&mut self) {
        self.beenden();
    }
}

/// `<App-Ordner>/recordings` (wird angelegt, falls nötig).
fn recordings_dir() -> Result<PathBuf, String> {
    let app = crate::current_app()?;
    let basis = crate::insets::external_files_dir(&app)
        .ok_or_else(|| "kein App-Verzeichnis".to_string())?;
    let ordner = PathBuf::from(basis).join("recordings");
    std::fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    Ok(ordner)
}

/// `aufnahme-1.wav`, `aufnahme-2.wav`, … — die erste freie Nummer.
fn naechster_pfad(ordner: &Path) -> PathBuf {
    for nr in 1..10_000u32 {
        let pfad = ordner.join(format!("aufnahme-{nr}.wav"));
        if !pfad.exists() {
            return pfad;
        }
    }
    ordner.join("aufnahme.wav")
}

fn create_record(
    vm: &Arc<jni::JavaVM>,
) -> Result<(jni::objects::GlobalRef, jni::objects::GlobalRef, c_int), String> {
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach: {e}"))?;
    let bytes = (FRAMES_PER_BLOCK * BYTES_PER_FRAME * BLOCKS) as c_int;
    // Gleiche Reihenfolge wie `mic.rs`: die auf dem Gerät erprobten
    // Kombinationen nacheinander probieren und die gewählte protokollieren.
    let varianten: [(&str, c_int, c_int); 3] = [
        ("VOICE_RECOGNITION/16000", SOURCE_VOICE_RECOGNITION, 16000),
        ("MIC/44100", SOURCE_MIC, SAMPLE_RATE),
        ("MIC/16000", SOURCE_MIC, 16000),
    ];
    let mut gewaehlt: Option<(jni::objects::GlobalRef, jni::objects::GlobalRef, c_int)> = None;
    let mut gruende: Vec<String> = Vec::new();
    for (name, quelle, rate) in varianten {
        match build_record(&mut env, bytes, quelle, rate) {
            Ok(record) => {
                info!("wavrec: Quelle {name} funktioniert");
                gewaehlt = Some(record);
                break;
            }
            Err(e) => {
                info!("wavrec: Quelle {name}: {e}");
                gruende.push(format!("{name}: {e}"));
            }
        }
    }
    let Some(gewaehlt) = gewaehlt else {
        return Err(format!(
            "keine Aufnahmequelle nutzbar: {}",
            gruende.join(" | ")
        ));
    };
    Ok(gewaehlt)
}

/// Ein Aufnahmeversuch: `AudioRecord.Builder` (die Konstruktoren sind auf
/// diesem Gerät gesperrt, siehe `mic.rs`), dann starten.
fn build_record(
    env: &mut jni::AttachGuard,
    bytes: c_int,
    quelle: c_int,
    rate: c_int,
) -> Result<(jni::objects::GlobalRef, jni::objects::GlobalRef, c_int), String> {
    use jni::objects::JValue;

    let builder_class = env
        .find_class("android/media/AudioRecord$Builder")
        .map_err(|e| format!("AudioRecord.Builder-Klasse: {e}"))?;
    let builder = env
        .new_object(&builder_class, "()V", &[])
        .map_err(|e| format!("AudioRecord.Builder(): {e}"))?;

    // `setAudioFormat` nimmt ein AudioFormat-Objekt (Encoding, Rate, Kanäle),
    // keine Int-Konstante. Fehler kommen mit Details heraus.
    let format_builder_class = env
        .find_class("android/media/AudioFormat$Builder")
        .map_err(|e| format!("AudioFormat.Builder-Klasse: {e}"))?;
    let format_builder = env
        .new_object(&format_builder_class, "()V", &[])
        .map_err(|e| format!("AudioFormat.Builder(): {e}"))?;
    let format_signatur = "(I)Landroid/media/AudioFormat$Builder;";
    for (methode, wert) in [
        ("setEncoding", ENCODING_PCM_16BIT),
        ("setSampleRate", rate),
        ("setChannelMask", CHANNEL_IN_MONO),
    ] {
        if let Err(e) = env.call_method(
            &format_builder,
            methode,
            format_signatur,
            &[JValue::Int(wert)],
        ) {
            let detail = crate::jniutil::describe_and_clear(env);
            return Err(format!(
                "AudioFormat.Builder.{methode}({wert}): {e} / {detail}"
            ));
        }
    }
    let format = match env.call_method(
        &format_builder,
        "build",
        "()Landroid/media/AudioFormat;",
        &[],
    ) {
        Ok(value) => match value.l() {
            Ok(format) => format,
            Err(e) => return Err(format!("AudioFormat.Builder.build(): {e}")),
        },
        Err(e) => {
            let detail = crate::jniutil::describe_and_clear(env);
            return Err(format!("AudioFormat.Builder.build(): {e} / {detail}"));
        }
    };
    let builder_signatur = "(I)Landroid/media/AudioRecord$Builder;";
    for (methode, wert) in [("setAudioSource", quelle), ("setBufferSizeInBytes", bytes)] {
        if let Err(e) = env.call_method(&builder, methode, builder_signatur, &[JValue::Int(wert)]) {
            let detail = crate::jniutil::describe_and_clear(env);
            return Err(format!("Builder.{methode}({wert}): {e} / {detail}"));
        }
    }
    if let Err(e) = env.call_method(
        &builder,
        "setAudioFormat",
        "(Landroid/media/AudioFormat;)Landroid/media/AudioRecord$Builder;",
        &[JValue::Object(&format)],
    ) {
        let detail = crate::jniutil::describe_and_clear(env);
        return Err(format!("Builder.setAudioFormat(): {e} / {detail}"));
    }
    let record = match env.call_method(&builder, "build", "()Landroid/media/AudioRecord;", &[]) {
        Ok(value) => match value.l() {
            Ok(record) => record,
            Err(e) => return Err(format!("Builder.build(): {e}")),
        },
        Err(e) => {
            let detail = crate::jniutil::describe_and_clear(env);
            return Err(format!("Builder.build(): {e} / {detail}"));
        }
    };

    let state = match env.call_method(&record, "getState", "()I", &[]) {
        Ok(v) => v.i().unwrap_or(0),
        Err(e) => {
            let detail = crate::jniutil::describe_and_clear(env);
            return Err(format!("getState(): {e} / {detail}"));
        }
    };
    // `AudioRecord.STATE_INITIALIZED` ist 1 (0 = STATE_UNINITIALIZED).
    if state != STATE_INITIALIZED {
        return Err(format!(
            "nicht initialisiert (state {state}) – Mikrofon belegt oder Berechtigung fehlt"
        ));
    }
    if let Err(e) = env.call_method(&record, "startRecording", "()V", &[]) {
        let detail = crate::jniutil::describe_and_clear(env);
        return Err(format!(
            "startRecording(): {e} / {detail} – RECORD_AUDIO fehlt?"
        ));
    }

    let array_len = (FRAMES_PER_BLOCK * BYTES_PER_FRAME) as c_int;
    let array = env
        .new_byte_array(array_len)
        .map_err(|e| format!("byte[]: {e}"))?;
    let record = env
        .new_global_ref(&record)
        .map_err(|e| format!("global record: {e}"))?;
    let array = env
        .new_global_ref(&array)
        .map_err(|e| format!("global array: {e}"))?;
    info!("wavrec: AudioRecord {rate} Hz mono PCM16, Puffer {bytes} Byte");
    Ok((record, array, rate))
}

/// Aufnahmeschleife: PCM16-LE-Bytes lesen, in i16 wandeln, sammeln und den
/// Pegel melden. Läuft bis `running` fällt oder der Kanal wegbricht.
#[allow(clippy::too_many_arguments)]
fn aufnahme_schleife(
    vm: &Arc<jni::JavaVM>,
    record: &jni::objects::GlobalRef,
    array: &jni::objects::GlobalRef,
    running: &Arc<AtomicBool>,
    proben: &Arc<Mutex<Vec<i16>>>,
    peak: &Arc<AtomicI32>,
    fehler: &Arc<Mutex<Option<String>>>,
    rate: c_int,
) {
    let Ok(mut env) = vm.attach_current_thread() else {
        melde_fehler(fehler, "Thread konnte nicht an die JVM angehängt werden");
        return;
    };
    let max_proben = (rate.max(1) as usize) * MAX_SEKUNDEN;

    while running.load(Ordering::Relaxed) {
        let read = env.call_method(
            record.as_obj(),
            "read",
            "([BII)I",
            &[
                jni::objects::JValue::Object(array.as_ref()),
                jni::objects::JValue::Int(0),
                jni::objects::JValue::Int((FRAMES_PER_BLOCK * BYTES_PER_FRAME) as i32),
            ],
        );
        let n = match read {
            Ok(wert) => wert.i().unwrap_or(0),
            Err(e) => {
                let detail = crate::jniutil::describe_and_clear(&mut env);
                melde_fehler(fehler, &format!("AudioRecord.read: {e} / {detail}"));
                break;
            }
        };
        if n <= 0 {
            melde_fehler(fehler, &format!("AudioRecord.read gab {n}"));
            break;
        }

        let jarray: &jni::objects::JPrimitiveArray<i8> = array.as_obj().into();
        let mut raw = vec![0i8; n as usize];
        if env
            .get_byte_array_region(jarray, 0, &mut raw[..n as usize])
            .is_err()
        {
            let detail = crate::jniutil::describe_and_clear(&mut env);
            melde_fehler(fehler, &format!("get_byte_array_region: {detail}"));
            break;
        }

        // PCM16 little-endian: zwei Bytes je Probe.
        let mut spitze = 0i32;
        let mut neue: Vec<i16> = Vec::with_capacity(raw.len() / 2);
        for paar in raw.chunks_exact(2) {
            let wert = i16::from_le_bytes([paar[0] as u8, paar[1] as u8]);
            spitze = spitze.max((wert as i32).abs());
            neue.push(wert);
        }
        if let Ok(mut liste) = proben.lock() {
            if liste.len() + neue.len() > max_proben {
                melde_fehler(fehler, "Aufnahme zu lang (30 Minuten)");
                break;
            }
            liste.extend_from_slice(&neue);
        }
        // Pegel für die Oberfläche (f32-Bits; positive Werte sind als i32
        // monoton, deshalb genügt fetch_max).
        peak.fetch_max((spitze as f32 / 32767.0).to_bits() as i32, Ordering::Relaxed);
    }
}

/// Fehlermeldung einmal setzen (der erste Grund bleibt stehen).
fn melde_fehler(fehler: &Arc<Mutex<Option<String>>>, text: &str) {
    info!("wavrec: {text}");
    if let Ok(mut slot) = fehler.lock() {
        if slot.is_none() {
            *slot = Some(text.to_string());
        }
    }
}

fn call_void(
    vm: &Arc<jni::JavaVM>,
    object: &jni::objects::GlobalRef,
    name: &str,
    sig: &str,
) -> Result<(), String> {
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let res = env.call_method(object.as_obj(), name, sig, &[]).map(|_| ());
    crate::jniutil::clear(&mut env);
    res.map_err(|e| e.to_string())
}
