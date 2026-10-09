//! Minimal-FFI an libopenmpt plus sichere Wrapper für Desktop und Android.
//!
//! Desktop linkt gegen das libopenmpt aus der Nix-Shell, Android gegen die
//! statische Bibliothek, die `build.rs` für das NDK baut (siehe dort).

use std::ffi::CStr;
use std::os::raw::{c_char, c_float, c_int, c_void};

#[repr(C)]
pub struct openmpt_module {
    _private: [u8; 0],
}

#[allow(non_camel_case_types)]
pub type openmpt_log_func = Option<unsafe extern "C" fn(msg: *const c_char, user: *mut c_void)>;
#[allow(non_camel_case_types)]
pub type openmpt_unknown_cb =
    Option<unsafe extern "C" fn(unknown: *const c_char, user: *mut c_void) -> c_int>;

#[link(name = "openmpt")]
extern "C" {
    pub fn openmpt_module_create_from_memory(
        filedata: *const u8,
        filesize: usize,
        logfunc: openmpt_log_func,
        loguser: *mut c_void,
        ctls: *const u8,
    ) -> *mut openmpt_module;

    pub fn openmpt_module_destroy(mod_: *mut openmpt_module);

    pub fn openmpt_module_read_float_stereo(
        mod_: *mut openmpt_module,
        samplerate: c_int,
        count: usize,
        left: *mut c_float,
        right: *mut c_float,
    ) -> usize;

    pub fn openmpt_module_set_repeat_count(mod_: *mut openmpt_module, count: c_int) -> c_int;
    pub fn openmpt_module_set_position_order_row(
        mod_: *mut openmpt_module,
        order: c_int,
        row: c_int,
    ) -> c_int;
    pub fn openmpt_module_set_position_seconds(mod_: *mut openmpt_module, seconds: f64) -> f64;
    pub fn openmpt_module_get_position_seconds(mod_: *mut openmpt_module) -> f64;
    pub fn openmpt_module_get_duration_seconds(mod_: *mut openmpt_module) -> f64;
    pub fn openmpt_module_set_tempo_factor(mod_: *mut openmpt_module, factor: f64);

    pub fn openmpt_module_get_num_channels(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_num_patterns(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_num_orders(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_num_samples(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_order_pattern(mod_: *mut openmpt_module, order: c_int) -> c_int;
    pub fn openmpt_module_get_pattern_num_rows(mod_: *mut openmpt_module, pattern: c_int) -> c_int;
    pub fn openmpt_module_get_current_order(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_current_pattern(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_current_row(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_pattern_rows_per_beat(
        mod_: *mut openmpt_module,
        pattern: c_int,
    ) -> c_int;
    pub fn openmpt_module_get_current_tempo(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_current_speed(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_metadata(
        mod_: *mut openmpt_module,
        key: *const c_char,
    ) -> *const c_char;
    pub fn openmpt_free_string(str_: *const c_char);
    pub fn openmpt_module_get_sample_name(
        mod_: *mut openmpt_module,
        sample: c_int,
    ) -> *const c_char;
}

/// Geladenes Modul. `ptr` bleibt sichtbar für die Desktop-Taktlogik, die direkt
/// liest; die Wrapper-Methoden decken den Rest ab. Der Audio-Thread auf
/// Android rendert daraus, deshalb ist Mod dort Send.
pub struct Mod {
    pub ptr: *mut openmpt_module,
    #[allow(dead_code)]
    bytes: Vec<u8>,
}

unsafe impl Send for Mod {}

impl Mod {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        Self::from_bytes(data).map_err(|e| format!("{e} ({path})"))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        let ptr = unsafe {
            openmpt_module_create_from_memory(
                bytes.as_ptr(),
                bytes.len(),
                None,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if ptr.is_null() {
            Err("libopenmpt konnte das Modul nicht laden".to_string())
        } else {
            Ok(Mod { ptr, bytes })
        }
    }

    /// Rendert `frames` Frames nach links/rechts. Gibt 0 zurück, wenn das
    /// Modul am Ende ist (kein Repeat).
    pub fn read_stereo(&self, sample_rate: c_int, left: &mut [f32], right: &mut [f32]) -> usize {
        let count = left.len().min(right.len());
        if count == 0 {
            return 0;
        }
        let read = unsafe {
            openmpt_module_read_float_stereo(
                self.ptr,
                sample_rate,
                count,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
            )
        };
        read.min(count)
    }

    pub fn set_repeat_forever(&self) {
        unsafe {
            openmpt_module_set_repeat_count(self.ptr, -1);
        }
    }

    pub fn set_order(&self, order: i32, row: i32) {
        unsafe {
            openmpt_module_set_position_order_row(self.ptr, order, row);
        }
    }

    pub fn seek_seconds(&self, seconds: f64) {
        unsafe {
            openmpt_module_set_position_seconds(self.ptr, seconds);
        }
    }

    pub fn position_seconds(&self) -> f64 {
        unsafe { openmpt_module_get_position_seconds(self.ptr) }
    }

    pub fn duration_seconds(&self) -> f64 {
        unsafe { openmpt_module_get_duration_seconds(self.ptr) }
    }

    pub fn set_tempo_factor(&self, factor: f64) {
        unsafe {
            openmpt_module_set_tempo_factor(self.ptr, factor);
        }
    }

    pub fn current_order(&self) -> i32 {
        unsafe { openmpt_module_get_current_order(self.ptr) }
    }

    pub fn current_row(&self) -> i32 {
        unsafe { openmpt_module_get_current_row(self.ptr) }
    }

    pub fn current_pattern(&self) -> i32 {
        unsafe { openmpt_module_get_current_pattern(self.ptr) }
    }

    pub fn rows_per_beat(&self, pattern: i32) -> i32 {
        unsafe { openmpt_module_get_pattern_rows_per_beat(self.ptr, pattern) }.max(1)
    }

    /// Anzahl der Zeilen eines Patterns (für den Struktur-Dump).
    pub fn pattern_num_rows(&self, pattern: i32) -> i32 {
        unsafe { openmpt_module_get_pattern_num_rows(self.ptr, pattern) }
    }

    pub fn order_pattern(&self, order: i32) -> i32 {
        unsafe { openmpt_module_get_order_pattern(self.ptr, order) }
    }

    pub fn num_orders(&self) -> i32 {
        unsafe { openmpt_module_get_num_orders(self.ptr) }
    }

    pub fn num_patterns(&self) -> i32 {
        unsafe { openmpt_module_get_num_patterns(self.ptr) }
    }

    pub fn num_channels(&self) -> i32 {
        unsafe { openmpt_module_get_num_channels(self.ptr) }
    }

    pub fn num_samples(&self) -> i32 {
        unsafe { openmpt_module_get_num_samples(self.ptr) }
    }

    pub fn current_tempo(&self) -> i32 {
        unsafe { openmpt_module_get_current_tempo(self.ptr) }
    }

    pub fn current_speed(&self) -> i32 {
        unsafe { openmpt_module_get_current_speed(self.ptr) }
    }

    /// Metadaten-Abfrage. `key` laut libopenmpt.h, z. B. "title", "artist".
    pub fn metadata(&self, key: &str) -> String {
        let mut key_c = key.as_bytes().to_vec();
        key_c.push(0);
        unsafe {
            let p = openmpt_module_get_metadata(self.ptr, key_c.as_ptr() as *const c_char);
            if p.is_null() {
                return String::new();
            }
            let value = CStr::from_ptr(p).to_string_lossy().into_owned();
            openmpt_free_string(p);
            value
        }
    }

    pub fn info(&self) -> String {
        let mut lines = Vec::new();
        for key in ["type_long", "title", "artist", "date"] {
            let value = self.metadata(key);
            if !value.is_empty() {
                lines.push(format!("{key}: {value}"));
            }
        }
        if lines.is_empty() {
            lines.push(format!("Modul ({} Kanäle)", self.num_channels()));
        }
        lines.join("\n")
    }

    pub fn sample_name(&self, s: i32) -> String {
        unsafe {
            let p = openmpt_module_get_sample_name(self.ptr, s);
            if p.is_null() {
                return String::new();
            }
            // libopenmpt gibt den String dynamisch heraus — ohne
            // openmpt_free_string leckt jede Abfrage (der Player fragt je Frame).
            let name = CStr::from_ptr(p).to_string_lossy().into_owned();
            openmpt_free_string(p);
            name
        }
    }

    /// Modultitel, sonst der Dateiname.
    pub fn title(&self) -> String {
        let title = self.metadata("title");
        if title.trim().is_empty() {
            "Modul".to_string()
        } else {
            title
        }
    }
}

impl Drop for Mod {
    fn drop(&mut self) {
        unsafe {
            openmpt_module_destroy(self.ptr);
        }
    }
}
