//! Link-Flags für das cdylib.
//!
//! Die C++-Runtime muss ganz am Ende der Linkzeile stehen, weil libopenmpt sie
//! referenziert; ein `-lc++` aus einem abhängigen Crate landet davor und lässt
//! offene Symbole stehen. Der Suchpfad zeigt bewusst auf das versionierte
//! API-Verzeichnis: im unversionierten liegen auch `.a`-Archive, und `-lc`/`-lz`
//! würden sonst statisch aufgelöst (eigenes `getauxval`, Crash beim Laden).

use std::path::PathBuf;

const API_LEVEL: &str = "26";

fn main() {
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    let triple = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "aarch64-linux-android",
        Ok("arm") => "arm-linux-androideabi",
        Ok("x86_64") => "x86_64-linux-android",
        other => panic!("C++-Runtime für {other:?} nicht eingerichtet"),
    };

    let ndk = ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("ANDROID_NDK_HOME/NDK_HOME nicht gesetzt"));

    let host_dir = std::fs::read_dir(ndk.join("toolchains/llvm/prebuilt"))
        .expect("NDK: toolchains/llvm/prebuilt fehlt")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.join("sysroot/usr/lib").exists())
        .expect("NDK: kein Host-Verzeichnis mit sysroot");

    let api_dir = host_dir
        .join("sysroot/usr/lib")
        .join(triple)
        .join(API_LEVEL);
    println!("cargo:rustc-link-search=native={}", api_dir.display());

    let cxx = host_dir
        .join("sysroot/usr/lib")
        .join(triple)
        .join("libc++_shared.so");
    if !cxx.exists() {
        panic!("fehlt: {}", cxx.display());
    }
    println!("cargo:rustc-link-arg={}", cxx.display());
}
