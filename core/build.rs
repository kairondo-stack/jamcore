//! Desktop linkt gegen das libopenmpt aus der Nix-Shell.
//! Android braucht eine passende Bibliothek, die hier einmal mit dem
//! NDK-Build-System aus der Quelle gebaut und danach aus dem Cache benutzt wird.
//! Es wird `build/android_ndk/Android.mk` benutzt, also der offizielle Weg des
//! Projekts (das CMake liegt nur im Release-Tarball, nicht im Git-Baum).

use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "0.8.7";
const SHA256: &str = "2a73f6aa795eced6856e480deaece29775f3d654a7fc024195a69d8d3d94f3e2";
const URL: &str = "https://github.com/OpenMPT/openmpt/archive/refs/tags/libopenmpt-0.8.7.tar.gz";
const API_LEVEL: &str = "26";
const SRC_DIRS: [&str; 6] = [
    "common",
    "include",
    "libopenmpt",
    "sounddsp",
    "soundlib",
    "src",
];

fn main() {
    println!("cargo:rerun-if-env-changed=LIBOPENMPT_SRC");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    let abi = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64-v8a",
        Ok("arm") => "armeabi-v7a",
        Ok("x86_64") => "x86_64",
        other => panic!("libopenmpt-Bau für {other:?} ist nicht eingerichtet"),
    };

    if let Ok(dir) = std::env::var("LIBOPENMPT_SRC") {
        let dir = PathBuf::from(dir);
        let lib = find_lib(&dir).unwrap_or_else(|| {
            panic!("LIBOPENMPT_SRC={} enthält kein libopenmpt.a", dir.display())
        });
        emit(&lib);
        return;
    }

    let ndk_tag = ndk_home()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "nohome".to_string());
    let cache = target_dir()
        .join("openmpt")
        .join(format!("{VERSION}-{abi}-{ndk_tag}"));
    let lib = build(&cache, abi).unwrap_or_else(|e| panic!("libopenmpt {VERSION}: {e}"));
    emit(&lib);
}

fn emit(lib: &Path) {
    let dir = lib.parent().expect("Bibliothekspfad");
    println!("cargo:rustc-link-search=native={}", dir.display());
    if lib.extension().is_some_and(|e| e == "a") {
        println!("cargo:rustc-link-lib=static=openmpt");
    } else {
        println!("cargo:rustc-link-lib=openmpt");
    }
    println!("cargo:rustc-link-lib=z");
    println!("cargo:rustc-link-lib=m");
}

fn target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    // OUT_DIR = <target>/<profil>/build/<paket-hash>/out
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    out.ancestors().nth(4).map(Path::to_path_buf).unwrap_or(out)
}

fn find_lib(dir: &Path) -> Option<PathBuf> {
    ["libopenmpt.a", "libopenmpt.so"]
        .iter()
        .map(|f| dir.join(f))
        .find(|p| p.exists())
}

