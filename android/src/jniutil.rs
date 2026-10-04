//! Kleine JNI-Hilfen: Thread an den JVM hängen, Objekte aufrufen, Ausnahmen räumen.

#[cfg(target_os = "android")]
pub fn with_env<R>(
    app: &android_activity::AndroidApp,
    f: impl FnOnce(&mut jni::JNIEnv, &jni::objects::JObject) -> R,
) -> Option<R> {
    use jni::objects::JObject;

    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let out = f(&mut env, &activity);
    clear(&mut env);
    Some(out)
}

#[cfg(target_os = "android")]
pub fn call_object<'a>(
    env: &mut jni::JNIEnv<'a>,
    obj: &jni::objects::JObject,
    name: &str,
    sig: &str,
) -> Option<jni::objects::JObject<'a>> {
    call_object_args(env, obj, name, sig, &[])
}

#[cfg(target_os = "android")]
pub fn call_object_args<'a>(
    env: &mut jni::JNIEnv<'a>,
    obj: &jni::objects::JObject,
    name: &str,
    sig: &str,
    args: &[jni::objects::JValue],
) -> Option<jni::objects::JObject<'a>> {
    match env.call_method(obj, name, sig, args) {
        Ok(v) => v.l().ok().filter(|o| !o.is_null()),
        Err(e) => {
            log::info!("jamandroid: {name} fehlgeschlagen: {e}");
            None
        }
    }
}

#[cfg(target_os = "android")]
pub fn jstring(env: &mut jni::JNIEnv, obj: &jni::objects::JObject) -> Option<String> {
    let text = unsafe { jni::objects::JString::from_raw(obj.as_raw()) };
    env.get_string(&text).ok().map(|s| s.into())
}

#[cfg(target_os = "android")]
pub fn describe_and_clear(env: &mut jni::JNIEnv) -> String {
    if !matches!(env.exception_check(), Ok(true)) {
        return String::new();
    }
    let throwable = env.exception_occurred().ok();
    let _ = env.exception_clear();
    throwable
        .and_then(|throwable| {
            let s = env
                .call_method(&throwable, "toString", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            jstring(env, &s)
        })
        .unwrap_or_default()
}

#[cfg(target_os = "android")]
pub fn clear(env: &mut jni::JNIEnv) {
    if let Ok(true) = env.exception_check() {
        let _ = env.exception_clear();
    }
}
