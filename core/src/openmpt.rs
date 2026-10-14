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

/// Undurchsichtiger Handle der ext-Schnittstelle (libopenmpt_ext.h).
#[repr(C)]
pub struct openmpt_module_ext {
    _private: [u8; 0],
}

/// Funktionszeiger der ext-Schnittstelle „interactive". Die Reihenfolge stammt
/// eins zu eins aus `libopenmpt_ext.h` — sie darf nicht veraendert werden,
/// sonst zeigt etwa `play_note` ins Leere. Genutzt werden nur die letzten
/// beiden Felder.
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct openmpt_module_ext_interface_interactive {
    pub set_current_speed: Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int) -> c_int>,
    pub set_current_tempo: Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int) -> c_int>,
    pub set_tempo_factor: Option<unsafe extern "C" fn(*mut openmpt_module_ext, f64) -> c_int>,
    pub get_tempo_factor: Option<unsafe extern "C" fn(*mut openmpt_module_ext) -> f64>,
    pub set_pitch_factor: Option<unsafe extern "C" fn(*mut openmpt_module_ext, f64) -> c_int>,
    pub get_pitch_factor: Option<unsafe extern "C" fn(*mut openmpt_module_ext) -> f64>,
    pub set_global_volume: Option<unsafe extern "C" fn(*mut openmpt_module_ext, f64) -> c_int>,
    pub get_global_volume: Option<unsafe extern "C" fn(*mut openmpt_module_ext) -> f64>,
    pub set_channel_volume:
        Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int, f64) -> c_int>,
    pub get_channel_volume: Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int) -> f64>,
    pub set_channel_mute_status:
        Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int, c_int) -> c_int>,
    pub get_channel_mute_status:
        Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int) -> c_int>,
    pub set_instrument_mute_status:
        Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int, c_int) -> c_int>,
    pub get_instrument_mute_status:
        Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int) -> c_int>,
    pub play_note:
        Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int, c_int, f64, f64) -> i32>,
    pub stop_note: Option<unsafe extern "C" fn(*mut openmpt_module_ext, c_int) -> c_int>,
}

