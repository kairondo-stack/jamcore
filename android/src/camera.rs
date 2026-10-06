//! Kamera auf Android über die Java-Brücke `CameraBridge`.
//!
//! Warum Java dazwischen: Camera2 verlangt Pflicht-Callbacks
//! (`CameraDevice.StateCallback`, `CaptureSession.StateCallback`,
//! `ImageReader.OnImageAvailableListener`), die sich in reinem Rust/JNI nicht
//! erzeugen lassen. Die Klasse hält Kamera, Session und ImageReader und ruft
//! je Frame [`Java_org_jamcore_app_CameraBridge_nativeFrame`]; die Umrechnung
//! nach RGBA macht `jamcore::yuv`.

use std::sync::{Mutex, OnceLock};

use jamcore::yuv::{yuv420_to_rgba, Plane};
use log::info;

use crate::jniutil;

const BREITE: i32 = 640;
const HOEHE: i32 = 480;

/// Das zuletzt konvertierte Bild.
#[derive(Default)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub nummer: u64,
}

static FRAME: OnceLock<Mutex<Frame>> = OnceLock::new();
fn frame_slot() -> &'static Mutex<Frame> {
    FRAME.get_or_init(|| Mutex::new(Frame::default()))
}

/// Zähler und Maße des zuletzt angekommenen Frames (unter demselben Lock,
/// damit Nummer und Maße zusammenpassen).
pub fn stand() -> (u64, usize, usize) {
    frame_slot()
        .lock()
        .map(|f| (f.nummer, f.width, f.height))
        .unwrap_or((0, 0, 0))
}

/// Kopie des letzten Bildes für die Oberfläche.
/// Kommt in S7c (Vorschau); hier schon bereit, damit der Framepuffer
/// nicht wegoptimiert wird.
#[allow(dead_code)]
pub fn letztes_bild() -> Option<(usize, usize, Vec<u8>)> {
    frame_slot()
        .lock()
        .ok()
        .filter(|f| !f.rgba.is_empty())
        .map(|f| (f.width, f.height, f.rgba.clone()))
}

/// Die Brückenklasse über den ClassLoader der Activity holen.
///
/// `JNIEnv::find_class` benutzt bei Threads, die von native an die JVM
/// gehängt wurden, den Bootstrap-ClassLoader und findet App-Klassen nicht
/// („Java exception was thrown"). Der Weg über die Activity funktioniert.
pub(crate) fn klasse_holen<'a>(
    env: &mut jni::JNIEnv<'a>,
    activity: &jni::objects::JObject,
    klassenname: &str,
) -> Result<jni::objects::JClass<'a>, String> {
    use jni::objects::{JClass, JObject, JValue};

    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            let detail = jniutil::describe_and_clear(env);
            format!("getClassLoader: {e} / {detail}")
        })?;
    let name = env
        .new_string(klassenname)
        .map_err(|e| format!("new_string: {e}"))?;
    let klasse = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&JObject::from(name))],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            let detail = jniutil::describe_and_clear(env);
            format!("loadClass: {e} / {detail}")
        })?;
    Ok(JClass::from(klasse))
}

/// Startet die Vorschau über die Java-Brücke.
pub fn start(app: &android_activity::AndroidApp) -> Result<(), String> {
    let ergebnis = jniutil::with_env(app, |env, activity| {
        use jni::objects::{JObject, JValue};

        let klasse = klasse_holen(env, activity, "org.jamcore.app.CameraBridge")?;
        let wert = env
            .call_static_method(
                klasse,
                "start",
                "(Landroid/content/Context;II)Ljava/lang/String;",
                &[
                    JValue::Object(activity),
                    JValue::Int(BREITE),
                    JValue::Int(HOEHE),
                ],
            )
            .map_err(|e| {
                let detail = jniutil::describe_and_clear(env);
                format!("CameraBridge.start: {e} / {detail}")
            })?;
        let text: JObject = wert
            .l()
            .map_err(|e| format!("CameraBridge.start-Ergebnis: {e}"))?;
        let meldung = jniutil::jstring(env, &text).unwrap_or_default();
        if meldung.is_empty() {
            Ok(())
        } else {
            Err(meldung)
        }
    });
    match ergebnis {
        Some(Ok(())) => {
            info!("camera start angefragt ({BREITE}x{HOEHE})");
            Ok(())
        }
        Some(Err(e)) => Err(e),
        None => Err("kein JNI-Kontext".to_string()),
    }
}

