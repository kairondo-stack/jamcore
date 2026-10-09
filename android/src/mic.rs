//! Mikrofon-BPM auf Android: `AudioRecord` (Java-API über JNI) speist
//! `jamcore::micbpm`.
//!
//! Warum `AudioRecord` und nicht oboe/AAudio: Auf Android 17 ist die AAudio-C-API
//! ein Stub (`AAudio_createStreamBuilder` liefert nullptr), genauso wie beim
//! Abspielen. `AudioRecord` funktioniert und liefert dasselbe Format wie
//! `AudioTrack`: `read(byte[], …)` mit PCM 16 Bit little-endian mono.

use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use jamcore::micbpm::{pcm16_le_to_f32, BpmEstimator};
use jamcore::osc::{OscOut, Renoise};
use log::info;

const SAMPLE_RATE: c_int = 44100;
const CHANNEL_IN_MONO: c_int = 16;
const ENCODING_PCM_16BIT: c_int = 2;
/// `MediaRecorder.AudioSource.VOICE_RECOGNITION`: wenig Eigenbearbeitung, gut
/// für Klatschen und Musik.
const SOURCE_VOICE_RECOGNITION: c_int = 6;
/// `MediaRecorder.AudioSource.MIC`: erlaubt hoehere Abtastraten als
/// VOICE_RECOGNITION, bringt aber Eigenbearbeitung (AGC) mit.
const SOURCE_MIC: c_int = 1;
/// `AudioRecord.STATE_INITIALIZED`.
const STATE_INITIALIZED: c_int = 1;
const FRAMES_PER_BLOCK: usize = 2048;
const BYTES_PER_FRAME: usize = 2;
const BLOCKS: usize = 4;

pub struct Recorder {
    vm: Arc<jni::JavaVM>,
    record: jni::objects::GlobalRef,
    _array: jni::objects::GlobalRef,
    state: Arc<Atomics>,
    worker: Option<std::thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    rate: c_int,
}

/// Was der Aufnahmethread an die Oberfläche meldet. Alles atomar, damit die UI
/// keinen Mutex des Aufnahmethreads anfasst.
#[derive(Clone, Copy, Debug, Default)]
pub struct Level {
    pub peak: f32,
    pub bpm: f32,
    pub confidence: f32,
    pub windows: u32,
    pub rate: u32,
    /// Der Aufnahmethread hat sich beendet oder hatte einen Fehler.
    pub error: bool,
    pub finished: bool,
}

struct Atomics {
    peak: AtomicI32,
    bpm: AtomicI32,
    confidence: AtomicI32,
    windows: AtomicI32,
    error: AtomicI32,
    finished: AtomicBool,
}

impl Atomics {
    fn read(&self) -> Level {
        Level {
            peak: f32::from_bits(self.peak.load(Ordering::Relaxed) as u32),
            bpm: f32::from_bits(self.bpm.load(Ordering::Relaxed) as u32),
            confidence: f32::from_bits(self.confidence.load(Ordering::Relaxed) as u32),
            windows: self.windows.load(Ordering::Relaxed) as u32,
            rate: 0,
            error: self.error.load(Ordering::Acquire) != 0,
            finished: self.finished.load(Ordering::Acquire),
        }
    }

    fn store(&self, level: &Level) {
        self.peak
            .store(level.peak.to_bits() as i32, Ordering::Relaxed);
        self.bpm
            .store(level.bpm.to_bits() as i32, Ordering::Relaxed);
        self.confidence
            .store(level.confidence.to_bits() as i32, Ordering::Relaxed);
        self.windows.store(level.windows as i32, Ordering::Relaxed);
        self.error.store(level.error as i32, Ordering::Release);
        // `finished` traegt die Aussage "alle Felder sind aktuell", deshalb
        // Release/Acquire statt Relaxed.
        self.finished.store(level.finished, Ordering::Release);
    }
}