impl Default for openmpt_module_ext_interface_interactive {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

/// Fehler-Callback der ext-API (nicht benutzt, bleibt null).
#[allow(non_camel_case_types)]
pub type openmpt_error_func = *const c_void;

#[allow(non_camel_case_types)]
pub type openmpt_log_func = Option<unsafe extern "C" fn(msg: *const c_char, user: *mut c_void)>;
#[allow(non_camel_case_types)]
pub type openmpt_unknown_cb =
    Option<unsafe extern "C" fn(unknown: *const c_char, user: *mut c_void) -> c_int>;

#[link(name = "openmpt")]
extern "C" {
    pub fn openmpt_module_ext_create_from_memory(
        filedata: *const u8,
        filesize: usize,
        logfunc: openmpt_log_func,
        loguser: *mut c_void,
        errfunc: openmpt_error_func,
        erruser: *mut c_void,
        error: *mut c_int,
        error_message: *mut *const c_char,
        ctls: *const c_void,
    ) -> *mut openmpt_module_ext;

    pub fn openmpt_module_ext_destroy(mod_ext: *mut openmpt_module_ext);

    pub fn openmpt_module_ext_get_module(mod_ext: *mut openmpt_module_ext) -> *mut openmpt_module;

    pub fn openmpt_module_ext_get_interface(
        mod_ext: *mut openmpt_module_ext,
        interface_id: *const c_char,
        interface: *mut c_void,
        interface_size: usize,
    ) -> c_int;

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
    pub fn openmpt_module_get_num_instruments(mod_: *mut openmpt_module) -> c_int;
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
    pub fn openmpt_module_get_instrument_name(
        mod_: *mut openmpt_module,
        instrument: c_int,
    ) -> *const c_char;
}

/// Geladenes Modul. `ptr` bleibt sichtbar für die Desktop-Taktlogik, die direkt
/// liest; die Wrapper-Methoden decken den Rest ab. Der Audio-Thread auf
/// Android rendert daraus, deshalb ist Mod dort Send.
pub struct Mod {
    pub ptr: *mut openmpt_module,
    /// ext-Handle nur zum Aufraeumen und fuer die interactive-Schnittstelle.
    ext: *mut openmpt_module_ext,
    /// Funktionszeiger (play_note/stop_note); leer, wenn libopenmpt sie nicht anbietet.
    interactive: Box<openmpt_module_ext_interface_interactive>,
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
        // Ueber die ext-API laden, damit die interactive-Schnittstelle
        // verfuegbar ist (Sample-Anspielen mit `play_note`).
        let ext = unsafe {
            openmpt_module_ext_create_from_memory(
                bytes.as_ptr(),
                bytes.len(),
                None,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if ext.is_null() {
            return Err("libopenmpt konnte das Modul nicht laden".to_string());
        }
        let ptr = unsafe { openmpt_module_ext_get_module(ext) };
        if ptr.is_null() {
            unsafe { openmpt_module_ext_destroy(ext) };
            return Err("libopenmpt: kein Basismodul".to_string());
        }
        let mut interactive = Box::<openmpt_module_ext_interface_interactive>::default();
        let ok = unsafe {
            openmpt_module_ext_get_interface(
                ext,
                c"interactive".as_ptr(),
                &mut *interactive as *mut _ as *mut c_void,
                std::mem::size_of::<openmpt_module_ext_interface_interactive>(),
            )
        };
        // ok == 0: Schnittstelle fehlt; note_spielen liefert dann None.
        let _ = ok;
        Ok(Mod {
            ptr,
            ext,
            interactive,
            bytes,
        })
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

    /// Wiederholungszahl setzen (0 = einmal, -1 = endlos).
    pub fn set_repeat_count(&self, count: i32) -> bool {
        (unsafe { openmpt_module_set_repeat_count(self.ptr, count) }) != 0
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

    /// Rows je Beat eines Patterns; 0 heißt „unbekannt“ (der Aufrufer
    /// entscheidet ueber eine Vorgabe, wie im Desktop-it_live: 32).
    pub fn rows_per_beat(&self, pattern: i32) -> i32 {
        unsafe { openmpt_module_get_pattern_rows_per_beat(self.ptr, pattern) }
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

    pub fn num_instruments(&self) -> i32 {
        unsafe { openmpt_module_get_num_instruments(self.ptr) }
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

    /// Name eines Instruments (0-basiert); leer, wenn der Platz leer ist.
    pub fn instrument_name(&self, i: i32) -> String {
        unsafe {
            let p = openmpt_module_get_instrument_name(self.ptr, i);
            if p.is_null() {
                return String::new();
            }
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

    /// Gültige Instrumentnummer: Wunsch auf den vorhandenen Bereich legen
    /// (Module haben oft weniger Instrumente als der Desktop-Recorder nutzt).
    pub fn instrument_waehlen(&self, wunsch: i32) -> i32 {
        let anzahl = if self.num_instruments() > 0 {
            self.num_instruments()
        } else {
            self.num_samples()
        };
        if anzahl <= 0 {
            0
        } else {
            wunsch.rem_euclid(anzahl)
        }
    }

    /// Spielt eine Note mit dem Instrument/Sample des Moduls (ext-Schnittstelle
    /// `interactive.play_note`). Liefert den Kanal oder `None`, wenn kein Kanal
    /// frei ist oder die Schnittstelle fehlt.
    ///
    /// **Urheberrecht:** Die Samples gehören ihren Rechteinhabern; das Anspielen
    /// zum Mitspielen ist ok, eine Wiederverwendung (Export/Veröffentlichung)
    /// muss vorher geklärt werden.
    pub fn note_spielen(&self, instrument: i32, note: i32, volume: f32) -> Option<i32> {
        let funktion = self.interactive.play_note?;
        let instrument = self.instrument_waehlen(instrument);
        let note = note.clamp(0, 119);
        let volume = (volume.clamp(0.0, 1.0) as f64).clamp(0.0, 1.0);
        let kanal = unsafe { funktion(self.ext, instrument, note, volume, 0.0) };
        (kanal >= 0).then_some(kanal)
    }

    /// Eine zuvor per [`Mod::note_spielen`] gestartete Note beenden.
    pub fn note_stoppen(&self, kanal: i32) {
        if let Some(funktion) = self.interactive.stop_note {
            unsafe {
                funktion(self.ext, kanal);
            }
        }
    }
}

impl Drop for Mod {
    fn drop(&mut self) {
        unsafe {
            // Das ext-Handle besitzt das Basismodul und raeumt es mit auf.
            openmpt_module_ext_destroy(self.ext);
        }
    }
}