fn build(cache: &Path, abi: &str) -> Result<PathBuf, String> {
    let lib = cache.join("libopenmpt.a");
    if lib.exists() {
        return Ok(lib);
    }

    let src = source(cache)?;
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    // ndk-build bricht bei Leerzeichen im Pfad ab, deshalb liegt das
    // Hilfsprojekt im Temp-Verzeichnis; das Ergebnis landet im Cache.
    let proj = work_dir(cache, abi);
    let _ = std::fs::remove_dir_all(&proj);
    let jni = proj.join("jni");
    std::fs::create_dir_all(&jni).map_err(|e| e.to_string())?;

    let mk = std::fs::read_to_string(src.join("build/android_ndk/Android.mk"))
        .map_err(|e| format!("build/android_ndk/Android.mk: {e}"))?;
    let mk = mk.replace("$(BUILD_SHARED_LIBRARY)", "$(BUILD_STATIC_LIBRARY)");
    std::fs::write(jni.join("Android.mk"), mk).map_err(|e| e.to_string())?;

    let app_mk = std::fs::read_to_string(src.join("build/android_ndk/Application.mk"))
        .map_err(|e| format!("build/android_ndk/Application.mk: {e}"))?;
    let app_mk = format!("{app_mk}\nAPP_ABI := {abi}\nAPP_PLATFORM := android-{API_LEVEL}\n");
    std::fs::write(jni.join("Application.mk"), app_mk).map_err(|e| e.to_string())?;
    std::fs::write(proj.join("AndroidManifest.xml"), "<manifest/>\n").map_err(|e| e.to_string())?;

    for dir in SRC_DIRS {
        let link = jni.join(dir);
        if !link.exists() {
            std::os::unix::fs::symlink(src.join(dir), &link).map_err(|e| e.to_string())?;
        }
    }

    let ndk = ndk_home()?;
    let libs = proj.join("libs");
    run(Command::new(ndk.join("ndk-build"))
        .env("NDK_PROJECT_PATH", &proj)
        .env("NDK_APPLICATION_MK", jni.join("Application.mk"))
        .env("APP_BUILD_SCRIPT", jni.join("Android.mk"))
        .env("NDK_OUT", proj.join("obj"))
        .env("NDK_LIBS_OUT", &libs)
        .arg(format!("-j{}", num_jobs())))?;

    let found = find_lib(&libs.join(abi))
        .or_else(|| find_lib(&libs))
        .or_else(|| find_lib(&proj.join("obj/local").join(abi)))
        .ok_or_else(|| {
            format!(
                "ndk-build hat keine Bibliothek erzeugt (suche in {})",
                libs.display()
            )
        })?;
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    if found != lib {
        std::fs::copy(&found, &lib).map_err(|e| e.to_string())?;
    }
    Ok(lib)
}

fn source(cache: &Path) -> Result<PathBuf, String> {
    let src_dir = cache.join("src");
    if let Some(dir) = find_source(&src_dir) {
        return Ok(dir);
    }
    let tarball = cache.join(format!("libopenmpt-{VERSION}.tar.gz"));
    fetch(&tarball)?;
    std::fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;
    run(Command::new("tar")
        .arg("xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&src_dir))?;
    find_source(&src_dir).ok_or_else(|| {
        format!(
            "kein libopenmpt-Quellbaum in {} gefunden",
            src_dir.display()
        )
    })
}

fn work_dir(cache: &Path, abi: &str) -> PathBuf {
    // ndk-build bricht bei Leerzeichen im Pfad ab, deshalb /tmp mit einem
    // Anhang, der den Cache-Pfad eindeutig macht (parallele Builds).
    let tag = format!("{:x}", md5ish(cache.to_string_lossy().as_bytes()));
    PathBuf::from("/tmp").join(format!("jamcore-openmpt-{VERSION}-{abi}-{tag}"))
}

/// Kleine Prüfsumme, damit parallele Builds getrennte Hilfsverzeichnisse nutzen.
fn md5ish(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn find_source(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.join("build/android_ndk/Android.mk").exists())
}

fn fetch(tarball: &Path) -> Result<(), String> {
    if tarball.exists() && sha256(tarball) == SHA256 {
        return Ok(());
    }
    if let Some(parent) = tarball.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    println!("cargo:warning=libopenmpt {VERSION} wird geladen: {URL}");
    let curl = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(tarball)
        .arg(URL)
        .status();
    let ok = matches!(curl, Ok(s) if s.success())
        || Command::new("wget")
            .args(["-q", "-O"])
            .arg(tarball)
            .arg(URL)
            .status()
            .is_ok_and(|s| s.success());
    if !ok {
        return Err(format!("Download fehlgeschlagen: {URL}"));
    }
    let got = sha256(tarball);
    if got != SHA256 {
        let _ = std::fs::remove_file(tarball);
        return Err(format!("Pruefsumme falsch: {got} statt {SHA256}"));
    }
    Ok(())
}

fn sha256(path: &Path) -> String {
    let out = Command::new("sha256sum")
        .arg(path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn ndk_home() -> Result<PathBuf, String> {
    for key in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"] {
        if let Ok(v) = std::env::var(key) {
            if Path::new(&v).exists() {
                return Ok(PathBuf::from(v));
            }
        }
    }
    Err("ANDROID_NDK_HOME/NDK_HOME nicht gesetzt".to_string())
}

fn num_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let rendered = format!("{cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| format!("starten fehlgeschlagen ({rendered}): {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("fehlgeschlagen ({rendered}): {status}"))
    }
}
