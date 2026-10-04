use log::info;

pub type InsetsPx = (f32, f32);

#[cfg(target_os = "android")]
pub fn root_insets_px(app: &android_activity::AndroidApp) -> Option<InsetsPx> {
    use jni::objects::JObject;

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };

    let window = call_object(&mut env, &activity, "getWindow", "()Landroid/view/Window;")?;
    let decor = call_object(&mut env, &window, "getDecorView", "()Landroid/view/View;")?;
    let insets = call_object(
        &mut env,
        &decor,
        "getRootWindowInsets",
        "()Landroid/view/WindowInsets;",
    )?;

    let type_class = match env.find_class("android/view/WindowInsets$Type") {
        Ok(c) => c,
        Err(e) => {
            info!("jamandroid: WindowInsets$Type nicht ladbar: {e}");
            clear(&mut env);
            return None;
        }
    };
    let system_bars: i32 = match env.call_static_method(&type_class, "systemBars", "()I", &[]) {
        Ok(v) => v.i().unwrap_or(0),
        Err(e) => {
            info!("jamandroid: systemBars() fehlgeschlagen: {e}");
            clear(&mut env);
            return None;
        }
    };

    let args: [jni::objects::JValue; 1] = [jni::objects::JValue::Int(system_bars)];
    let rect = match env.call_method(&insets, "getInsets", "(I)Landroid/graphics/Rect;", &args) {
        Ok(v) => match v.l() {
            Ok(r) if !r.is_null() => r,
            Ok(_) => return None,
            Err(e) => {
                info!("jamandroid: getInsets liefert kein Rect: {e}");
                return None;
            }
        },
        Err(e) => {
            clear(&mut env);
            let _ = e;
            return None;
        }
    };
    let top: i32 = env.get_field(&rect, "top", "I").ok()?.i().ok()?;
    let bottom: i32 = env.get_field(&rect, "bottom", "I").ok()?.i().ok()?;
    Some((top as f32, bottom as f32))
}

#[cfg(target_os = "android")]
pub fn status_bar_px(app: &android_activity::AndroidApp) -> Option<InsetsPx> {
    use jni::objects::JObject;
    use jni::objects::JValue;

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let resources = call_object(
        &mut env,
        &activity,
        "getResources",
        "()Landroid/content/res/Resources;",
    )?;

    let key = env.new_string("status_bar_height").ok()?;
    let kind = env.new_string("dimen").ok()?;
    let pkg = env.new_string("android").ok()?;
    let args: [JValue; 3] = [
        JValue::Object(&key),
        JValue::Object(&kind),
        JValue::Object(&pkg),
    ];
    let id = match env.call_method(
        &resources,
        "getIdentifier",
        "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
        &args,
    ) {
        Ok(v) => v.i().unwrap_or(0),
        Err(e) => {
            info!("jamandroid: getIdentifier fehlgeschlagen: {e}");
            clear(&mut env);
            return None;
        }
    };
    if id == 0 {
        info!("jamandroid: Ressource status_bar_height fehlt");
        return None;
    }
    let args: [JValue; 1] = [JValue::Int(id)];
    let px = match env.call_method(&resources, "getDimensionPixelSize", "(I)I", &args) {
        Ok(v) => v.i().unwrap_or(0),
        Err(e) => {
            info!("jamandroid: getDimensionPixelSize fehlgeschlagen: {e}");
            clear(&mut env);
            return None;
        }
    };
    info!("jamandroid: status_bar_height={px}px (Fallback)");
    Some((px as f32, 0.0))
}

#[cfg(target_os = "android")]
fn call_object<'a>(
    env: &mut jni::JNIEnv<'a>,
    obj: &jni::objects::JObject,
    name: &str,
    sig: &str,
) -> Option<jni::objects::JObject<'a>> {
    match env.call_method(obj, name, sig, &[]) {
        Ok(v) => v.l().ok().filter(|o| !o.is_null()),
        Err(e) => {
            info!("jamandroid: {name} fehlgeschlagen: {e}");
            clear(env);
            None
        }
    }
}

#[cfg(target_os = "android")]
fn clear(env: &mut jni::JNIEnv) {
    if let Ok(true) = env.exception_check() {
        let _ = env.exception_clear();
    }
}
