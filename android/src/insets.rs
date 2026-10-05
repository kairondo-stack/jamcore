//! Systemleisten-Insets für die Kopf-/Fußzeile (Android 15+ ist Edge-to-Edge).

use crate::jniutil;
use log::info;

pub type InsetsPx = (f32, f32);

#[cfg(target_os = "android")]
pub fn root_insets_px(app: &android_activity::AndroidApp) -> Option<InsetsPx> {
    use jni::objects::JValue;

    jniutil::with_env(app, |env, activity| {
        let window = jniutil::call_object(env, activity, "getWindow", "()Landroid/view/Window;")?;
        let decor = jniutil::call_object(env, &window, "getDecorView", "()Landroid/view/View;")?;
        let insets = jniutil::call_object(
            env,
            &decor,
            "getRootWindowInsets",
            "()Landroid/view/WindowInsets;",
        )?;

        let type_class = env.find_class("android/view/WindowInsets$Type").ok()?;
        let system_bars: i32 = env
            .call_static_method(&type_class, "systemBars", "()I", &[])
            .ok()?
            .i()
            .ok()?;
        let args: [JValue; 1] = [JValue::Int(system_bars)];
        let rect = env
            .call_method(&insets, "getInsets", "(I)Landroid/graphics/Rect;", &args)
            .ok()?
            .l()
            .ok()?;
        let top: i32 = env.get_field(&rect, "top", "I").ok()?.i().ok()?;
        let bottom: i32 = env.get_field(&rect, "bottom", "I").ok()?.i().ok()?;
        Some((top as f32, bottom as f32))
    })
    .flatten()
}

#[cfg(target_os = "android")]
pub fn status_bar_px(app: &android_activity::AndroidApp) -> Option<InsetsPx> {
    use jni::objects::{JString, JValue};

    jniutil::with_env(app, |env, activity| {
        let resources = jniutil::call_object(
            env,
            activity,
            "getResources",
            "()Landroid/content/res/Resources;",
        )?;
        let key = JString::from(env.new_string("status_bar_height").ok()?);
        let kind = JString::from(env.new_string("dimen").ok()?);
        let pkg = JString::from(env.new_string("android").ok()?);
        let args: [JValue; 3] = [
            JValue::Object(&key),
            JValue::Object(&kind),
            JValue::Object(&pkg),
        ];
        let id = env
            .call_method(
                &resources,
                "getIdentifier",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
                &args,
            )
            .ok()?
            .i()
            .ok()?;
        if id == 0 {
            info!("jamandroid: Ressource status_bar_height fehlt");
            return None;
        }
        let args: [JValue; 1] = [JValue::Int(id)];
        let px = env
            .call_method(&resources, "getDimensionPixelSize", "(I)I", &args)
            .ok()?
            .i()
            .ok()?;
        Some((px as f32, 0.0))
    })
    .flatten()
}

#[cfg(target_os = "android")]
pub fn external_files_dir(app: &android_activity::AndroidApp) -> Option<String> {
    use jni::objects::{JObject, JString, JValue};

    jniutil::with_env(app, |env, activity| {
        let none = JString::from(JObject::null());
        let file = jniutil::call_object_args(
            env,
            activity,
            "getExternalFilesDir",
            "(Ljava/lang/String;)Ljava/io/File;",
            &[JValue::Object(&none)],
        )?;
        let path = jniutil::call_object(env, &file, "getAbsolutePath", "()Ljava/lang/String;")?;
        jniutil::jstring(env, &path)
    })
    .flatten()
}
