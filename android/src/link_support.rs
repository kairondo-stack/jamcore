//! MulticastLock für Ableton Link (S9-Rest).
//!
//! Android verwirft eingehende Multicast-Pakete im WLAN, solange kein Lock
//! gehalten wird — ohne ihn sieht die Link-Session keine Peers. Der Java-
//! Helfer `LinkSupport` holt und gibt das Lock; hier nur die JNI-Aufrufe.

use log::info;

use crate::camera::klasse_holen;
use crate::jniutil;

fn aufruf(app: &android_activity::AndroidApp, methode: &str, sig: &str) -> Result<(), String> {
    let ergebnis = jniutil::with_env(app, |env, activity| {
        use jni::objects::{JObject, JValue};

        let klasse = klasse_holen(env, activity, "org.jamcore.app.LinkSupport")?;
        let args = if sig.starts_with('(') && sig.contains("Context") {
            vec![JValue::Object(activity)]
        } else {
            Vec::new()
        };
        let wert = env
            .call_static_method(klasse, methode, sig, &args)
            .map_err(|e| {
                let detail = jniutil::describe_and_clear(env);
                format!("LinkSupport.{methode}: {e} / {detail}")
            })?;
        if sig.ends_with("Ljava/lang/String;") {
            let text: JObject = wert
                .l()
                .map_err(|e| format!("LinkSupport.{methode}-Ergebnis: {e}"))?;
            let meldung = jniutil::jstring(env, &text).unwrap_or_default();
            if meldung.is_empty() {
                Ok(())
            } else {
                Err(meldung)
            }
        } else {
            Ok(())
        }
    });
    match ergebnis {
        Some(ok) => ok,
        None => Err("kein JNI-Kontext".to_string()),
    }
}

/// Holt das MulticastLock (für Link nötig).
pub fn aktivieren(app: &android_activity::AndroidApp) -> Result<(), String> {
    let ergebnis = aufruf(
        app,
        "acquire",
        "(Landroid/content/Context;)Ljava/lang/String;",
    );
    match &ergebnis {
        Ok(()) => info!("link: MulticastLock angefordert"),
        Err(e) => info!("link: MulticastLock-Fehler: {e}"),
    }
    ergebnis
}

/// Gibt das MulticastLock wieder her.
pub fn freigeben(app: &android_activity::AndroidApp) {
    let _ = aufruf(app, "release", "()V");
    info!("link: MulticastLock freigegeben");
}