/// Von Java gemeldeter Zustand. Auch asynchrone Fälle ("Kamera getrennt",
/// `onError`) landen dort und werden so für die Oberfläche sichtbar.
pub fn java_fehlermeldung(app: &android_activity::AndroidApp) -> String {
    let ergebnis = jniutil::with_env(app, |env, activity| {
        let Ok(klasse) = klasse_holen(env, activity, "org.jamcore.app.CameraBridge") else {
            return String::new();
        };
        let Ok(wert) = env.call_static_method(klasse, "status", "()Ljava/lang/String;", &[]) else {
            jniutil::describe_and_clear(env);
            return String::new();
        };
        wert.l()
            .ok()
            .and_then(|text| jniutil::jstring(env, &text))
            .unwrap_or_default()
    });
    ergebnis.unwrap_or_default()
}

/// Stoppt die Vorschau über die Java-Brücke.
pub fn stop(app: &android_activity::AndroidApp) {
    let _ = jniutil::with_env(app, |env, activity| {
        if let Ok(klasse) = klasse_holen(env, activity, "org.jamcore.app.CameraBridge") {
            let _ = env.call_static_method(klasse, "stop", "()V", &[]);
        }
    });
    info!("camera gestoppt");
}

/// Wird von der Java-Brücke je Frame gerufen (YUV_420_888, drei Planes).
///
/// # Safety
/// Aufruf erfolgt durch die JVM mit gültigen Array- und Größenargumenten.
#[no_mangle]
pub extern "system" fn Java_org_jamcore_app_CameraBridge_nativeFrame(
    env: jni::JNIEnv,
    _klasse: jni::objects::JClass,
    y: jni::objects::JByteArray,
    y_row: jni::sys::jint,
    y_pix: jni::sys::jint,
    u: jni::objects::JByteArray,
    u_row: jni::sys::jint,
    u_pix: jni::sys::jint,
    v: jni::objects::JByteArray,
    v_row: jni::sys::jint,
    v_pix: jni::sys::jint,
    width: jni::sys::jint,
    height: jni::sys::jint,
) {
    let breite = width.max(0) as usize;
    let hoehe = height.max(0) as usize;
    if breite == 0 || hoehe == 0 {
        return;
    }
    let (Ok(y), Ok(u), Ok(v)) = (
        env.convert_byte_array(&y),
        env.convert_byte_array(&u),
        env.convert_byte_array(&v),
    ) else {
        info!("frame: Arrays nicht lesbar");
        return;
    };
    ablegen(
        &y,
        y_row.max(0) as usize,
        y_pix.max(0) as usize,
        &u,
        u_row.max(0) as usize,
        u_pix.max(0) as usize,
        &v,
        v_row.max(0) as usize,
        v_pix.max(0) as usize,
        breite,
        hoehe,
    );
}

/// Rechnet drei YUV_420_888-Planes nach RGBA und legt das Bild in den
/// Framepuffer. Kamera und Videodekoder teilen sich diesen Weg.
pub(crate) fn ablegen(
    y: &[u8],
    y_row: usize,
    y_pix: usize,
    u: &[u8],
    u_row: usize,
    u_pix: usize,
    v: &[u8],
    v_row: usize,
    v_pix: usize,
    breite: usize,
    hoehe: usize,
) {
    if breite == 0 || hoehe == 0 {
        return;
    }
    let mut rgba = Vec::new();
    yuv420_to_rgba(
        Plane {
            data: y,
            row_stride: y_row,
            pixel_stride: y_pix,
        },
        Plane {
            data: u,
            row_stride: u_row,
            pixel_stride: u_pix,
        },
        Plane {
            data: v,
            row_stride: v_row,
            pixel_stride: v_pix,
        },
        breite,
        hoehe,
        &mut rgba,
    );
    if let Ok(mut frame) = frame_slot().lock() {
        frame.width = breite;
        frame.height = hoehe;
        frame.rgba = rgba;
        frame.nummer += 1;
    }
}
