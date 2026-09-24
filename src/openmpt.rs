use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

// Minimal FFI-Bindung an libopenmpt (nur die für die Live-Engine nötigen Calls).
// Header: libopenmpt/libopenmpt.h

#[repr(C)]
pub struct openmpt_module {
    _private: [u8; 0],
}

pub type openmpt_log_func = Option<unsafe extern "C" fn(
    msg: *const c_char,
    user: *mut c_void,
)>;
pub type openmpt_unknown_cb = Option<unsafe extern "C" fn(
    unknown: *const c_char,
    user: *mut c_void,
) -> c_int>;

#[link(name = "openmpt")]
extern "C" {
    pub fn openmpt_module_create_from_memory(
        filedata: *const u8,
        filesize: usize,
        logfunc: openmpt_log_func,
        loguser: *mut c_void,
        ctls: *const u8, // optional ctl string, NULL ok
    ) -> *mut openmpt_module;

    pub fn openmpt_module_destroy(mod_: *mut openmpt_module);

    pub fn openmpt_module_read_stereo(
        mod_: *mut openmpt_module,
        samplerate: c_int,
        count: usize,
        left: *mut c_void,
        right: *mut c_void,
    ) -> c_int;

    pub fn openmpt_module_set_position_seconds(
        mod_: *mut openmpt_module,
        seconds: f64,
    ) -> f64;

    pub fn openmpt_module_get_position_seconds(
        mod_: *mut openmpt_module,
    ) -> f64;

    // Sampling kenntnisse
    pub fn openmpt_module_get_num_channels(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_num_patterns(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_num_orders(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_num_samples(mod_: *mut openmpt_module) -> c_int;

    pub fn openmpt_module_get_order_pattern(
        mod_: *mut openmpt_module,
        order: c_int,
    ) -> c_int;
    pub fn openmpt_module_get_pattern_num_rows(
        mod_: *mut openmpt_module,
        pattern: c_int,
    ) -> c_int;
    pub fn openmpt_module_get_current_order(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_current_pattern(
        mod_: *mut openmpt_module,
    ) -> c_int;
    pub fn openmpt_module_get_current_row(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_pattern_rows_per_beat(
        mod_: *mut openmpt_module,
        pattern: c_int,
    ) -> c_int;
    pub fn openmpt_module_get_current_tempo(mod_: *mut openmpt_module) -> c_int;
    pub fn openmpt_module_get_current_speed(
        mod_: *mut openmpt_module,
    ) -> c_int;
    pub fn openmpt_module_get_current_global_volume(
        mod_: *mut openmpt_module,
    ) -> c_int;
    pub fn openmpt_module_get_metadata(
        mod_: *mut openmpt_module,
    ) -> *const c_char;

    pub fn openmpt_module_get_sample_name(
        mod_: *mut openmpt_module,
        sample: c_int,
    ) -> *const c_char;
    pub fn openmpt_module_get_pattern_name(
        mod_: *mut openmpt_module,
        pattern: c_int,
    ) -> *const c_char;

    // Effektinformation je Zelle
    pub fn openmpt_module_get_pattern_row_channel_command(
        mod_: *mut openmpt_module,
        pattern: c_int,
        row: c_int,
        channel: c_int,
        command: c_int,
    ) -> c_int;

    // Steuerung
    pub fn openmpt_module_set_position_order_row(
        mod_: *mut openmpt_module,
        order: c_int,
        row: c_int,
    ) -> c_int;
    pub fn openmpt_module_set_tempo_factor(
        mod_: *mut openmpt_module,
        factor: f64,
    );
}

pub struct Mod {
    pub ptr: *mut openmpt_module,
}

impl Mod {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let m = unsafe {
            openmpt_module_create_from_memory(
                data.as_ptr(),
                data.len(),
                None,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if m.is_null() {
            Err(format!("libopenmpt konnte {:?} nicht laden", path))
        } else {
            Ok(Mod { ptr: m })
        }
    }

    pub fn info(&self) -> String {
        unsafe {
            let md = openmpt_module_get_metadata(self.ptr);
            if md.is_null() {
                return String::new();
            }
            CStr::from_ptr(md).to_string_lossy().into_owned()
        }
    }

    pub fn sample_name(&self, s: i32) -> String {
        unsafe {
            let p = openmpt_module_get_sample_name(self.ptr, s);
            if p.is_null() {
                return String::new();
            }
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }

    pub fn set_order(&self, order: i32, row: i32) {
        unsafe { openmpt_module_set_position_order_row(self.ptr, order, row) };
    }
}

impl Drop for Mod {
    fn drop(&mut self) {
        unsafe { openmpt_module_destroy(self.ptr) }
    }
}

// Vorsicht: diese Funktionen werden über die *modernen* Namen auf dem Header
// freigelegt; die konkreten Symbolnamen prüfen wir beim Linken.
