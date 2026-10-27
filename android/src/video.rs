//! Videodateien als Bildquelle (S8) über die Java-Brücke `VideoBridge`.
//!
//! MediaExtractor/MediaCodec sind Java-APIs; die Klasse dekodiert synchron
//! und ruft je Bild [`Java_org_jamcore_app_VideoBridge_nativeVideoFrame`].
//! Die Konvertierung nach RGBA, die Drehung und der Framepuffer sind dieselben
//! wie bei der Kamera (`camera::ablegen_gedreht`); die Drehung kommt hier aus
//! den Datei-Metadaten (`MediaFormat.KEY_ROTATION`).

use log::info;

use crate::camera::{ablegen_gedreht, klasse_holen};
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
            // Die Dauer kommt aus den Datei-Metadaten der Brücke. Eigene Zeile,
            // damit sie im Logcat leicht zu greifen ist (der Laub-Tab leitet
            // daraus seinen Abtastabstand ab).
            info!(
                "video: dauer={}",
                jamcore::videotempo::dauer_text(dauer_s(app))
            );
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

/// Dauer der **laufenden** Videoquelle in Sekunden; `None` = unbekannt.
///
/// Warum die Brücke sie merkt: der Laub-Tab verteilt seine ~40 Proben über die
/// ganze Länge (feste 400 ms reichten nur für die ersten Sekunden), und ein
/// zweiter Dateizugriff aus Rust wäre teuer und fehleranfällig. Der Wert steht
/// ab dem erfolgreichen [`start`] bereit und bleibt bis zum nächsten Start
/// gültig (auch nach [`stop`]).
pub fn dauer_s(app: &android_activity::AndroidApp) -> Option<f32> {
    dauer_ms(app, None).map(|ms| ms as f32 / 1000.0)
}

/// Dauer einer beliebigen Videodatei in Sekunden; `None` = unbekannt oder
/// nicht lesbar. Für Aufrufer, die die Länge **vor** dem Start brauchen
/// (z.B. um die Zahl der Stichproben zu planen).
pub fn dauer_s_von(app: &android_activity::AndroidApp, pfad: &str) -> Option<f32> {
    dauer_ms(app, Some(pfad)).map(|ms| ms as f32 / 1000.0)
}

/// Millisekunden von der Brücke holen: ohne Pfad die gemerkte Dauer der
/// laufenden Quelle, mit Pfad eine frische Messung. `< 0` (Java-Kennung für
/// „unbekannt“) wird zu `None`.
fn dauer_ms(app: &android_activity::AndroidApp, pfad: Option<&str>) -> Option<i64> {
    let ergebnis = jniutil::with_env(app, |env, activity| {
        use jni::objects::{JObject, JValue};

        let klasse = klasse_holen(env, activity, "org.jamcore.app.VideoBridge").ok()?;
        let wert = match pfad {
            None => env.call_static_method(klasse, "dauerMs", "()J", &[]),
            Some(pfad) => {
                let name = env.new_string(pfad).ok()?;
                env.call_static_method(
                    klasse,
                    "dauerMsVon",
                    "(Ljava/lang/String;)J",
                    &[JValue::Object(&JObject::from(name))],
                )
            }
        };
        let Ok(wert) = wert else {
            jniutil::describe_and_clear(env);
            return None;
        };
        let ms = wert.j().ok()?;
        (ms > 0).then_some(ms)
    });
    ergebnis.flatten()
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
/// `drehung` ist die Drehung aus den Datei-Metadaten (KEY_ROTATION, Grad im
/// Uhrzeigersinn); der Dekoder liefert die Bilder ungedreht, deshalb dreht
/// dieselbe Stelle wie bei der Kamera.
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
    drehung: jni::sys::jint,
) {
    let (Ok(y), Ok(u), Ok(v)) = (
        env.convert_byte_array(&y),
        env.convert_byte_array(&u),
        env.convert_byte_array(&v),
    ) else {
        info!("video: Bild-Arrays nicht lesbar");
        return;
    };
    ablegen_gedreht(
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
        drehung.max(0) as u32,
    );
}