impl Recorder {
    /// Startet die Aufnahme. `Ok(None)` heißt: Mikrofon ist nicht erreichbar
    /// (typisch fehlende Berechtigung), der Grund steht dann in der Meldung.
    pub fn start() -> Result<Self, String> {
        let app = crate::current_app()?;
        let vm = Arc::new(
            unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
                .map_err(|e| format!("JavaVM: {e}"))?,
        );
        let (record, array, rate) = create_record(&vm)?;

        let running = Arc::new(AtomicBool::new(true));
        let state = Arc::new(Atomics {
            peak: AtomicI32::new(0),
            bpm: AtomicI32::new(0),
            confidence: AtomicI32::new(0),
            windows: AtomicI32::new(0),
            error: AtomicI32::new(0),
            finished: AtomicBool::new(false),
        });

        let worker_state = Arc::clone(&state);
        let worker_running = Arc::clone(&running);
        let worker_vm = Arc::clone(&vm);
        let worker_record = record.clone();
        let worker_array = array.clone();
        let worker = std::thread::Builder::new()
            .name("jam-mic".to_string())
            .spawn(move || {
                capture_loop(
                    &worker_vm,
                    &worker_record,
                    &worker_array,
                    &worker_state,
                    &worker_running,
                    rate,
                );
            })
            .map_err(|e| format!("Mikrofon-Thread: {e}"))?;

        info!(
            "jamandroid::mic: Aufnahme startet ({rate} Hz mono PCM16, {FRAMES_PER_BLOCK} Frames x {BLOCKS})"
        );
        Ok(Recorder {
            vm,
            record,
            _array: array,
            state,
            worker: Some(worker),
            running,
            rate,
        })
    }

    /// Nur Atomics lesen, damit die Oberflaeche keinen Mutex des Aufnahmethreads braucht.
    pub fn level(&self) -> Level {
        let mut level = self.state.read();
        level.rate = self.rate as u32;
        level
    }

