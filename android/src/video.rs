//! Videodateien als Bildquelle (S8) über die Java-Brücke `VideoBridge`.
//!
//! MediaExtractor/MediaCodec sind Java-APIs; die Klasse dekodiert synchron
//! und ruft je Bild [`Java_org_jamcore_app_VideoBridge_nativeVideoFrame`].
//! Die Konvertierung nach RGBA und der Framepuffer sind dieselben wie bei der
//! Kamera (`camera::ablegen`).

use log::info;

use crate::camera::{ablegen, klasse_holen};
use crate::jniutil;

/// Startet die Wiedergabe; Rückgabe `Ok(())` oder der Fehlertext der Brücke.
pub fn start(app: &android_activity::AndroidApp, pfad: &str, fps: u32) -> Result<(), String> {
    let ergebnis = jniutil::with_env(app, |env, activity| {
        use jni::objects::{JObject, JValue};

        let klasse = klasse_holen(env, activity, "org.jamcore.app.VideoBridge")?;
        let name = env
            .new_string(pfad)
            .map_err(|e| format!("new_string: {e}"))?;
        let wert = env
            .call_static_method(
                klasse,
                "start",
                "(Ljava/lang/String;I)Ljava/lang/String;",
                &[
                    JValue::Object(&JObject::from(name)),
                    JValue::Int(fps as i32),
                ],
            )
            .map_err(|e| {
                let detail = jniutil::describe_and_clear(env);
                format!("VideoBridge.start: {e} / {detail}")
            })?;
        let text: JObject = wert
            .l()
            .map_err(|e| format!("VideoBridge.start-Ergebnis: {e}"))?;
        let meldung = jniutil::jstring(env, &text).unwrap_or_default();
        if meldung.is_empty() {
            Ok(())
        } else {
            Err(meldung)
        }
    });
    match ergebnis {
        Some(Ok(())) => {
            info!("video start angefragt ({pfad}, {fps} fps)");
            Ok(())
        }
        Some(Err(e)) => Err(e),
        None => Err("kein JNI-Kontext".to_string()),
    }
}

/// Stoppt die Wiedergabe.
pub fn stop(app: &android_activity::AndroidApp) {
    let _ = jniutil::with_env(app, |env, activity| {
        if let Ok(klasse) = klasse_holen(env, activity, "org.jamcore.app.VideoBridge") {
            let _ = env.call_static_method(klasse, "stop", "()V", &[]);
        }
    });
    info!("video gestoppt");
}

/// Setzt die Bildrate der laufenden Wiedergabe.
pub fn fps_setzen(app: &android_activity::AndroidApp, fps: u32) {
    let _ = jniutil::with_env(app, |env, activity| {
        if let Ok(klasse) = klasse_holen(env, activity, "org.jamcore.app.VideoBridge") {
            let _ = env.call_static_method(
                klasse,
                "setFps",
                "(I)V",
                &[jni::objects::JValue::Int(fps as i32)],
            );
        }
    });
}

/// Von Java gemeldeter Zustand (z.B. Dekodierfehler).
pub fn java_meldung(app: &android_activity::AndroidApp) -> String {
    let ergebnis = jniutil::with_env(app, |env, activity| {
        let Ok(klasse) = klasse_holen(env, activity, "org.jamcore.app.VideoBridge") else {
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

/// Wird von der Java-Brücke je dekodiertem Bild gerufen.
///
/// # Safety
/// Aufruf erfolgt durch die JVM mit gültigen Array- und Größenargumenten.
#[no_mangle]
pub extern "system" fn Java_org_jamcore_app_VideoBridge_nativeVideoFrame(
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
    let (Ok(y), Ok(u), Ok(v)) = (
        env.convert_byte_array(&y),
        env.convert_byte_array(&u),
        env.convert_byte_array(&v),
    ) else {
        info!("video: Bild-Arrays nicht lesbar");
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
        width.max(0) as usize,
        height.max(0) as usize,
    );
}
