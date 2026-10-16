//! Datei-Ordner der App und die „Alle Dateien"-Berechtigung
//! (Nutzerentscheidung D2, 2026-10-16).
//!
//! Zwei Wege, damit die App an Bild-, Audio-, Modul-, Video- und
//! Aufnahme-Dateien kommt:
//!
//! 1. **App-eigene Ordner** unter
//!    `/sdcard/Android/data/org.jamcore.app/files/{bild,audio,module,videos,recordings}`
//!    — ohne Berechtigung lesbar und per USB (`adb pull`) erreichbar.
//! 2. **„Alle Dateien"** (`MANAGE_EXTERNAL_STORAGE`): zusätzlich `DCIM`,
//!    `Pictures`, `Music`, `Download` und `Documents`.
//!
//! Die Activity ist eine `android.app.NativeActivity` (kein SAF); geprüft wird
//! mit `Environment.isExternalStorageManager()` (API 30+), der Rückfall ist
//! `Context.checkSelfPermission(...)`. Die Systemseite öffnet ein `Intent` per
//! `startActivity` — die JNI-Aufrufe laufen wie in `camera.rs` über
//! [`crate::jniutil`].

use std::path::Path;

#[cfg(target_os = "android")]
use crate::jniutil;

/// Die App-eigenen Ordner (D2) — werden beim Start angelegt.
pub const APP_ORDNER: [&str; 5] = ["bild", "audio", "module", "videos", "recordings"];

/// Paketname als Rückfall, falls `getPackageName()` nicht antwortet.
pub const PAKET: &str = "org.jamcore.app";

/// Legt die App-eigenen Ordner unter `basis` an. Fehler werden nur geloggt:
/// ein fehlender Ordner darf die App nie anhalten.
pub fn app_ordner_anlegen_in(basis: &Path) {
    for name in APP_ORDNER {
        let pfad = basis.join(name);
        if let Err(e) = std::fs::create_dir_all(&pfad) {
            log::info!("jamandroid: {} nicht anlegbar: {e}", pfad.display());
        }
    }
}

/// Legt die App-eigenen Ordner beim Start an (`android_main`).
#[cfg(target_os = "android")]
pub fn app_ordner_anlegen(app: &android_activity::AndroidApp) {
    match crate::insets::external_files_dir(app) {
        Some(basis) => {
            app_ordner_anlegen_in(Path::new(&basis));
            log::info!("App-Ordner bereit: {basis}");
        }
        None => log::info!("jamandroid: kein App-Ordner (external files dir fehlt)"),
    }
}

/// Ist die „Alle Dateien"-Berechtigung erteilt?
///
/// Zuerst `Environment.isExternalStorageManager()` (API 30+), sonst
/// `Context.checkSelfPermission("android.permission.MANAGE_EXTERNAL_STORAGE")`
/// (der Rückfall greift auf Android 8–10, wo es die Berechtigung noch nicht
/// gibt — dort ist das Ergebnis `false`).
#[cfg(target_os = "android")]
pub fn alle_dateien_erlaubt(app: &android_activity::AndroidApp) -> bool {
    use jni::objects::JValue;

    jniutil::with_env(app, |env, activity| {
        if let Ok(klasse) = env.find_class("android/os/Environment") {
            match env.call_static_method(&klasse, "isExternalStorageManager", "()Z", &[]) {
                Ok(wert) => return wert.z().unwrap_or(false),
                Err(_) => {
                    // API 26–29 kennt die Methode nicht: Ausnahme räumen und
                    // den Rückfall unten nehmen.
                    jniutil::clear(env);
                }
            }
        }
        let Ok(name) = env.new_string("android.permission.MANAGE_EXTERNAL_STORAGE") else {
            return false;
        };
        let args: [JValue; 1] = [JValue::Object(&name)];
        match env.call_method(
            activity,
            "checkSelfPermission",
            "(Ljava/lang/String;)I",
            &args,
        ) {
            Ok(wert) => wert.i().map(|i| i == 0).unwrap_or(false),
            Err(e) => {
                log::info!("jamandroid: checkSelfPermission: {e}");
                jniutil::clear(env);
                false
            }
        }
    })
    .unwrap_or(false)
}

/// Öffnet die Systemseite „Alle Dateien" für dieses Paket
/// (`Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` mit
/// `package:…`). Fehler kommen als Text zurück und werden in der Oberfläche
/// angezeigt — die App stürzt nie ab.
#[cfg(target_os = "android")]
pub fn einstellungen_oeffnen(app: &android_activity::AndroidApp) -> Result<(), String> {
    use jni::objects::JValue;

    let ergebnis = jniutil::with_env(app, |env, activity| -> Result<(), String> {
        let paket = jniutil::call_object(env, activity, "getPackageName", "()Ljava/lang/String;")
            .and_then(|text| jniutil::jstring(env, &text))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| PAKET.to_string());
        let uri_text = env
            .new_string(format!("package:{paket}"))
            .map_err(|e| format!("new_string: {e}"))?;
        let uri_klasse = env
            .find_class("android/net/Uri")
            .map_err(|e| format!("Uri-Klasse: {e}"))?;
        let uri = env
            .call_static_method(
                &uri_klasse,
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&uri_text)],
            )
            .and_then(|wert| wert.l())
            .map_err(|e| format!("Uri.parse: {e}"))?;
        let aktion = env
            .new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")
            .map_err(|e| format!("new_string: {e}"))?;
        let intent_klasse = env
            .find_class("android/content/Intent")
            .map_err(|e| format!("Intent-Klasse: {e}"))?;
        let intent = env
            .new_object(
                &intent_klasse,
                "(Ljava/lang/String;Landroid/net/Uri;)V",
                &[JValue::Object(&aktion), JValue::Object(&uri)],
            )
            .map_err(|e| format!("Intent: {e}"))?;
        match env.call_method(
            activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "startActivity: {e} {}",
                jniutil::describe_and_clear(env)
            )),
        }
    });
    match ergebnis {
        Some(ergebnis) => ergebnis,
        None => Err("kein JNI-Kontext".to_string()),
    }
}