    pub fn stop(&mut self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        // Erst AudioRecord.stop(), damit ein blockierendes read() zurueckkehrt,
        // dann den Thread einsammeln.
        let _ = call_void(&self.vm, &self.record, "stop", "()V");
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = call_void(&self.vm, &self.record, "release", "()V");
        // Die globalen Referenzen gibt jni 0.21 nicht zum Freigeben her; sie
        // werden beim Prozessende freigegeben (gleiches Muster wie der Player).
        info!("jamandroid::mic: Aufnahme beendet");
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

fn create_record(
    vm: &Arc<jni::JavaVM>,
) -> Result<(jni::objects::GlobalRef, jni::objects::GlobalRef, c_int), String> {
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("attach: {e}"))?;
    let bytes = (FRAMES_PER_BLOCK * BYTES_PER_FRAME * BLOCKS) as c_int;
    // Quelle und Abtastrate sind auf dem Geraet nicht frei waehlbar:
    // VOICE_RECOGNITION nimmt nur 16000 Hz, MIC auch 44100. Deshalb die
    // sinnvollen Kombinationen nacheinander probieren und protokollieren,
    // welche funktioniert.
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
                info!("jamandroid::mic: Quelle {name} funktioniert");
                gewaehlt = Some(record);
                break;
            }
            Err(e) => {
                info!("jamandroid::mic: Quelle {name}: {e}");
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
/// diesem Geraet gesperrt, siehe `create_record`), dann starten.
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
    let setzungen = [("setAudioSource", quelle), ("setBufferSizeInBytes", bytes)];
    for (methode, wert) in setzungen {
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
    // `AudioRecord.STATE_INITIALIZED` ist 1 (nicht 3, das ist die Zahl der
    // Frames oder aehnliches): 0 ist STATE_UNINITIALIZED. Ein falscher Wert
    // hier hat schon einmal alle funktionierenden Quellen verworfen.
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
    info!("jamandroid::mic: AudioRecord {rate} Hz mono PCM16, Puffer {bytes} Byte");
    Ok((record, array, rate))
}

fn capture_loop(
    vm: &Arc<jni::JavaVM>,
    record: &jni::objects::GlobalRef,
    array: &jni::objects::GlobalRef,
    state: &Arc<Atomics>,
    running: &Arc<AtomicBool>,
    rate: c_int,
) {
    let Ok(mut env) = vm.attach_current_thread() else {
        let mut level = Level::default();
        level.error = true;
        level.finished = true;
        state.store(&level);
        return;
    };
    let mut estimator = BpmEstimator::new(rate as u32);
    let mut level = Level::default();

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
        let Ok(written) = read else {
            let detail = crate::jniutil::describe_and_clear(&mut env);
            info!("jamandroid::mic: AudioRecord.read fehlgeschlagen: {detail}");
            level.error = true;
            break;
        };
        let n = written.i().unwrap_or(0);
        if n <= 0 {
            info!("jamandroid::mic: AudioRecord.read gab {n}, Aufnahme beendet");
            level.error = n < 0;
            break;
        }

        let array: &jni::objects::JPrimitiveArray<i8> = array.as_obj().into();
        let mut raw = vec![0i8; n as usize];
        if env
            .get_byte_array_region(array, 0, &mut raw[..n as usize])
            .is_err()
        {
            let detail = crate::jniutil::describe_and_clear(&mut env);
            info!("jamandroid::mic: get_byte_array_region fehlgeschlagen: {detail}");
            level.error = true;
            break;
        }

        let samples = pcm16_le_to_f32(&raw);
        let mut peak = level.peak;
        for s in &samples {
            peak = peak.max(s.abs());
        }
        level.peak = peak;
        estimator.push(&samples);
        if let Some((bpm, conf)) = estimator.estimate() {
            level.bpm = bpm;
            level.confidence = conf;
            level.windows += 1;
        }
        state.store(&level);
    }
    // Blieb `running` true, wurde die Schleife ungeplant verlassen (Kanal weg,
    // read-Fehler). Nur dann muss die Oberflaeche es erfahren; nach einem
    // regulaeren stop() ist der Recorder ohnehin schon abgeraeumt.
    if running.load(Ordering::Relaxed) {
        level.finished = true;
        state.store(&level);
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
/// UI-Zustand des Mikrofon-Screens.
pub struct MicPanel {
    recorder: Option<Recorder>,
    /// Auf Klatschen oder Musik warten; nach dieser Zeit steht der Wert fest.
    messen_bis: Option<Instant>,
    /// Stand der Fensterzaehler beim Start der Messung; nur ein hoeherer
    /// Wert zaehlt als neue Messung.
    start_windows: u32,
    /// Tap-Tempo: Zeitpunkte der letzten Tipps.
    taps: Vec<Instant>,
    gemessen: Option<f32>,
    status: String,
    /// Ziel von aussen gesetzt (eine Eingabe fuer OSC und Tempo).
    host: String,
    port: String,
    /// Beim „Übernehmen" gemeldet, damit JamApp es einmal uebernimmt.
    uebernommen: Option<f32>,
}

impl Default for MicPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl MicPanel {
    pub fn new() -> Self {
        Self {
            recorder: None,
            messen_bis: None,
            start_windows: 0,
            taps: Vec::new(),
            gemessen: None,
            status: "Bereit. Mikrofon an, dann klatschen oder Musik spielen.".to_string(),
            host: jamcore::osc::DEFAULT_HOST.to_string(),
            port: jamcore::osc::DEFAULT_BASE_PORT.to_string(),
            uebernommen: None,
        }
    }

    /// Ziel von aussen setzen — eine Eingabe für OSC und Tempo.
    pub fn set_target(&mut self, host: &str, port: &str) {
        self.host = host.trim().to_string();
        self.port = port.trim().to_string();
    }

    fn target(&self) -> (String, u16) {
        (
            self.host.clone(),
            self.port.parse().unwrap_or(jamcore::osc::DEFAULT_BASE_PORT),
        )
    }

    pub fn bpm(&self) -> Option<f32> {
        self.gemessen.or_else(|| {
            self.recorder
                .as_ref()
                .map(|r| r.level().bpm)
                .filter(|b| *b > 0.0)
        })
    }

    fn start(&mut self) {
        if self.recorder.is_some() {
            return;
        }
        match Recorder::start() {
            Ok(rec) => {
                self.recorder = Some(rec);
                self.status = "Höre zu …".to_string();
                info!("jamandroid::mic: Mikrofon aktiv");
            }
            Err(e) => {
                self.status = format!("Mikrofon nicht verfügbar: {e}");
                info!("jamandroid::mic: Start fehlgeschlagen: {e}");
            }
        }
    }

    fn stop(&mut self) {
        self.messen_bis = None;
        if let Some(rec) = self.recorder.as_mut() {
            rec.stop();
        }
        self.recorder = None;
    }

    /// Nach vier Sekunden wird der Wert festgehalten.
    fn messen(&mut self) {
        self.gemessen = None;
        if self.recorder.is_none() {
            self.start();
        }
        let Some(recorder) = self.recorder.as_ref() else {
            // Startfehler steht schon im Status und darf nicht ueberschrieben
            // werden.
            return;
        };
        self.start_windows = recorder.level().windows;
        self.messen_bis = Some(Instant::now() + Duration::from_secs(4));
        self.status = "Messe 4 Sekunden lang …".to_string();
    }

    /// Tap-Tempo wie im Desktop-Recorder: mittleres Intervall der Tipps.
    fn tap(&mut self) {
        let jetzt = Instant::now();
        // Mehr als drei Sekunden Pause beginnt eine neue Messung.
        if self
            .taps
            .last()
            .is_some_and(|t| jetzt.duration_since(*t).as_secs_f32() > 3.0)
        {
            self.taps.clear();
        }
        self.taps.push(jetzt);
        if self.taps.len() > 8 {
            self.taps.remove(0);
        }
        let basis = self.taps[0];
        let sekunden: Vec<f64> = self
            .taps
            .iter()
            .map(|t| t.duration_since(basis).as_secs_f64())
            .collect();
        match jamcore::transformation::bpm_aus_taps(&sekunden) {
            Some(bpm) => {
                self.gemessen = Some(bpm);
                self.status = format!(
                    "Tap: {bpm:.1} BPM ({} Tipps) - mit Übernehmen senden",
                    self.taps.len()
                );
                info!("mic: Tap-Tempo {bpm:.1} BPM ({} Tipps)", self.taps.len());
            }
            None => {
                self.status = format!("Tap: weiter tippen ({} Tipps)", self.taps.len());
            }
        }
    }

    /// Das gemessene Tempo uebernehmen (an Renoise schicken und merken).
    pub fn uebernehmen(&mut self) {
        let Some(bpm) = self.bpm() else {
            self.status = "Kein Tempo erkannt.".to_string();
            return;
        };
        let (host, port) = self.target();
        let result = OscOut::new(&host, port).and_then(|out| Renoise::new(&out).bpm(7000, bpm));
        match result {
            Ok(used) => {
                self.gemessen = Some(bpm);
                self.uebernommen = Some(bpm);
                self.status = format!("{bpm:.1} BPM -> {host}:{used}");
                info!("jamandroid::mic: BPM {bpm:.1} -> Port {used}");
            }
            Err(e) => self.status = format!("OSC-Fehler: {e}"),
        }
    }

    /// Das zuletzt uebernommene Tempo; wird nach dem Lesen verworfen, damit
    /// `JamApp` es genau einmal uebernimmt.
    pub fn uebernommenes_tempo(&mut self) -> Option<f32> {
        self.uebernommen.take()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint_after(Duration::from_millis(200));
        // Wenn der Aufnahmethread sich beendet hat (Kanal belegt oder
        // entzogen), den Recorder abraeumen, damit "Messen" neu startet
        // statt alte Werte weiterzuzeigen.
        if let Some(recorder) = self.recorder.as_ref() {
            let ende = recorder.level();
            if ende.finished {
                self.stop();
                self.messen_bis = None;
                self.status = if ende.error {
                    "Aufnahme gestört (kein Signal vom Mikrofon – Kanal belegt oder entzogen)."
                        .to_string()
                } else {
                    "Aufnahme beendet.".to_string()
                };
            }
        }
        let level = self.recorder.as_ref().map(|r| r.level());

        ui.horizontal(|ui| {
            ui.heading("Tempo");
            match self.gemessen {
                Some(bpm) => {
                    ui.label(RichText::new(format!("{bpm:.1} BPM")).size(28.0).strong());
                }
                None => {
                    let live = level.map(|l| l.bpm).unwrap_or(0.0);
                    if live > 0.0 {
                        ui.label(RichText::new(format!("{live:.1} BPM (live)")).size(24.0));
                    } else {
                        ui.label(RichText::new("— BPM").size(24.0).color(Color32::GRAY));
                    }
                }
            }
        });

        if let Some(l) = level {
            ui.label(format!(
                "Pegel {:.3} · Konfidenz {:.0} % · {} kHz · Fenster {} · {}",
                l.peak,
                l.confidence * 100.0,
                l.rate / 1000,
                l.windows,
                if l.peak > 0.001 {
                    "Signal da"
                } else {
                    "Stille"
                }
            ));
        }

        ui.horizontal(|ui| {
            if ui.button("Messen (4 s)").clicked() {
                self.messen();
            }
            if ui.button("Live").clicked() {
                self.gemessen = None;
                self.messen_bis = None;
                self.start();
                self.status = "Live-Messung.".to_string();
            }
            if ui.button("Stop").clicked() {
                self.stop();
                self.status = "Beendet.".to_string();
            }
            if ui.button("Tap").clicked() {
                self.tap();
            }
            if ui.button("Übernehmen").clicked() {
                self.uebernehmen();
            }
        });

        // Restzeit der Messung
        if let Some(deadline) = self.messen_bis {
            let left = deadline.saturating_duration_since(Instant::now());
            ui.label(format!("Messung endet in {:.1} s", left.as_secs_f32()));
            let l = level.unwrap_or_default();
            if left.is_zero() {
                self.messen_bis = None;
                let frisch = l.windows > self.start_windows;
                match (l.bpm, l.confidence, frisch) {
                    (bpm, conf, true) if bpm > 0.0 && conf > 0.3 => {
                        self.gemessen = Some(bpm);
                        self.status =
                            format!("Gemessen: {bpm:.1} BPM (Konfidenz {:.0} %)", conf * 100.0);
                        info!(
                            "jamandroid::mic: gemessen {bpm:.1} BPM (Konfidenz {:.2})",
                            conf
                        );
                    }
                    _ => {
                        self.status =
                            "Kein klares Tempo – lauter klatschen oder Musik spielen.".to_string();
                    }
                }
            }
        }

        // Das Ziel steht einmal im OSC-Tab (eine IP-Eingabe für OSC und Tempo),
        // hier nur als Text — sonst müsste man es zweimal eintippen.
        ui.label(
            RichText::new(format!("Ziel: {}:{}", self.host, self.port))
                .small()
                .color(Color32::GRAY),
        );
        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        ui.label(
            RichText::new(
                "Aufnahme über VOICE_RECOGNITION. Erkannt wird das Tempo aus den\nEinsätzen im Pegel, nicht die Tonhöhe. Für gute Werte muss der Pegel\ndeutlich über null liegen.",
            )
                .small()
                .color(Color32::DARK_GRAY),
        );
    }
}
