mod notes;
mod osc;
mod shared;

use notes::*;
use shared::*;
use shared_memory::Shmem;
use sdl2::audio::{AudioCallback, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::ttf::Sdl2TtfContext;
use sdl2::video::Window;
use sdl2::EventPump;
use sdl2::Sdl;
use std::f64::consts::PI as M_PI;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::Instant;

const SAMPLE_RATE: i32 = 44100;
const AMPLITUDE: i16 = 28000;

struct SdPtr(*mut SharedData);
unsafe impl Send for SdPtr {}
unsafe impl Sync for SdPtr {}
impl SdPtr {
    fn get(&self) -> *mut SharedData {
        self.0
    }
}

struct AudioGen {
    playing: Arc<AtomicBool>,
    freq_bits: Arc<AtomicU32>,
    phase: f64,
}

impl AudioCallback for AudioGen {
    type Channel = i16;
    fn callback(&mut self, out: &mut [i16]) {
        let f = f32::from_bits(self.freq_bits.load(Ordering::Relaxed));
        let play = self.playing.load(Ordering::Relaxed);
        let phase_inc = 2.0 * M_PI * f as f64 / SAMPLE_RATE as f64;
        for s in out.iter_mut() {
            if play {
                *s = (AMPLITUDE as f64 * self.phase.sin()) as i16;
                self.phase += phase_inc;
                if self.phase > 2.0 * M_PI {
                    self.phase -= 2.0 * M_PI;
                }
            } else {
                *s = 0;
            }
        }
    }
}

/// Test-Hook: E2E_KEYS="R,1,1,2,3,0" beim Start in die SDL-Queue legen.
fn inject_test_keys() {
    let Ok(spec) = std::env::var("E2E_KEYS") else {
        return;
    };
    std::thread::spawn(move || push_test_keys(&spec));
}

fn push_test_keys(spec: &str) {
    use sdl2::keyboard::{Keycode as K, Scancode as S};
    for name in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(ms) = name.strip_prefix("wait:") {
            std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap_or(0)));
            continue;
        }
        if let Some(rest) = name.strip_prefix("click:") {
            let mut parts = rest.split(':');
            let x: i32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let y: i32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            unsafe {
                let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
                raw.type_ = sdl2::sys::SDL_EventType::SDL_MOUSEBUTTONDOWN as u32;
                raw.button.type_ = sdl2::sys::SDL_EventType::SDL_MOUSEBUTTONDOWN as u32;
                raw.button.button = 1;
                raw.button.state = 1;
                raw.button.x = x;
                raw.button.y = y;
                sdl2::sys::SDL_PushEvent(&mut raw);
            }
            continue;
        }
        if name.eq_ignore_ascii_case("quit") {
            unsafe {
                let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
                raw.type_ = sdl2::sys::SDL_EventType::SDL_QUIT as u32;
                sdl2::sys::SDL_PushEvent(&mut raw);
            }
            continue;
        }
        let (k, _s) = match name.to_ascii_lowercase().as_str() {
            "tab" => (K::Tab, S::Tab),
            "right" => (K::Right, S::Right),
            "left" => (K::Left, S::Left),
            "up" => (K::Up, S::Up),
            "down" => (K::Down, S::Down),
            "return" => (K::Return, S::Return),
            "0" => (K::Num0, S::Num0),
            "1" => (K::Num1, S::Num1),
            "2" => (K::Num2, S::Num2),
            "3" => (K::Num3, S::Num3),
            "4" => (K::Num4, S::Num4),
            "5" => (K::Num5, S::Num5),
            "6" => (K::Num6, S::Num6),
            "7" => (K::Num7, S::Num7),
            "8" => (K::Num8, S::Num8),
            "9" => (K::Num9, S::Num9),
            "a" => (K::A, S::A),
            "b" => (K::B, S::B),
            "c" => (K::C, S::C),
            "d" => (K::D, S::D),
            "e" => (K::E, S::E),
            "f" => (K::F, S::F),
            "g" => (K::G, S::G),
            "h" => (K::H, S::H),
            "i" => (K::I, S::I),
            "j" => (K::J, S::J),
            "k" => (K::K, S::K),
            "l" => (K::L, S::L),
            "m" => (K::M, S::M),
            "n" => (K::N, S::N),
            "o" => (K::O, S::O),
            "p" => (K::P, S::P),
            "q" => (K::Q, S::Q),
            "r" => (K::R, S::R),
            "s" => (K::S, S::S),
            "t" => (K::T, S::T),
            "u" => (K::U, S::U),
            "v" => (K::V, S::V),
            "w" => (K::W, S::W),
            "x" => (K::X, S::X),
            "y" => (K::Y, S::Y),
            "z" => (K::Z, S::Z),
            "f5" => (K::F5, S::F5),
            "f8" => (K::F8, S::F8),
            "f9" => (K::F9, S::F9),
            _ => continue,
        };
        unsafe {
            let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
            raw.key.type_ = sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32;
            raw.key.state = 1;
            raw.key.keysym.sym = k.into_i32();
            raw.key.keysym.scancode = sdl2::sys::SDL_GetScancodeFromKey(k.into_i32());
            sdl2::sys::SDL_PushEvent(&mut raw);
        }
    }
}

fn poll_event_compat() -> Option<Event> {
    unsafe {
        loop {
            let mut raw = std::mem::MaybeUninit::uninit();
            if sdl2::sys::SDL_PollEvent(raw.as_mut_ptr()) != 1 {
                return None;
            }
            let ev = raw.assume_init();
            let t = ev.type_;
            if t == sdl2::sys::SDL_EventType::SDL_QUIT as u32
                || t == sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32
                || t == sdl2::sys::SDL_EventType::SDL_KEYUP as u32
            {
                return Some(Event::from_ll(ev));
            }
        }
    }
}

fn ticks() -> u32 {
    thread_local! {
        static TIMER: sdl2::TimerSubsystem =
            sdl2::init().expect("SDL init").timer().expect("timer subsystem");
    }
    TIMER.with(|t| t.ticks())
}
fn ticks_f() -> f32 {
    ticks() as f32 / 1000.0
}

fn digit_value(kc: Keycode) -> Option<u32> {
    let v = kc.into_i32();
    let base = Keycode::Num0.into_i32();
    if v >= base && v <= Keycode::Num9.into_i32() {
        Some((v - base) as u32)
    } else {
        None
    }
}

fn fkey_value(kc: Keycode) -> Option<u32> {
    let v = kc.into_i32();
    let base = Keycode::F1.into_i32();
    if v >= base && v <= Keycode::F10.into_i32() {
        Some((v - base) as u32 + 1)
    } else {
        None
    }
}

fn read_stdin_line() -> String {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    let _ = stdin.lock().read_line(&mut line);
    line.trim().to_string()
}

/// Klassifikation eines KeyDown-Events im Aufnahme-Modus (record()).
#[derive(Debug, PartialEq, Clone, Copy)]
enum RecordAction {
    Stop,
    ToggleAlleseins,
    /// alleseins && !scaletoa: fester Notenwert 1
    NoteFixedOne,
    /// Ziffer 1-9 * numfactor
    NoteDigit(i32),
    /// F1-F10 * numfactor
    NoteFKey(i32),
    PageUp(bool),
    SelectKk,
    SelectKr,
    SelectNone,
    Schritt3,
    NumFactor(i32),
    RStep(f32),
    None,
}

/// Offset der vorherigen Note für den Playhead (C++: pattern[noteIndex-1].timestamp).
/// Bei noteIndex==0 ist der Offset negativ: letzter minus vorletzter Timestamp.
fn play_time_old(pattern: &[Note3], note_index: usize, zeitfakto: f32) -> f32 {
    if note_index == 0 {
        if pattern.len() >= 2 {
            (pattern[pattern.len() - 2].timestamp - pattern[pattern.len() - 1].timestamp) * zeitfakto
        } else {
            0.0
        }
    } else if note_index <= pattern.len() {
        pattern[note_index - 1].timestamp * zeitfakto
    } else {
        pattern[pattern.len() - 1].timestamp * zeitfakto
    }
}

/// Startzeit-Fortschreibung am Loop-Ende (C++: playStartTime += letzter Timestamp).
fn loop_start_advance(pattern: &[Note3], zeitfakto: f32) -> f32 {
    pattern
        .last()
        .map(|n| n.timestamp * zeitfakto)
        .unwrap_or(0.0)
}

/// Zeitpunkt einer Note im Thread-Playback (C++: timestamp*zf/dauer*pd).
fn thread_note_time(timestamp: f32, zeitfakto: f32, dauer: f32, pd: f32) -> f32 {
    let dauer = if dauer == 0.0 { 1.0 } else { dauer };
    timestamp * zeitfakto / dauer * pd
}

/// Kumulierter Startzeit-Offset nach `loops_done` durchlaufenen Loops
/// (C++: play3StartTime1 += letzte Notendauer je Loop).
fn loop_cycle_offset(notes: &[Note3], loops_done: i32, zeitfakto: f32, dauer: f32, pd: f32) -> f32 {
    match notes.last() {
        Some(last) => thread_note_time(last.timestamp, zeitfakto, dauer, pd) * loops_done.max(0) as f32,
        None => 0.0,
    }
}

/// CSV-Zeilen wie C++: `timestamp,instrument,pitch,glide` mit glide als 0/1.
fn notes_to_csv(notes: &[Note3]) -> String {
    let mut s = String::new();
    for note in notes {
        s += &format!(
            "{},{},{},{}\n",
            note.timestamp,
            note.instrument,
            note.pitch,
            if note.glide { 1 } else { 0 }
        );
    }
    s
}

/// Liest CSV-Zeilen; akzeptiert 0/1 und true/false für glide.
fn notes_from_csv(content: &str) -> Vec<Note3> {
    let mut out = Vec::new();
    for line in content.lines() {
        let tokens: Vec<&str> = line.split(',').collect();
        if tokens.len() >= 4 {
            out.push(Note3 {
                timestamp: tokens[0].trim().parse().unwrap_or(0.0),
                instrument: tokens[1].trim().parse().unwrap_or(0),
                pitch: tokens[2].trim().parse().unwrap_or(0),
                glide: tokens[3].trim() == "1" || tokens[3].trim() == "true",
                p2: 0.0,
            });
        }
    }
    out
}

/// Dateiname eines Patterns; Slot-Index wie im Einzel-Speichern (Taste 1 -> Slot 0).
fn csv_filename_for_slot(slot: usize) -> String {
    format!("pattern {slot}.csv")
}

/// Totale Ordnung für f32: NaN sortiert ans Ende statt zu panicken.
fn total_cmp_f32(a: f32, b: f32) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

/// Sortiert Noten nach Timestamp; NaN sortiert stabil ans Ende statt zu panicken.
fn sort_notes_by_timestamp(notes: &mut [Note3]) {
    notes.sort_by(|a, b| total_cmp_f32(a.timestamp, b.timestamp));
}

/// Notenwert-Taste in removeAndRecord: F1=10..F10=19, Ziffern 1-9,
/// bei alleseins immer 1. `None` für alles andere.
fn remove_and_record_note_value(kc: Keycode, alleseins: bool) -> Option<i32> {
    if let Some(d) = digit_value(kc).filter(|d| (1..=9).contains(d)) {
        return Some(if alleseins { 1 } else { d as i32 });
    }
    fkey_value(kc).map(|f| f as i32 + 9)
}

/// Step-Graph-Klicks zu einer stückweise linearen Kurve (x -> y) füllen.
fn interpolate_step_graph(step: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let Some(&(last_x, _)) = step.last() else {
        return Vec::new();
    };
    if last_x < 0 {
        return Vec::new();
    }
    let mut out = vec![(0, 0); last_x as usize + 1];
    for pair in step.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        if x1 <= x0 {
            continue;
        }
        let end = x1.min(last_x);
        for x in x0.max(0)..=end {
            let t = (x - x0) as f32 / (x1 - x0) as f32;
            out[x as usize] = (x, (y0 as f32 + (y1 - y0) as f32 * t) as i32);
        }
    }
    for &(x, y) in step {
        if (0..=last_x).contains(&x) {
            out[x as usize] = (x, y);
        }
    }
    out
}

/// "aefill": Step-Kurve in Schrittweiten-Sprüngen abtasten.
fn ae_points(stepxy: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut i: i32 = 0;
    while (i as usize) < stepxy.len() {
        let y = stepxy[i as usize].1;
        if y != 0 {
            out.push((i, y));
            i += y.abs().max(1);
        } else {
            i += 1;
        }
    }
    out
}

/// Graph-Punkte in Noten wandeln (y/10 = Halbtöne ab MIDI 60), letzte Note entfällt.
fn graph_points_to_notes(points: &[(i32, i32)], start_time: f32, end_time: f32) -> Vec<Note3> {
    let Some(&(last_x, _)) = points.last() else {
        return Vec::new();
    };
    if last_x <= 0 {
        return Vec::new();
    }
    let inv = 1.0 / last_x as f32;
    points
        .iter()
        .take(points.len().saturating_sub(1))
        .map(|&(x, y)| {
            let t = x as f32 * inv;
            Note3::new(start_time + t * (end_time - start_time), 30, 60 + y / 10)
        })
        .collect()
}

/// Eingabezeichen anhängen; nur ein Dezimalpunkt (KpDecimal) wird akzeptiert.
fn append_input_token(input: &mut String, ch: char) {
    if ch == '.' && input.contains('.') {
        return;
    }
    input.push(ch);
}

/// Sinus-Interpolation zwischen Start-/End-Index (C++: interpolateNotesWithSinusPitch).
fn sinus_interpolate(
    notes4: &[Note3],
    start: usize,
    end: usize,
    steps_per_intervala: i32,
    steps_per_intervale: i32,
    factorpi: f32,
    noteanz: f32,
    curve: i32,
) -> Vec<Note3> {
    let mut result: Vec<Note3> = Vec::new();
    if notes4.is_empty() {
        return result;
    }
    result.push(notes4[0].clone());
    if start >= end || end >= notes4.len() {
        return result;
    }
    let steps = end - start;
    let steps_per_intervaladd = (steps_per_intervale - steps_per_intervala) as f32 / steps as f32;
    for i in start..end {
        let t_start = notes4[i].timestamp;
        let t_end = notes4[i + 1].timestamp;
        let range = t_end - t_start;
        if range <= 0.0 {
            continue;
        }
        let steps_per_interval = (steps_per_intervala as f32 + steps_per_intervaladd) as i32;
        for j in 1..=steps_per_interval {
            let t = t_start + range * j as f32 / (steps_per_interval + 1) as f32;
            let normalized = (t - t_start) * (factorpi * M_PI as f32) / range;
            let sine_value = normalized.sin();
            let mut pitch = 0;
            if curve == 1 {
                pitch = (sine_value * noteanz * (t / notes4[end].timestamp) / notes4[end].timestamp)
                    as i32
                    + notes4[i].pitch;
            } else if curve == 2 {
                pitch = (sine_value
                    * noteanz
                    * (1.0 - (t / notes4[end].timestamp) / notes4[end].timestamp))
                    as i32
                    + notes4[i].pitch;
            } else if curve == 3 {
                pitch = (sine_value * noteanz) as i32 + notes4[i].pitch;
            }
            let mut new_note = notes4[i].clone();
            new_note.timestamp = t;
            new_note.pitch = pitch;
            result.push(new_note);
        }
        result.push(notes4[i + 1].clone());
    }
    result
}

/// Divisionsschutz: 0/NaN/negativ -> 1.0 (verhindert NaN-Timestamps).
fn safe_divisor(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        1.0
    }
}

/// Gültiger Index in einen Puffer, `None` bei negativ/zu groß.
fn checked_index(i: i32, len: usize) -> Option<usize> {
    if i < 0 {
        None
    } else {
        let i = i as usize;
        (i < len).then_some(i)
    }
}

fn record_key_action(kc: Keycode, alleseins: bool, scaletoa: bool) -> RecordAction {
    if kc == Keycode::Num0 {
        return RecordAction::Stop;
    }
    if kc == Keycode::A {
        return RecordAction::ToggleAlleseins;
    }
    let digit = digit_value(kc).filter(|d| (1..=9).contains(d));
    if alleseins && !scaletoa {
        if digit.is_some() {
            return RecordAction::NoteFixedOne;
        }
    } else if let Some(d) = digit {
        return RecordAction::NoteDigit(d as i32);
    }
    if kc == Keycode::PageUp {
        return RecordAction::PageUp(true);
    }
    if kc == Keycode::PageDown {
        return RecordAction::PageUp(false);
    }
    if kc == Keycode::S {
        return RecordAction::SelectKk;
    }
    if kc == Keycode::D {
        return RecordAction::SelectKr;
    }
    if kc == Keycode::W {
        return RecordAction::SelectNone;
    }
    if kc == Keycode::Y {
        return RecordAction::Schritt3;
    }
    if kc == Keycode::Q {
        return RecordAction::NumFactor(2);
    }
    if kc == Keycode::E {
        return RecordAction::NumFactor(4);
    }
    let rstep = match kc {
        Keycode::X => Some(7.0),
        Keycode::C => Some(15.0),
        Keycode::V => Some(2.0),
        Keycode::B => Some(5.0),
        Keycode::N => Some(11.0),
        Keycode::M => Some(23.0),
        _ => None,
    };
    if let Some(r) = rstep {
        return RecordAction::RStep(r);
    }
    if let Some(f) = fkey_value(kc) {
        return RecordAction::NoteFKey(f as i32);
    }
    RecordAction::None
}

/// Liest (index, note, instrument) je Line aus dem Renoise-PatternClipboard-XML.
/// Nur die erste äußere Column zählt (wie tinyxml2 im Original); self-closing
/// `<Line index="N"/>` sind leere Zeilen und liefern keine Noten.
fn parse_line_notes(xml: &str) -> Vec<(i32, String, String)> {
    use quick_xml::events::Event as XmlEvent;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut first_outer_seen = false;
    let mut active_outer_depth: Option<usize> = None;
    let mut line: Option<(i32, Option<String>, Option<String>)> = None;
    let mut text_target: Option<u8> = None;
    let mut text = String::new();

    let is_note_lines = |path: &[String]| {
        path.len() >= 3
            && path[path.len() - 3] == "Column"
            && path[path.len() - 2] == "Column"
            && path[path.len() - 1] == "Lines"
    };

    loop {
        match reader.read_event() {
            Ok(XmlEvent::Eof) | Err(_) => break,
            Ok(XmlEvent::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "Column" && path.last().map(String::as_str) == Some("Columns") {
                    if !first_outer_seen {
                        first_outer_seen = true;
                        active_outer_depth = Some(path.len() + 1);
                    }
                }
                if name == "Line" && is_note_lines(&path) && active_outer_depth.is_some() {
                    let idx = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"index")
                        .and_then(|a| std::str::from_utf8(&a.value).ok()?.trim().parse().ok())
                        .unwrap_or(0);
                    line = Some((idx, None, None));
                }
                if line.is_some() {
                    if name == "Note" {
                        text_target = Some(0);
                        text.clear();
                    } else if name == "Instrument" {
                        text_target = Some(1);
                        text.clear();
                    }
                }
                path.push(name);
            }
            Ok(XmlEvent::Text(t)) => {
                if text_target.is_some() {
                    text.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Ok(XmlEvent::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "Note" if text_target == Some(0) => {
                        if let Some((_, note, _)) = line.as_mut() {
                            *note = Some(text.trim().to_string());
                        }
                        text_target = None;
                    }
                    "Instrument" if text_target == Some(1) => {
                        if let Some((_, _, instr)) = line.as_mut() {
                            *instr = Some(text.trim().to_string());
                        }
                        text_target = None;
                    }
                    "Line" => {
                        if let Some((idx, note, instr)) = line.take() {
                            if let (Some(n), Some(i)) = (note, instr) {
                                if !n.is_empty() {
                                    out.push((idx, n, i));
                                }
                            }
                        }
                    }
                    _ => {}
                }
                path.pop();
                if active_outer_depth.is_some_and(|d| path.len() < d) {
                    active_outer_depth = None;
                }
            }
            _ => {}
        }
    }
    out
}

/// clipboard lesen via xclip
fn get_clipboard_content() -> String {
    let out = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}

/// xclip/xsel-Fallback in die Zwischenablage kopieren
fn copy_to_clipboard(temp_file: &str) -> bool {
    let ok = std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-i", temp_file])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        thread::sleep(Duration::from_millis(100));
        let _ = std::process::Command::new("xclip")
            .args(["-selection", "primary", "-i", temp_file])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    } else {
        let cmd = format!("xsel --clipboard --input < {}", temp_file);
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    ok
}

struct PatternRecorder {
    audio_playing: Arc<AtomicBool>,
    freq_bits: Arc<AtomicU32>,
    _audio: Option<sdl2::audio::AudioDevice<AudioGen>>,
    frequency: f32,
    target_frequency: f32,
    current_frequency: f32,
    glide_rate: f32,
    editing_glide: bool,
    current_pattern: i32,
    current_note_index: i32,

    pnotes: [Vec<Note3>; 8],
    notes55: [Vec<Note3>; 8],
    pdauer: Vec<Vec<f32>>,
    pselect: Vec<Vec<i32>>,

    _shm: Option<Shmem>,
    sd: *mut SharedData,

    zeitfakto: f32,
    start_time: f32,
    end_time: f32,
    start_index: i32,
    end_index: i32,
    start_indexnum: f32,
    end_indexnum: f32,
    indexnum_laenge: f32,
    schrittweitanz: i32,
    plsl: i32,
    loops: i32,
    num_patterns: i32,
    patterns: i32,
    toene: i32,
    factor: f32,
    shared_memory_update_delay: i32,

    total_di: bool,
    kk: bool,
    kl: bool,
    kr: bool,
    firstrec: bool,
    firstrec0: bool,
    pgup: bool,
    vmidi: bool,

    is_playing: [Arc<AtomicBool>; 8],
    data_mutex: Mutex<()>,
    playing_mutex: Mutex<()>,

    notes: Vec<Note>,
    snotes: Vec<Note>,
    ssnotes: Vec<Note>,
    notesr: Vec<Note>,
    notes2: Vec<Note2>,
    notes22: Vec<Note2>,
    notes3: Vec<Note3>,
    notes4: Vec<Note3>,
    notes4s: Vec<Note3>,
    notes5: Vec<Note3>,
    notes4p: Vec<Note3>,
    new_notes: Vec<Note3>,
    notestoscale: Vec<Notescale>,
    mod1: Vec<Modu>,
    veforec: Vec<VecForRec>,
    playfofohi1: Vec<PlayFofoHi>,
    festerfester: Vec<Festerfester>,
    scaled_notes: Vec<Note3>,
    scaled_notes2: Vec<Note3>,

    svalue: String,
    current_playing_note: i32,
    renoise_playing: bool,
    renoise_recording: bool,
    renoise_play_and_record_enabled: bool,
    timestampvalue: f32,
}

impl PatternRecorder {
    fn new(sdl: &Sdl) -> Result<PatternRecorder, String> {
        println!("Removing existing shared memory...");
        println!("Creating shared memory...");
        let mut shm = create_zeitfakto_shm()?;
        println!("Truncating shared memory...");
        println!("Mapping region...");
        let base = unsafe { shm.as_slice_mut().as_mut_ptr() };
        unsafe {
            base.write_bytes(0, HH_SIZE);
        }
        let sd = base as *mut SharedData;
        unsafe {
            (*sd).zeitfakto = 1.0;
            (*sd).variable2 = 1.0;
            (*sd).variable3 = 1.0;
            (*sd).dauer = 4.0;
            (*sd).bpm = 120.0;
        }

        println!("Initializing audio...");
        let audio_subsys = sdl.audio()?;
        let audio_playing = Arc::new(AtomicBool::new(false));
        let freq_bits = Arc::new(AtomicU32::new(440.0f32.to_bits()));
        let desired = AudioSpecDesired {
            freq: Some(SAMPLE_RATE),
            channels: Some(1),
            samples: Some(2048),
        };
        let fcheck = Arc::clone(&freq_bits);
        let pcheck = Arc::clone(&audio_playing);
        let audio_device = audio_subsys
            .open_playback(
                None,
                &desired,
                move |_spec| AudioGen {
                    playing: pcheck,
                    freq_bits: fcheck,
                    phase: 0.0,
                },
            )
            .map_err(|e| eprintln!("Failed to open audio device: {e}"))
            .ok();
        if let Some(dev) = audio_device.as_ref() {
            dev.resume();
        }

        let mut pdauer: Vec<Vec<f32>> = Vec::new();
        for _ in 0..8 {
            pdauer.push(vec![0.0]);
        }
        for _ in 0..8 {
            pdauer.push(vec![0.0]);
        }
        let mut playfofohi1: Vec<PlayFofoHi> = Vec::new();
        for _ in 0..9 {
            playfofohi1.push(PlayFofoHi {
                current_indices: 0,
                start_times: 0.0,
            });
        }

        println!("PatternRecorder initialized successfully");
        Ok(PatternRecorder {
            audio_playing,
            freq_bits,
            _audio: audio_device,
            frequency: 440.0,
            target_frequency: 440.0,
            current_frequency: 440.0,
            glide_rate: 0.001,
            editing_glide: false,
            current_pattern: 0,
            current_note_index: 0,
            pnotes: std::array::from_fn(|_| Vec::new()),
            notes55: std::array::from_fn(|_| Vec::new()),
            pdauer,
            pselect: Vec::new(),
            _shm: Some(shm),
            sd,
            zeitfakto: 1.0,
            start_time: 0.0,
            end_time: 0.0,
            start_index: 0,
            end_index: 0,
            start_indexnum: 0.0,
            end_indexnum: 0.0,
            indexnum_laenge: 0.0,
            schrittweitanz: 0,
            plsl: 0,
            loops: 1,
            num_patterns: 8,
            patterns: 0,
            toene: 0,
            factor: 1.0,
            shared_memory_update_delay: 0,
            total_di: false,
            kk: true,
            kl: false,
            kr: false,
            firstrec: false,
            firstrec0: false,
            pgup: true,
            vmidi: false,
            is_playing: std::array::from_fn(|_| Arc::new(AtomicBool::new(false))),
            data_mutex: Mutex::new(()),
            playing_mutex: Mutex::new(()),
            notes: Vec::new(),
            snotes: Vec::new(),
            ssnotes: Vec::new(),
            notesr: Vec::new(),
            notes2: Vec::new(),
            notes22: Vec::new(),
            notes3: Vec::new(),
            notes4: Vec::new(),
            notes4s: Vec::new(),
            notes5: Vec::new(),
            notes4p: Vec::new(),
            new_notes: Vec::new(),
            notestoscale: Vec::new(),
            mod1: Vec::new(),
            veforec: Vec::new(),
            playfofohi1,
            festerfester: Vec::new(),
            scaled_notes: Vec::new(),
            scaled_notes2: Vec::new(),
            svalue: String::new(),
            current_playing_note: -1,
            renoise_playing: false,
            renoise_recording: false,
            renoise_play_and_record_enabled: false,
            timestampvalue: 0.0,
        })
    }

    fn sd(&self) -> &SharedData {
        unsafe { &*self.sd }
    }
    fn sd_mut(&self) -> &mut SharedData {
        unsafe { &mut *self.sd }
    }

    fn set_frequency(&self, f: f32) {
        self.freq_bits.store(f.to_bits(), Ordering::Relaxed);
    }

    // ------------- Shared-Memory-Updater -------------
    fn set_shared_memory_update_delay(&mut self, ms: i32) {
        self.shared_memory_update_delay = ms;
        println!("Shared Memory Update Delay gesetzt auf: {} ms", ms);
    }
    fn sdelay(&self) {
        if self.shared_memory_update_delay > 0 {
            thread::sleep(Duration::from_millis(self.shared_memory_update_delay as u64));
        }
    }
    fn update_zeitfakto(&mut self, v: f32) {
        self.sdelay();
        self.sd_mut().zeitfakto = v;
    }
    fn update_variable2(&mut self, v: f32) {
        self.sdelay();
        self.sd_mut().variable2 = v;
    }
    fn update_variable3(&mut self, v: f32) {
        self.sdelay();
        self.sd_mut().variable3 = v;
    }
    fn updatedauer(&mut self, v: f32) {
        self.sdelay();
        self.sd_mut().dauer = v;
    }
    fn updatebpm(&mut self, v: f32) {
        self.sdelay();
        self.sd_mut().bpm = v;
    }
    fn updatep1(&mut self, v: bool) {
        self.sdelay();
        self.sd_mut().p1 = v;
    }
    fn updatep2(&mut self, v: bool) {
        self.sdelay();
        self.sd_mut().p2 = v;
    }
    fn updatep3(&mut self, v: bool) {
        self.sdelay();
        self.sd_mut().p3 = v;
    }
    fn zeitfaktor(&mut self) {
        self.sdelay();
        self.sd_mut().zeitfakto = (60.0 / self.sd().bpm) * self.sd().dauer;
        println!("{}", self.sd().zeitfakto);
    }

    fn eingabe(&self, pump: &mut EventPump) -> String {
        let _ = pump;
        let mut input = String::new();
        loop {
            if let Some(event) = poll_event_compat() {
                if let Event::KeyDown {
                    keycode: Some(kc),
                    ..
                } = event
                {
                    if kc == Keycode::Return {
                        return input;
                    }
                    if let Some(d) = digit_value(kc) {
                        append_input_token(&mut input, char::from_digit(d, 10).unwrap_or('0'));
                        println!("input: {}", input);
                    } else if kc == Keycode::KpDecimal {
                        append_input_token(&mut input, '.');
                        println!("input: {}", input);
                    }
                }
            }
        }
    }

    // ------------- Audio -------------
    fn play_pattern_as_sound(&mut self, pattern_index: i32) {
        if pattern_index >= 0 && (pattern_index as usize) < self.pnotes.len() {
            let pattern_len = self.pnotes[pattern_index as usize].len();
            if pattern_len == 0 {
                println!("Pattern {} is empty!", pattern_index + 1);
                return;
            }
            println!(
                "Playing pattern {} with {} notes",
                pattern_index + 1,
                pattern_len
            );
            println!("BPM: {}", self.sd().bpm);
            thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
            self.audio_playing.store(true, Ordering::Relaxed);
            let mut last_freq = 440.0f32;
            let mut play_start_time = ticks_f();
            let mut note_index = 0usize;
            let mut current_loop = 0i32;
            while current_loop < self.loops {
                if note_index >= pattern_len {
                    play_start_time += loop_start_advance(
                        &self.pnotes[pattern_index as usize],
                        self.sd().zeitfakto,
                    );
                    note_index = 0;
                    current_loop += 1;
                    if current_loop >= self.loops {
                        break;
                    }
                    continue;
                }
                let pattern = &self.pnotes[pattern_index as usize];
                let play_time_old = play_time_old(pattern, note_index, self.sd().zeitfakto);
                let play_time = pattern[note_index].timestamp * self.sd().zeitfakto;
                let current_time = ticks_f();
                let note = pattern[note_index].clone();
                let p2 = note.p2;
                let p = (current_time - (play_start_time + play_time_old)) / (play_time - play_time_old);
                let q = (note_index + 1) as f32 / pattern_len as f32;
                let mut ps = String::new();
                self.frequency = ((6.28 * ((p / 4.0) + (q / 4.0)) + 1.0).sin() / 2.0)
                    * self.target_frequency
                    * (1.0 + p2);
                let mut i = 0.0f32;
                while i <= 1.0f32 {
                    if i <= p && i <= q {
                        ps += "=";
                    } else if i <= p {
                        ps += "-";
                    } else if i <= q {
                        ps += "+";
                    } else {
                        ps += " ";
                    }
                    i += 0.1;
                }
                print!("  [{}]\r", ps);
                let _ = std::io::stdout().flush();
                let _frozen_p_ge = p >= 0.42;
                if p >= 1.0 {
                    self.target_frequency =
                        440.0 * 2.0f32.powf((note.pitch - self.toene - 69) as f32 / 12.0);
                    if note.glide && note_index > 0 {
                        self.current_frequency = last_freq;
                        println!(
                            "Playing note with glide: pitch={}, from freq={}Hz to {}Hz",
                            note.pitch - self.toene,
                            last_freq,
                            self.target_frequency
                        );
                        let glide_duration = note.timestamp * self.sd().zeitfakto;
                        let glide_start = Instant::now();
                        while glide_start.elapsed().as_secs_f32() < glide_duration {
                            let progress = glide_start.elapsed().as_secs_f32() / glide_duration;
                            self.current_frequency =
                                last_freq + (self.target_frequency - last_freq) * progress;
                            self.frequency = self.current_frequency;
                            self.set_frequency(self.frequency);
                        }
                    } else {
                        self.current_frequency = self.target_frequency;
                        self.frequency = self.current_frequency;
                        println!(
                            "Playing note: pitch={}, freq={}Hz",
                            note.pitch - self.toene,
                            self.target_frequency
                        );
                        println!("  [          ]\r");
                    }
                    self.set_frequency(self.frequency);
                    last_freq = self.target_frequency;
                    note_index += 1;
                }
            }
            self.audio_playing.store(false, Ordering::Relaxed);
            self.current_frequency = 440.0;
        } else {
            println!("Invalid pattern index: {}", pattern_index);
        }
    }

    // ------------- Noten-----------------
    fn play(&mut self) {
        if self.notes3.is_empty() {
            println!("No notes recorded.");
            return;
        }
        println!("Playing pattern at {} BPM...", self.sd().bpm);
        thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
        let mut play3_start_time1 = ticks_f();
        let mut v1 = 0usize;
        let mut l1 = 0i32;
        while l1 < self.loops {
            let play_time1 = self.notes3[v1].timestamp * self.sd().zeitfakto;
            let current_time = ticks_f();
            if l1 < self.loops {
                if v1 < self.notes3.len() {
                    if current_time - play3_start_time1 >= play_time1 {
                        if v1 == self.notes3.len() - 1 {
                            play3_start_time1 += self.notes3[v1].timestamp * self.sd().zeitfakto;
                            l1 += 1;
                            v1 = 0;
                            if l1 == self.loops {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.notes3[v1].instrument + self.sd().variable2 as i32,
                                self.notes3[v1].pitch + self.sd().variable3 as i32,
                            );
                            v1 += 1;
                            println!(
                                "{:<10.3}{:<20}{:<10}{}",
                                current_time - play3_start_time1,
                                play_time1,
                                v1,
                                l1
                            );
                        }
                    }
                }
            }
        }
    }

    fn play2(&mut self) {
        if self.notes4.is_empty() {
            println!("No notes recorded.");
            return;
        }
        println!("Playing pattern at {} BPM...", self.sd().bpm);
        thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
        let mut play3_start_time1 = ticks_f();
        let mut v1 = 0usize;
        let mut l1 = 0i32;
        while l1 < self.loops {
            let play_time1 = self.notes4[v1].timestamp * self.sd().zeitfakto;
            let current_time = ticks_f();
            if l1 < self.loops {
                if v1 < self.notes4.len() {
                    if current_time - play3_start_time1 >= play_time1 {
                        if v1 == self.notes4.len() - 1 {
                            play3_start_time1 += self.notes4[v1].timestamp * self.sd().zeitfakto;
                            l1 += 1;
                            v1 = 0;
                            if l1 == self.loops {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.notes4[v1].instrument + self.sd().variable2 as i32,
                                self.notes4[v1].pitch + self.sd().variable3 as i32,
                            );
                            println!(
                                "{} {} {} {} {}",
                                l1,
                                v1,
                                self.notes4[v1].timestamp,
                                self.notes4[v1].instrument,
                                self.notes4[v1].pitch
                            );
                            v1 += 1;
                        }
                    }
                }
            }
        }
    }

    fn play2add(&mut self, i: usize) {
        if self.notes55[i].is_empty() {
            println!("No notes recorded.");
            return;
        }
        println!("Playing pattern at {} BPM...", self.sd().bpm);
        thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
        let mut play3_start_time1 = ticks_f();
        let mut v1 = 0usize;
        let mut l1 = 0i32;
        while l1 < self.loops {
            let play_time1 = self.notes55[i][v1].timestamp * self.sd().zeitfakto;
            let current_time = ticks_f();
            if l1 < self.loops {
                if v1 < self.notes55[i].len() {
                    if current_time - play3_start_time1 >= play_time1 {
                        if v1 == self.notes55[i].len() - 1 {
                            play3_start_time1 +=
                                self.notes55[i][v1].timestamp * self.sd().zeitfakto;
                            l1 += 1;
                            v1 = 0;
                            if l1 == self.loops {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.notes55[i][v1].instrument + self.sd().variable2 as i32,
                                self.notes55[i][v1].pitch + self.sd().variable3 as i32,
                            );
                            println!(
                                "{} {} {} {} {}",
                                l1,
                                v1,
                                self.notes55[i][v1].timestamp,
                                self.notes55[i][v1].instrument,
                                self.notes55[i][v1].pitch
                            );
                            v1 += 1;
                        }
                    }
                }
            }
        }
    }

    fn play3(&mut self) {
        if self.notes4.is_empty() {
            println!("No notes recorded.");
            return;
        }
        if self.mod1.is_empty() {
            println!("mod1 ist leer (M drücken).");
            return;
        }
        println!("Playing pattern at {} BPM...", self.sd().bpm);
        thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
        let mut play3_start_time1 = ticks_f();
        let mut play3_start_time2 = play3_start_time1;
        let mut v1 = 0usize;
        let mut v2 = 0usize;
        let mut l1 = 0i32;
        let mut l2 = 0i32;
        while (l1 < self.loops) && (l2 < self.loops) {
            let play_time1 = self.notes4[v1].timestamp * self.sd().zeitfakto;
            let play_time2 = self.mod1[v2].time * self.sd().zeitfakto;
            let current_time = ticks_f();
            if l1 < self.loops {
                if v1 < self.notes4.len() {
                    if current_time - play3_start_time1 >= play_time1 {
                        if v1 == self.notes4.len() - 1 {
                            play3_start_time1 += self.notes4[v1].timestamp * self.sd().zeitfakto;
                            l1 += 1;
                            v1 = 0;
                            if l1 == self.loops {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.notes4[v1].instrument + self.sd().variable2 as i32,
                                self.notes4[v1].pitch + self.sd().variable3 as i32,
                            );
                            v1 += 1;
                            println!(
                                "{} {} {} {} {}",
                                l1,
                                v1,
                                self.notes4[v1].timestamp,
                                self.notes4[v1].instrument,
                                self.notes4[v1].pitch
                            );
                        }
                    }
                }
            }
            if l2 < self.loops {
                if v2 < self.mod1.len() {
                    if current_time - play3_start_time2 >= play_time2 {
                        if v2 == self.mod1.len() - 1 {
                            play3_start_time2 += self.mod1[v2].time * self.sd().zeitfakto;
                            l2 += 1;
                            v2 = 0;
                            if l1 == self.loops {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.mod1[v2].instrument + self.sd().variable2 as i32,
                                self.mod1[v2].pitch + self.sd().variable3 as i32,
                            );
                            v2 += 1;
                            println!(
                                "{} {} {} {} {}",
                                l2,
                                v2,
                                self.mod1[v2].time,
                                self.mod1[v2].instrument,
                                self.mod1[v2].pitch
                            );
                        }
                    }
                }
            }
        }
    }

    fn play34(&mut self, num: usize) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        if self.pnotes[num].is_empty() {
            println!("No notes recorded.");
            return;
        }
        if self.mod1.is_empty() {
            println!("mod1 ist leer (M drücken).");
            return;
        }
        println!("Playing pattern at {} BPM...", self.sd().bpm);
        thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
        let mut play3_start_time1 = ticks_f();
        let mut play3_start_time2 = play3_start_time1;
        let mut v1 = 0usize;
        let mut v2 = 0usize;
        let mut l1 = 0i32;
        let mut l2 = 0i32;
        while (l1 < self.loops) && (l2 < self.loops) {
            let play_time1 = self.pnotes[num][v1].timestamp
                * self.sd().zeitfakto
                / self.sd().dauer
                * self.pdauer[num][0];
            let play_time2 =
                self.mod1[v2].time * self.sd().zeitfakto / self.sd().dauer * self.pdauer[num][0];
            let current_time = ticks_f();
            if l1 < self.loops {
                if v1 < self.pnotes[num].len() {
                    if current_time - play3_start_time1 >= play_time1 {
                        if v1 == self.pnotes[num].len() - 1 {
                            play3_start_time1 += self.pnotes[num][v1].timestamp
                                * self.sd().zeitfakto
                                / self.sd().dauer
                                * self.pdauer[num][0];
                            l1 += 1;
                            v1 = 0;
                            if l1 == self.loops - 1 {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.pnotes[num][v1].instrument + self.sd().variable2 as i32,
                                self.pnotes[num][v1].pitch + self.sd().variable3 as i32,
                            );
                            v1 += 1;
                            println!(
                                "{} {} {} {} {}",
                                l1,
                                v1,
                                self.pnotes[num][v1].timestamp,
                                self.pnotes[num][v1].instrument,
                                self.pnotes[num][v1].pitch
                            );
                        }
                    }
                }
            }
            if l2 < self.loops {
                if v2 < self.mod1.len() {
                    if current_time - play3_start_time2 >= play_time2 {
                        if v2 == self.mod1.len() - 1 {
                            play3_start_time2 +=
                                self.mod1[v2].time * self.sd().zeitfakto / self.sd().dauer
                                    * self.pdauer[num][0];
                            l2 += 1;
                            v2 = 0;
                            if l1 == self.loops - 1 {
                                break;
                            }
                        } else {
                            osc::send_note_to_renoise(
                                self.mod1[v2].instrument + self.sd().variable2 as i32,
                                self.mod1[v2].pitch + self.sd().variable3 as i32,
                            );
                            v2 += 1;
                            println!(
                                "{} {} {} {} {}",
                                l2,
                                v2,
                                self.mod1[v2].time,
                                self.mod1[v2].instrument,
                                self.mod1[v2].pitch
                            );
                        }
                    }
                }
            }
        }
    }

    fn play4(&mut self, num: usize) {
        let _g = self.playing_mutex.lock();
        if num >= self.num_patterns as usize || self.pnotes[num].is_empty() {
            println!("Invalid pattern number or empty pattern: {}", num);
            return;
        }
        let isp = Arc::clone(&self.is_playing[num]);
        if isp.load(Ordering::Relaxed) {
            isp.store(false, Ordering::Relaxed);
        } else {
            isp.store(true, Ordering::Relaxed);
            let notes = self.pnotes[num].clone();
            let pd = self.pdauer[num][0];
            let plsl = self.plsl.max(0) as u64;
            let loops = self.loops;
            let sd_ptr = SdPtr(self.sd);
            self.play_pattern_thread(num, notes, pd, plsl, loops, sd_ptr, isp);
        }
    }

    fn play_pattern_thread(
        &self,
        num: usize,
        notes: Vec<Note3>,
        pd: f32,
        plsl: u64,
        loops: i32,
        sd_ptr: SdPtr,
        isp: Arc<AtomicBool>,
    ) {
        if notes.is_empty() {
            println!("No notes recorded for pattern {}.", num);
            return;
        }
        thread::spawn(move || {
            let sd_ptr = sd_ptr.get();
            println!("Playing pattern {} at {} BPM...", num, unsafe { (*sd_ptr).bpm });
            thread::sleep(Duration::from_millis(plsl));
            let start = Instant::now();
            let mut v1 = 0usize;
            let mut l1 = 0i32;
            while l1 < loops && isp.load(Ordering::Relaxed) {
                let zf = unsafe { (*sd_ptr).zeitfakto };
                let dauer = unsafe { (*sd_ptr).dauer };
                let play_time1 = thread_note_time(notes[v1].timestamp, zf, dauer, pd);
                let cycle_offset = loop_cycle_offset(&notes, l1, zf, dauer, pd);
                if start.elapsed().as_secs_f32() >= cycle_offset + play_time1 {
                    if v1 == notes.len() - 1 {
                        l1 += 1;
                        v1 = 0;
                        if l1 == loops {
                            break;
                        }
                    } else {
                        osc::send_note_to_renoise(
                            notes[v1].instrument + unsafe { (*sd_ptr).variable2 } as i32,
                            notes[v1].pitch + unsafe { (*sd_ptr).variable3 } as i32,
                        );
                        println!(
                            "Pattern {}: {} {} {} {} {}",
                            num, l1, v1, notes[v1].timestamp, notes[v1].instrument, notes[v1].pitch
                        );
                        v1 += 1;
                    }
                }
                thread::yield_now();
                thread::sleep(Duration::from_millis(1));
            }
            isp.store(false, Ordering::Relaxed);
        });
    }

    fn play_for_for_hi(&mut self, num: usize, pump: &mut EventPump) {
        let _ = pump;
        // Port von C++ playforforhi(num): pollt Events für 1-8, spielt 8 Patterns parallel.
        if self.pnotes[num].is_empty() {
            println!("No notes recorded.");
            return;
        }
        thread::sleep(Duration::from_millis(self.plsl.max(0) as u64));
        let mut l1 = 0i32;
        while l1 < self.loops {
            loop {
                match poll_event_compat() {
                    Some(Event::KeyDown {
                        keycode: Some(kc),
                        ..
                    }) => {
                        let v = kc.into_i32();
                        let base = Keycode::Num0.into_i32();
                        let indexx = if v >= base + 1 && v <= base + 8 {
                            (v - base) as usize
                        } else {
                            0
                        };
                        self.playfofohi1[indexx].current_indices = 0;
                        self.playfofohi1[indexx].start_times = ticks_f();
                        println!(
                            "Taste: {} {}",
                            indexx,
                            self.playfofohi1[indexx].start_times
                        );
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            for index in 0..8usize {
                if self.pnotes[index].is_empty() {
                    continue;
                }
                let note_time = self.pnotes[index]
                    [self.playfofohi1[index].current_indices as usize]
                    .timestamp
                    * self.pdauer[index][0];
                if ticks_f() - self.playfofohi1[index].start_times >= note_time {
                    if self.playfofohi1[index].current_indices as usize
                        == self.pnotes[index].len() - 1
                    {
                        self.playfofohi1[index].current_indices = 0;
                        l1 += 1;
                    } else {
                        osc::send_note_to_renoise(
                            self.pnotes[index]
                                [self.playfofohi1[index].current_indices as usize]
                                .instrument
                                + self.sd().variable2 as i32,
                            60,
                        );
                        println!(
                            "{} {}",
                            self.playfofohi1[index].current_indices,
                            self.pnotes[index][self.playfofohi1[index].current_indices as usize]
                                .timestamp
                        );
                        self.playfofohi1[index].current_indices += 1;
                    }
                }
            }
        }
    }

    // ------------- Modifikationen -------------
    fn remove_duplicates(&mut self, index: usize) {
        // Jetzt Port: notes4 Verweise zwei nebeneinander stehende gleiche timestamps
        let mut i = 0usize;
        while i + 1 < self.notes4.len() {
            if self.notes4[i].timestamp == self.notes4[i + 1].timestamp {
                self.notes4.remove(i + 1);
            } else {
                i += 1;
            }
        }
        let _ = index;
    }

    fn removeduplicates_orig(&mut self, notes4: &mut Vec<Note3>) {
        // C++: for i in 0..size-1: erase wenn gleich. (wirft Ausnahme bei NaN-Vergleichen nicht)
        let mut i = 0usize;
        while i + 1 < notes4.len() {
            if notes4[i].timestamp == notes4[i + 1].timestamp {
                notes4.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    fn anfend_index(&mut self, pump: &mut EventPump) {
        if self.notes4.is_empty() {
            println!("notes4 ist leer");
            self.start_index = 0;
            self.end_index = 0;
            return;
        }
        self.svalue = String::new();
        println!("Geben Sie den Anfangsindex (0 bis {}) ein: ", self.notes4.len() - 1);
        self.start_index = self.eingabe(pump).parse().unwrap_or(0);
        self.svalue = String::new();
        println!("Geben Sie den Endindex (0 bis {}) ein: ", self.notes4.len());
        self.end_index = self.eingabe(pump).parse().unwrap_or(0);
        if self.end_index == self.notes4.len() as i32 {
            self.end_time = 1.0;
        }
    }

    fn total_durationbreak(&self, start_time: f32, end_time: f32) {
        let total_duration = end_time - start_time;
        println!(
            "startTime, endTime, totalDuration, totalDurationbreak {:.3}, {:.3}, {:.3}, {}",
            start_time,
            end_time,
            total_duration,
            rhythm_fraction(total_duration as f64)
        );
    }

    fn dis_pl_notes(&self) {
        println!("notes3");
        for i in 0..self.notes3.len() {
            let next = if i + 1 < self.notes3.len() {
                self.notes3[i + 1].clone()
            } else {
                Note3::default()
            };
            let n = &self.notes3[i];
            println!(
                "{:<10}{:<10}{:<10.3}, {:.3}, {}, {}, {}",
                i,
                rhythm_fraction((next.timestamp - n.timestamp) as f64),
                n.timestamp * self.sd().zeitfakto,
                next.timestamp * self.sd().zeitfakto - n.timestamp * self.sd().zeitfakto,
                n.instrument,
                n.pitch,
                n.p2
            );
        }
        println!("notes4");
        for i in 0..self.notes4.len() {
            let next = if i + 1 < self.notes4.len() {
                self.notes4[i + 1].clone()
            } else {
                Note3::default()
            };
            let n = &self.notes4[i];
            println!(
                "{:<10}{:<10}{:<10.3}, {:.3}, {}, {}, {}",
                i,
                rhythm_fraction((next.timestamp - n.timestamp) as f64),
                n.timestamp * self.sd().zeitfakto,
                next.timestamp * self.sd().zeitfakto - n.timestamp * self.sd().zeitfakto,
                n.instrument,
                n.pitch,
                n.p2
            );
        }
        println!("snotes");
        for i in 0..self.snotes.len() {
            let next = if i + 1 < self.snotes.len() {
                self.snotes[i + 1].clone()
            } else {
                Note::default()
            };
            let n = &self.snotes[i];
            println!(
                "{:<10}{:<10}{:<10.3}, {} {}",
                i,
                rhythm_fraction((next.timestamp - n.timestamp) as f64),
                n.timestamp,
                next.instrument,
                self.notes4.get(i).map(|n| n.pitch).unwrap_or(0)
            );
        }
    }

    fn notelenght(&mut self, pump: &mut EventPump) {
        if self.notes4.is_empty() {
            println!("notes4 ist leer");
            return;
        }
        for i in 0..self.notes4.len() {
            println!(
                "{}, {}, {}",
                self.notes4[i].timestamp, self.notes4[i].instrument, self.notes4[i].pitch
            );
            let value = self.eingabe(pump);
            let stamp: f32 = value.parse().unwrap_or(0.0);
            self.notes4[i].timestamp = stamp;
        }
        for i in 0..self.notes4.len() {
            println!("{}", self.notes4[i].timestamp);
        }
        let scldiv = self.notes4[self.notes4.len() - 1].timestamp;
        println!("{}", scldiv);
        for i in 0..self.notes4.len() {
            self.notes4[i].timestamp = self.notes4[i].timestamp / scldiv;
            println!("{}", self.notes4[i].timestamp);
        }
    }

    fn noteglide(&mut self, pump: &mut EventPump) {
        for i in 0..self.notes4.len() {
            println!(
                "{}, {}, {}, {}",
                self.notes4[i].timestamp,
                self.notes4[i].instrument,
                self.notes4[i].pitch,
                self.notes4[i].glide
            );
            let value = self.eingabe(pump);
            let stamp: f32 = value.parse().unwrap_or(0.0);
            self.notes4[i].glide = stamp != 0.0;
        }
        for i in 0..self.notes4.len() {
            println!(
                "{}, {}, {}, {}",
                self.notes4[i].timestamp,
                self.notes4[i].instrument,
                self.notes4[i].pitch,
                self.notes4[i].glide
            );
        }
    }

    fn noteinstr(&mut self, pump: &mut EventPump) {
        for i in 0..self.notes4.len() {
            println!(
                "{}, {}, {}",
                self.notes4[i].timestamp, self.notes4[i].instrument, self.notes4[i].pitch
            );
            let value = self.eingabe(pump);
            let stamp: f32 = value.parse().unwrap_or(0.0);
            self.notes4[i].instrument = stamp as i32;
        }
        for i in 0..self.notes4.len() {
            println!("{}", self.notes4[i].instrument);
        }
    }

    fn fdx(&mut self, pump: &mut EventPump) {
        println!("Geben Sie den Anfangsindex ein: ");
        let anfangsindex: i32 = self.eingabe(pump).parse().unwrap_or(0);
        println!("Geben Sie den Endindex ein: ");
        let endindex: i32 = self.eingabe(pump).parse().unwrap_or(0);
        println!("Geben Sie die Frequenzspanne in Halbtönen ein: ");
        let fd: f32 = self.eingabe(pump).parse().unwrap_or(0.0);
        println!("Geben Sie den Frequenzspannenteiler ein: ");
        let fdx: f32 = self.eingabe(pump).parse().unwrap_or(0.0);
        let df = fd / fdx;
        let mut df2 = 60.0;
        let mut ii = 1i32;
        let mut i = anfangsindex.max(0);
        let endindex = endindex.min(self.notes4.len() as i32);
        while i < endindex {
            let iu = i as usize;
            println!(
                "{}, {}, {}",
                self.notes4[iu].timestamp, self.notes4[iu].instrument, self.notes4[iu].pitch
            );
            if ii > fdx as i32 {
                ii = 1;
                df2 += df * ii as f32;
                df2 += 12.0;
            } else {
                df2 += df * ii as f32;
            }
            ii += 1;
            self.notes4[iu].pitch = df2 as i32;
            i += 1;
        }
        for i in 0..self.notes4.len() {
            println!(
                "{}, {}, {}",
                self.notes4[i].timestamp, self.notes4[i].instrument, self.notes4[i].pitch
            );
        }
    }

    // ------------- Skalierung -------------
    fn scale(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        self.notes2.clear();
        self.notes3.clear();
        self.notes4.clear();
        let total_duration = self.notes.last().unwrap().timestamp - self.notes[0].timestamp;
        let scaled_duration = 1.0f32;
        let scaling_factor = if total_duration != 0.0 {
            scaled_duration / total_duration
        } else {
            0.0
        };
        for note in &self.notes {
            self.notes2.push(Note2 {
                timestamp: (note.timestamp - self.notes[0].timestamp) * scaling_factor,
                instrument: note.instrument,
                pitch: note.pitch,
                glide: note.scl != 0,
                p2: note.p2,
                mschrittweitanz: note.mschrittweitanz,
                schrittweitanz: note.schrittweitanz,
            });
        }
        println!("Mapped Noten2:");
        for n in &self.notes2 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        let mut i = 0usize;
        while i < self.notes2.len() {
            if self.notes2[i].instrument == 1000 {
                let mut j = i;
                while j + 1 < self.notes2.len() {
                    let add = self.notes2[i].timestamp;
                    self.notes2[j + 1].timestamp += add;
                    j += 1;
                }
            }
            i += 1;
        }
        i = 0;
        while i < self.notes2.len() {
            if self.notes2[i].instrument == 1000 {
                self.notes2.remove(i);
            } else {
                i += 1;
            }
        }
        if self.notes2.is_empty() {
            println!("notes2 ist leer");
            return;
        }
        let scldivisor = safe_divisor(self.notes2[self.notes2.len() - 1].timestamp);
        println!("scldivisor: {}", scldivisor);
        for n in self.notes2.iter_mut() {
            n.timestamp = n.timestamp / scldivisor;
        }
        println!("Mapped normalized Noten2 with Breaks:");
        for n in &self.notes2 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        for i in 0..self.notes2.len() {
            if i + 1 >= self.notes2.len() {
                break;
            }
            let duration = self.notes2[i + 1].timestamp - self.notes2[i].timestamp;
            let note_value = self.notes2[i].pitch;
            for j in 0..note_value {
                let part_timestamp = self.notes2[i].timestamp + (duration / note_value as f32) * j as f32;
                let instrument = self.notes2[i].instrument;
                let pitch = 60 + j;
                self.notes3.push(Note3 {
                    timestamp: part_timestamp,
                    instrument,
                    pitch,
                    glide: false,
                    p2: self.notes2[i].p2,
                });
            }
        }
        self.notes3.push(Note3 {
            timestamp: 1.0,
            instrument: 0,
            pitch: 0,
            glide: false,
            p2: 0.0,
        });
        println!("Endgültige Noten3:");
        for n in &self.notes3 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        self.notes4 = self.notes3.clone();
    }

    fn scale_to_range(&mut self, new_notes: &mut Vec<Note3>, start_time: f32, end_time: f32) {
        if new_notes.is_empty() {
            return;
        }
        let total_duration = end_time - start_time;
        let new_duration = new_notes.last().unwrap().timestamp - new_notes[0].timestamp;
        let scaling_factor = if new_duration != 0.0 {
            total_duration / new_duration
        } else {
            0.0
        };
        println!(
            "{} {} {} {}",
            start_time,
            end_time,
            new_duration * total_duration,
            total_duration / new_duration
        );
        for n in new_notes.iter() {
            let scaled_timestamp =
                start_time + (n.timestamp - new_notes[0].timestamp) * scaling_factor;
            self.scaled_notes.push(Note3 {
                timestamp: scaled_timestamp,
                instrument: n.instrument,
                pitch: n.pitch,
                glide: false,
                p2: 0.0,
            });
        }
        println!("scaledNotes1:");
        for n in &self.scaled_notes {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        let mut i = 0usize;
        while i < self.scaled_notes.len() {
            if self.scaled_notes[i].instrument == 1000 {
                let mut j = i;
                while j + 1 < self.scaled_notes.len() {
                    let add = self.scaled_notes[i].timestamp;
                    self.scaled_notes[j + 1].timestamp += add;
                    j += 1;
                }
            }
            i += 1;
        }
        i = 0;
        while i < self.scaled_notes.len() {
            if self.scaled_notes[i].instrument == 1000 {
                self.scaled_notes.remove(i);
            } else {
                i += 1;
            }
        }
        println!("scaledNotes1:");
        for n in &self.scaled_notes {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        for i in 0..self.scaled_notes.len() {
            if i < self.scaled_notes.len() - 1 {
                let duration =
                    self.scaled_notes[i + 1].timestamp - self.scaled_notes[i].timestamp;
                let note_value = self.scaled_notes[i].pitch;
                for j in 0..note_value {
                    let part_timestamp =
                        self.scaled_notes[i].timestamp + (duration / note_value as f32) * j as f32;
                    let instrument = self.scaled_notes[i].instrument + i as i32;
                    let intermediate_pitch = 60 + j;
                    self.scaled_notes2.push(Note3 {
                        timestamp: part_timestamp,
                        instrument,
                        pitch: intermediate_pitch,
                        glide: false,
                        p2: 0.0,
                    });
                }
            }
        }
        println!("scaledNotes2:");
        for n in &self.scaled_notes2 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
    }

    fn scale_to_range_num(
        &mut self,
        new_notes: &mut Vec<Note3>,
        start_time: f32,
        end_time: f32,
        start_indexnum: f32,
        end_indexnum: f32,
        schrittweitanz: i32,
        firstrec: bool,
    ) {
        let total_duration = end_time - start_time;
        println!(
            "startTime, endTime, totalDuration {:.3}, {:.3}, {:.3}",
            start_time, end_time, total_duration
        );
        let mut naeschritts: Vec<Xn> = Vec::new();
        let schrittwa = 1.0 / start_indexnum;
        let schrittwe = 1.0 / end_indexnum;
        println!("schrittwa{:>15}schrittwe{:>15}schrittweitanz", "", "");
        println!("{}{:>15}{:>15}", schrittwa, schrittwe, schrittweitanz);
        println!();
        if !self.kk && !self.kl && !self.kr {
            if self.total_di {
                self.indexnum_laenge = total_duration - (1.0 / start_indexnum);
            } else {
                self.indexnum_laenge = 1.0 - (1.0 / start_indexnum);
            }
            let mut naeschrittskl: Vec<Xn> = Vec::new();
            self.kl = !self.kl;
            let mut naeschritt: f32;
            naeschrittskl.push(Xn { i: 0.0, xn: 0.0 });
            if self.kk || self.kl {
                naeschrittskl.push(Xn { i: 0.0, xn: schrittwa });
                naeschritt = schrittwa;
            } else {
                naeschritt = 0.0;
            }
            if schrittwa > schrittwe {
                let kd = self.indexnum_laenge - schrittwe * schrittweitanz as f32;
                let sumanz = schrittweitanz * (schrittweitanz + 1) / 2;
                let kdtanz = kd / sumanz as f32;
                println!("kd: {:>15}sumanz: {:>15}kdtanz: ", "", "");
                println!("{}{:>15}{:>15}", kd, sumanz, kdtanz);
                println!();
                for i in 1..=schrittweitanz {
                    naeschritt += schrittwe + (schrittweitanz - i + 1) as f32 * kdtanz;
                    naeschrittskl.push(Xn { i: i as f32, xn: naeschritt });
                }
            } else {
                let kd = self.indexnum_laenge - schrittwa * schrittweitanz as f32;
                let sumanz = schrittweitanz * (schrittweitanz + 1) / 2;
                let kdtanz = kd / sumanz as f32;
                println!("kd: {:>15}sumanz: {:>15}kdtanz: ", "", "");
                println!("{}{:>15}{:>15}", kd, sumanz, kdtanz);
                println!();
                for i in 1..=schrittweitanz {
                    naeschritt += schrittwa + i as f32 * kdtanz;
                    naeschrittskl.push(Xn { i: i as f32, xn: naeschritt });
                }
            }
            println!();
            if self.kl {
                naeschrittskl.pop();
            }
            self.kl = !self.kl;
            self.kr = !self.kr;
            if self.total_di {
                self.indexnum_laenge = total_duration - (1.0 / end_indexnum);
            } else {
                self.indexnum_laenge = 1.0 - (1.0 / end_indexnum);
            }
            let mut naeschrittskr: Vec<Xn> = Vec::new();
            naeschrittskr.push(Xn { i: 0.0, xn: 0.0 });
            if self.kk || self.kl {
                naeschrittskr.push(Xn { i: 0.0, xn: schrittwa });
                naeschritt = schrittwa;
            } else {
                naeschritt = 0.0;
            }
            if schrittwa > schrittwe {
                let kd = self.indexnum_laenge - schrittwe * schrittweitanz as f32;
                let sumanz = schrittweitanz * (schrittweitanz + 1) / 2;
                let kdtanz = kd / sumanz as f32;
                println!("kd: {:>15}sumanz: {:>15}kdtanz: ", "", "");
                println!("{}{:>15}{:>15}", kd, sumanz, kdtanz);
                println!();
                for i in 1..=schrittweitanz {
                    naeschritt += schrittwe + (schrittweitanz - i + 1) as f32 * kdtanz;
                    naeschrittskr.push(Xn { i: i as f32, xn: naeschritt });
                }
            } else {
                let kd = self.indexnum_laenge - schrittwa * schrittweitanz as f32;
                let sumanz = schrittweitanz * (schrittweitanz + 1) / 2;
                let kdtanz = kd / sumanz as f32;
                println!("kd: {:>15}sumanz: {:>15}kdtanz: ", "", "");
                println!("{}{:>15}{:>15}", kd, sumanz, kdtanz);
                println!();
                for i in 1..=schrittweitanz {
                    naeschritt += schrittwa + i as f32 * kdtanz;
                    naeschrittskr.push(Xn { i: i as f32, xn: naeschritt });
                }
            }
            println!();
            println!(
                "näschrittskl: {:>15}näschrittskr: {:>15}näschritts: {:>15}näschrittsdiff: ",
                "", "", ""
            );
            for i in 0..naeschrittskl.len() {
                let naeschrittdiff = naeschrittskl[i].xn - naeschrittskr[i].xn;
                if naeschrittdiff < 0.0 {
                    naeschritts.push(Xn {
                        i: i as f32,
                        xn: naeschrittskl[i].xn + (naeschrittdiff * (-1.0) / 2.0),
                    });
                } else {
                    naeschritts.push(Xn {
                        i: i as f32,
                        xn: naeschrittskl[i].xn - (naeschrittdiff * (-1.0) / 2.0),
                    });
                }
                println!(
                    "{:>15}{:>15}{:>15}{:>15}",
                    naeschrittskl[i].xn,
                    naeschrittskr[i].xn,
                    naeschritts[i].xn,
                    naeschrittdiff
                );
            }
            println!();
            self.kr = !self.kr;
        } else {
            let mut naeschritt: f32;
            naeschritts.push(Xn { i: 0.0, xn: 0.0 });
            if self.kk || self.kl {
                naeschritts.push(Xn { i: 0.0, xn: schrittwa });
                naeschritt = schrittwa;
            } else {
                naeschritt = 0.0;
            }
            if schrittwa > schrittwe {
                let kd = self.indexnum_laenge - schrittwe * schrittweitanz as f32;
                let sumanz = schrittweitanz * (schrittweitanz + 1) / 2;
                let kdtanz = kd / sumanz as f32;
                println!("kd: {:>15}sumanz: {:>15}kdtanz: ", "", "");
                println!("{}{:>15}{:>15}", kd, sumanz, kdtanz);
                println!();
                for i in 1..=schrittweitanz {
                    naeschritt += schrittwe + (schrittweitanz - i + 1) as f32 * kdtanz;
                    naeschritts.push(Xn { i: i as f32, xn: naeschritt });
                }
            } else {
                let kd = self.indexnum_laenge - schrittwa * schrittweitanz as f32;
                let sumanz = schrittweitanz * (schrittweitanz + 1) / 2;
                let kdtanz = kd / sumanz as f32;
                println!("kd: {:>15}sumanz: {:>15}kdtanz: ", "", "");
                println!("{}{:>15}{:>15}", kd, sumanz, kdtanz);
                println!();
                for i in 1..=schrittweitanz {
                    naeschritt += schrittwa + i as f32 * kdtanz;
                    naeschritts.push(Xn { i: i as f32, xn: naeschritt });
                }
            }
            println!();
            if self.kl {
                naeschritts.pop();
            }
        }
        let mut currentsteptimescl = 0.0f32;
        let n = naeschritts.len();
        for i in 0..n {
            let if_ = i as f32;
            if self.total_di {
                currentsteptimescl = naeschritts[i].xn + start_time;
            } else {
                currentsteptimescl = total_duration * naeschritts[i].xn + start_time;
            }
            if firstrec {
                let p2 = if_ / (n as f32 + 1.0);
                println!("p2: {}", p2);
                self.notesr.push(Note {
                    timestamp: currentsteptimescl,
                    instrument: self.notes.len() as i32,
                    pitch: 1,
                    scl: 0,
                    p2,
                    mschrittweitanz: false,
                    schrittweitanz: 0.0,
                });
            } else {
                new_notes.push(Note3 {
                    timestamp: currentsteptimescl,
                    instrument: 20,
                    pitch: 60 + if_ as i32,
                    glide: false,
                    p2: 0.0,
                });
            }
        }
        if firstrec {
            println!("Endgültige notesr:");
            for note in &self.notesr {
                println!(
                    "{}, {}, {}, {}",
                    note.timestamp, note.instrument, note.pitch, note.p2
                );
            }
        } else {
            println!("Endgültige newNotes:");
            for note in new_notes.iter() {
                println!("{}, {}, {}", note.timestamp, note.instrument, note.pitch);
            }
        }
        if !firstrec {
            let start_idx = self.start_index.clamp(0, self.notes4.len() as i32) as usize;
            let mut items: Vec<Note3> = new_notes.clone();
            self.notes4.splice(start_idx..start_idx, items.drain(..));
            println!("Endgültige Noten4:");
            for note in &self.notes4 {
                println!("{}, {}, {}", note.timestamp, note.instrument, note.pitch);
            }
        }
        self.kk = false;
        self.kl = false;
        self.kr = false;
    }

    fn transform_timestamps(&mut self, pump: &mut EventPump) {
        if self.notes4.is_empty() {
            return;
        }
        println!("Transforming timestamps...1||2 x Integrieren?");
        let val = self.eingabe(pump);
        let vali: i32 = val.parse().unwrap_or(0);
        let mut timestamps: Vec<f32> = self.notes4.iter().map(|n| n.timestamp).collect();
        timestamps.sort_by(|a, b| total_cmp_f32(*a, *b));
        let mut integral = 0.0f32;
        for note in self.notes4.iter_mut() {
            let x = note.timestamp;
            let mut value = 1.0f32;
            for t in &timestamps {
                value *= x - t;
            }
            if vali == 1 || vali == 2 {
                let mut integral1 = 0.0f32;
                for t in &timestamps {
                    integral1 += (x - t) * (x - t) / 2.0;
                }
                integral += integral1;
            } else if vali == 2 {
                let mut integral2 = 0.0f32;
                for t in &timestamps {
                    integral2 += (x - t) * (x - t) * (x - t) / 6.0;
                }
                integral += integral2;
            }
            let _ = value;
            note.timestamp = integral / (timestamps.len() * timestamps.len()) as f32;
        }
        let min_time = self.notes4.first().unwrap().timestamp;
        let max_time = self.notes4.last().unwrap().timestamp;
        for note in &self.notes4 {
            println!("{}", note.timestamp);
        }
        for note in self.notes4.iter_mut() {
            println!("Normalisiert");
            note.timestamp = (note.timestamp - min_time) / (max_time - min_time);
            println!("{}", note.timestamp);
        }
    }

    fn reverse_transform_timestamps(&mut self, pump: &mut EventPump) {
        if self.notes4.is_empty() {
            return;
        }
        println!("Reversing timestamp transformation...1||2 Ableitungen?");
        let val = self.eingabe(pump);
        let vali: i32 = val.parse().unwrap_or(0);
        let mut timestamps: Vec<f32> = self.notes4.iter().map(|n| n.timestamp).collect();
        timestamps.sort_by(|a, b| total_cmp_f32(*a, *b));
        for note in self.notes4.iter_mut() {
            let x = note.timestamp;
            let mut derivative = 0.0f32;
            if vali == 1 {
                for t in &timestamps {
                    if *t < x {
                        derivative += ((x - t) * 2.0).sqrt();
                    }
                }
            } else if vali == 2 {
                for t in &timestamps {
                    if *t < x {
                        derivative += ((x - t) * 3.0).powf(1.0 / 3.0);
                    }
                }
            }
            note.timestamp = derivative / timestamps.len() as f32;
        }
        let min_time = self.notes4.first().unwrap().timestamp;
        let max_time = self.notes4.last().unwrap().timestamp;
        let range = max_time - min_time;
        for note in &self.notes4 {
            println!("{}", note.timestamp);
        }
        for note in self.notes4.iter_mut() {
            println!("Normalisiert");
            if range != 0.0 {
                note.timestamp = (note.timestamp - min_time) / range;
            }
            println!("{}", note.timestamp);
        }
        println!("Timestamp transformation reversed with {} derivatives.", vali);
    }

    fn quantize_notes(&mut self, notes: &mut Vec<Note3>, quantization: i32) {
        if notes.is_empty() {
            return;
        }
        let quantization_step = (60.0 / self.sd().bpm) / quantization as f32;
        for note in notes.iter_mut() {
            let steps = (note.timestamp / quantization_step).round();
            note.timestamp = steps * quantization_step;
        }
        sort_notes_by_timestamp(notes);
    }

    // ------------- BPM -------------
    fn bpmtap(&mut self, pump: &mut EventPump) {
        let _ = pump;
        let mut tap_timebefore = 0.0f32;
        let mut tap_time = 0.0f32;
        let mut bpmtap = 0.0f32;
        let mut bpmtaps = 0.0f32;
        let mut bpmtapq = 0.0f32;
        let mut count: f32 = 1.0;
        let mut firstpress = true;
        loop {
            if let Some(event) = poll_event_compat() {
                if let Event::KeyDown {
                    keycode: Some(kc),
                    ..
                } = event
                {
                    if kc == Keycode::Num0 {
                        break;
                    } else if kc == Keycode::Num9 {
                        if firstpress {
                            tap_timebefore = ticks_f();
                            firstpress = false;
                        } else {
                            let currenttap_time = ticks_f();
                            tap_time = currenttap_time - tap_timebefore;
                            bpmtap = 60.0 / tap_time;
                            bpmtaps += bpmtap;
                            bpmtapq = bpmtaps / count;
                            println!(
                                "tapTime: {} bpmtap: {} bpmtaps: {} bpmtapq: {} count: {}",
                                tap_time, bpmtap, bpmtaps, bpmtapq, count
                            );
                            tap_timebefore = currenttap_time;
                            count += 1.0;
                        }
                    }
                }
            }
        }
        self.sdelay();
        self.sd_mut().bpm = bpmtapq;
        osc::send_bpm_to_renoise(self.sd().bpm);
    }

    fn set_bpm(&mut self) {
        osc::send_bpm_to_renoise(self.sd().bpm);
    }

    // ------------- Record -------------
    fn record(&mut self, pump: &mut EventPump) {
        let _ = pump;
        self.notes.clear();
        self.snotes.clear();
        self.new_notes.clear();
        self.notestoscale.clear();
        println!("Recording... Press '0' to stop.");
        let mut current_timestampstart = ticks_f();
        let mut current_timestamp = ticks_f();
        let mut recording_started = false;
        let mut recording_started0 = false;
        let mut settimestamp = false;
        let mut alleseins = false;
        let mut scaletoa = false;
        let mut mschrittweitanz = false;
        let mut numfactor = 1i32;
        let mut rschrittweitanz = 1.0f32;
        let mut note_value = 0i32;
        let _note_value2 = 0i32;
        let _note_value3 = 0i32;
        'rec: loop {
            if let Some(event) = poll_event_compat() {
                if let Event::KeyDown {
                    keycode: Some(kc),
                    ..
                } = event
                {
                    match record_key_action(kc, alleseins, scaletoa) {
                        RecordAction::Stop => break 'rec,
                        RecordAction::ToggleAlleseins => alleseins = !alleseins,
                        RecordAction::NoteFixedOne => {
                            note_value = 1;
                            settimestamp = true;
                        }
                        RecordAction::NoteDigit(d) => {
                            note_value = d * numfactor;
                            settimestamp = true;
                        }
                        RecordAction::NoteFKey(f) => {
                            note_value = f * numfactor;
                            self.kk = true;
                            self.kl = false;
                            self.kr = false;
                            scaletoa = true;
                            settimestamp = true;
                            numfactor = 1;
                        }
                        RecordAction::PageUp(v) => self.pgup = v,
                        RecordAction::SelectKk => {
                            self.kk = true;
                            self.kl = false;
                            self.kr = false;
                            scaletoa = !scaletoa;
                        }
                        RecordAction::SelectKr => {
                            self.kr = true;
                            self.kl = false;
                            self.kk = false;
                            scaletoa = !scaletoa;
                        }
                        RecordAction::SelectNone => {
                            self.kk = false;
                            self.kl = false;
                            self.kr = false;
                            scaletoa = !scaletoa;
                        }
                        RecordAction::Schritt3 => {
                            self.schrittweitanz = 3;
                            mschrittweitanz = true;
                        }
                        RecordAction::NumFactor(n) => numfactor = n,
                        RecordAction::RStep(r) => {
                            rschrittweitanz = r;
                            mschrittweitanz = true;
                        }
                        RecordAction::None => {}
                    }
                }
            }
            if settimestamp {
                if !recording_started {
                    current_timestampstart = ticks_f();
                    self.notes.push(Note {
                        timestamp: 0.0,
                        instrument: self.notes.len() as i32,
                        pitch: note_value,
                        scl: if scaletoa { 1 } else { 0 },
                        p2: if mschrittweitanz { 1.0 } else { 0.0 },
                        mschrittweitanz,
                        schrittweitanz: rschrittweitanz,
                    });
                    recording_started = true;
                } else if recording_started {
                    current_timestamp = ticks_f() - current_timestampstart;
                    self.notes.push(Note {
                        timestamp: current_timestamp,
                        instrument: self.notes.len() as i32,
                        pitch: note_value,
                        scl: if scaletoa { 1 } else { 0 },
                        p2: if mschrittweitanz { 1.0 } else { 0.0 },
                        mschrittweitanz,
                        schrittweitanz: rschrittweitanz,
                    });
                }
                if self.notes.len() >= 2 && self.notes[self.notes.len() - 2].scl == 1 {
                    scaletoa = false;
                }
                settimestamp = false;
                mschrittweitanz = false;
            }
        }
        let end_time = ticks_f() - current_timestampstart;
        self.notes.push(Note {
            timestamp: end_time,
            instrument: 0,
            pitch: 0,
            scl: if scaletoa { 1 } else { 0 },
            p2: 0.0,
            mschrittweitanz: false,
            schrittweitanz: 0.0,
        });
        for note in &self.notes {
            println!(
                "{}, {}, {}, {}, {}, {}",
                note.timestamp,
                note.instrument,
                note.pitch,
                note.scl,
                note.mschrittweitanz,
                note.schrittweitanz
            );
        }
        let mut ie = self.notes.len() as i32;
        println!("ie={}", ie);
        let mut i = 0i32;
        while i < ie {
            let has_pair = i + 1 < self.notes.len() as i32
                && (i as usize) < self.notes.len()
                && ((i + 1) as usize) < self.notes.len();
            if has_pair
                && self.notes[i as usize].scl == 1
                && self.notes[(i + 1) as usize].scl == 1
            {
                self.firstrec = true;
                self.start_time = self.notes[i as usize].timestamp;
                if ((i + 2) as usize) < self.notes.len() {
                    self.end_time = self.notes[(i + 2) as usize].timestamp;
                }
                self.start_indexnum = self.notes[i as usize].pitch as f32;
                self.end_indexnum = self.notes[(i + 1) as usize].pitch as f32;
                self.kk = true;
                self.indexnum_laenge =
                    1.0 - (1.0 / self.start_indexnum) - (1.0 / self.end_indexnum);
                let sw = (self.end_indexnum - self.start_indexnum) as i32;
                let mut nn = std::mem::take(&mut self.new_notes);
                self.scale_to_range_num(
                    &mut nn,
                    self.start_time,
                    self.end_time,
                    self.start_indexnum,
                    self.end_indexnum,
                    sw,
                    self.firstrec,
                );
                self.new_notes = nn;
                self.notes[(i + 1) as usize].pitch = 1;
                self.notes[i as usize].scl = 0;
                self.notes[(i + 1) as usize].scl = 0;
                self.notes.drain((i as usize)..((i + 2) as usize).min(self.notes.len()));
                let merge = std::mem::take(&mut self.notesr);
                let insert_at = (i as usize).min(self.notes.len());
                let mut merge = merge;
                self.notes.splice(insert_at..insert_at, merge.drain(..));
                ie = self.notes.len() as i32;
                self.notesr.clear();
                i = 0;
                self.firstrec = false;
            } else {
                i += 1;
            }
        }
        for i in 0..self.notes.len() {
            if self.notes[i].pitch > 9 {
                let pitch2 = self.notes[i].pitch - 9;
                let stamp = (self.end_time - self.start_time) / (pitch2 as f32);
                self.notes[i].timestamp = stamp;
                self.notes[i].instrument = 1000;
                self.notes[i].pitch = 0;
            }
        }
        println!("--notes--");
        for note in &self.notes {
            println!(
                "{}, {}, {}, {}",
                note.timestamp, note.instrument, note.pitch, note.scl
            );
        }
        self.scale();
        self.firstrec = false;
    }

    fn remove_and_record(&mut self, pump: &mut EventPump) {
        let _ = pump;
        self.new_notes.clear();
        self.scaled_notes.clear();
        self.scaled_notes2.clear();
        if self.notes4.is_empty() {
            println!("No notes recorded.");
            return;
        }
        if self.start_index < 0
            || self.end_index as usize > self.notes4.len()
            || self.start_index > self.end_index
        {
            println!("Ungültige Indices.");
            return;
        }
        let _instrument = self.notes4[self.start_index as usize].instrument;
        self.start_time = self.notes4[self.start_index as usize].timestamp;
        self.end_time = if (self.end_index as usize) < self.notes4.len() {
            self.notes4[self.end_index as usize].timestamp
        } else {
            1.0
        };
        let mut recording_started = false;
        self.notes4.drain(
            (self.start_index as usize)
                ..((self.end_index as usize).min(self.notes4.len())),
        );
        println!(
            "Noten im Bereich [{}, {}] gelöscht.",
            self.start_index, self.end_index
        );
        println!("Endgültige Noten4:");
        for n in &self.notes4 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        println!("Aufnahme neuer Noten... Drücken Sie '0' zum Stoppen.");
        let recording_start_time = ticks_f();
        let mut settimestamp = false;
        let mut note_value = 0i32;
        let mut alleseins = false;
        'rec: loop {
            if let Some(event) = poll_event_compat() {
                if let Event::KeyDown {
                    keycode: Some(kc),
                    ..
                } = event
                {
                    if kc == Keycode::Num0 {
                        break 'rec;
                    } else if kc == Keycode::A {
                        alleseins = !alleseins;
                    } else if let Some(v) = remove_and_record_note_value(kc, alleseins) {
                        note_value = v;
                        settimestamp = true;
                    }
                }
            }
            if settimestamp {
                if !recording_started {
                    self.new_notes.push(Note3 {
                        timestamp: 0.0,
                        instrument: self.notes3.len() as i32,
                        pitch: note_value,
                        glide: false,
                        p2: 0.0,
                    });
                    recording_started = true;
                } else {
                    let current_timestamp = ticks_f() - recording_start_time;
                    self.new_notes.push(Note3 {
                        timestamp: current_timestamp,
                        instrument: self.notes3.len() as i32,
                        pitch: note_value,
                        glide: false,
                        p2: 0.0,
                    });
                }
                settimestamp = false;
            }
        }
        let end_timerec = ticks_f() - recording_start_time;
        self.new_notes.push(Note3 {
            timestamp: end_timerec,
            instrument: 0,
            pitch: 0,
            glide: false,
            p2: 0.0,
        });
        println!("newNotes:");
        for n in &self.new_notes {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
        for i in 0..self.new_notes.len() {
            if self.new_notes[i].pitch > 9 {
                let pitch2 = self.new_notes[i].pitch - 9;
                let stamp = (self.end_time - self.start_time) / (pitch2 as f32);
                self.new_notes[i].timestamp = stamp;
                self.new_notes[i].instrument = 1000;
                self.new_notes[i].pitch = 0;
            }
        }
        self.scaled_notes.clear();
        self.scaled_notes2.clear();
        let mut nn = std::mem::take(&mut self.new_notes);
        self.scale_to_range(&mut nn, self.start_time, self.end_time);
        self.new_notes = nn;
        let insert_at = (self.start_index as usize).min(self.notes4.len());
        let mut sn2 = std::mem::take(&mut self.scaled_notes2);
        self.notes4.splice(insert_at..insert_at, sn2.drain(..));
        println!("Endgültige Noten4:");
        for n in &self.notes4 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
    }

    fn remove(&mut self) {
        self.new_notes.clear();
        self.scaled_notes.clear();
        self.scaled_notes2.clear();
        if self.notes4.is_empty() {
            println!("No notes recorded.");
            return;
        }
        if self.start_index < 0
            || self.end_index as usize > self.notes4.len()
            || self.start_index > self.end_index
        {
            println!("Ungültige Indices.");
            return;
        }
        let _instrument = self.notes4[self.start_index as usize].instrument;
        self.start_time = self.notes4[self.start_index as usize].timestamp;
        self.end_time = if (self.end_index as usize) < self.notes4.len() {
            self.notes4[self.end_index as usize].timestamp
        } else {
            1.0
        };
        let last_ts = self.notes4.last().unwrap().timestamp;
        if self.end_time == last_ts {
            self.notes4.remove(self.start_index as usize);
        } else {
            self.notes4.drain(
                (self.start_index as usize)
                    ..((self.end_index as usize).min(self.notes4.len())),
            );
        }
        println!(
            "Noten im Bereich [{}, {}] gelöscht.",
            self.start_index, self.end_index
        );
        println!("Endgültige Noten4:");
        for n in &self.notes4 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
    }

    fn modul1(&mut self, pump: &mut EventPump) {
        println!("time 1, istrument 2, pitch 3");
        let value = self.eingabe(pump);
        let select: i32 = value.parse().unwrap_or(0);
        println!("Anfangs- und Endabweichung in %");
        let valuea: f32 = self.eingabe(pump).parse().unwrap_or(0.0);
        let valuee: f32 = self.eingabe(pump).parse().unwrap_or(0.0);
        self.mod1.clear();
        for i in 0..self.notes4.len() {
            let step = ((valuee - valuea) / (self.notes4.len() as f32 - 1.0) * i as f32 / 100.0);
            match select {
                1 => self.mod1.push(Modu {
                    time: self.notes4[i].timestamp
                        + self.notes4[i].timestamp * step,
                    instrument: self.notes4[i].instrument,
                    pitch: self.notes4[i].pitch,
                }),
                2 => self.mod1.push(Modu {
                    time: self.notes4[i].timestamp,
                    instrument: (self.notes4[i].instrument as f32
                        + self.notes4[i].instrument as f32 * step) as i32,
                    pitch: self.notes4[i].pitch,
                }),
                3 => self.mod1.push(Modu {
                    time: self.notes4[i].timestamp,
                    instrument: self.notes4[i].instrument,
                    pitch: (self.notes4[i].pitch as f32
                        + self.notes4[i].pitch as f32 * step) as i32,
                }),
                _ => println!("Invalid number!"),
            }
        }
        for i in 0..self.mod1.len() {
            println!(
                "{} {} {} {}",
                i, self.mod1[i].time, self.mod1[i].instrument, self.mod1[i].pitch
            );
        }
        for i in 0..self.notes4.len() {
            println!(
                "{} {} {} {}",
                i, self.notes4[i].timestamp, self.notes4[i].instrument, self.notes4[i].pitch
            );
        }
        self.mod1.push(Modu {
            time: 1.0,
            instrument: 0,
            pitch: 0,
        });
    }

    // ------------- Renoise/Clipboard -------------
    fn convert_notes_to_schism_pattern(&self, pattern_length: i32) {
        println!("Converting notes to Schism pattern...");
        let af = "|";
        let ed = ".\n";
        let ch_str = "..";
        let eff_str = "...";
        let empty = "..........";
        let mut buff = String::from("Pasted Pattern - IT\n");
        let mut bnote = false;
        for i in 0..pattern_length {
            let mut line_str = String::new();
            for ii in 0..self.notes4.len() {
                if (self.notes4[ii].timestamp * pattern_length as f32) as i32 == i {
                    let note_str = midi_to_renoise_note(self.notes4[ii].pitch);
                    let instr = "00";
                    line_str += af;
                    line_str += &note_str;
                    line_str += instr;
                    line_str += ch_str;
                    line_str += eff_str;
                    line_str += ed;
                    bnote = true;
                }
            }
            if !bnote {
                line_str += af;
                line_str += empty;
                line_str += ed;
            }
            buff += &line_str;
            bnote = false;
        }
        let temp_file = "/tmp/schism_pattern.pit";
        std::fs::write(temp_file, &buff).ok();
        let ok = copy_to_clipboard(temp_file);
        if ok {
            println!(
                "Pattern copied to clipboard with {} lines",
                pattern_length
            );
        } else {
            println!("Pattern saved to file: {}", temp_file);
            println!("Note: xclip/xsel not found. Please install xclip or xsel, or copy the file manually.");
            println!("Pattern content ({} lines):", pattern_length);
            println!("{}", buff);
        }
    }

    fn convert_notes_to_schism_pattern_with_input(&mut self, pattern_length: i32, pump: &mut EventPump) {
        let mut pl = pattern_length;
        if pl <= 0 {
            println!("Pattern length?");
            let s = self.eingabe(pump);
            pl = s.parse().unwrap_or(0);
        }
        self.convert_notes_to_schism_pattern(pl);
    }

    fn convert_notes_to_renoise_pattern(&self, pattern_length: i32) {
        println!("Converting notes to Renoise pattern...");
        let mut xml = String::from("<?xml version=\"1.0\"?>\n");
        xml += "<PatternClipboard.BlockBuffer doc_version=\"0\">\n";
        xml += "  <Columns>\n";
        xml += "    <Column>\n";
        xml += "      <Column>\n";
        xml += "        <Lines>\n";
        let time_per_line = 1.0 / pattern_length as f32;
        let mut sorted = self.notes4.clone();
        sort_notes_by_timestamp(&mut sorted);
        let mut line_map: std::collections::HashMap<i32, Vec<(String, String)>> =
            std::collections::HashMap::new();
        for note in &sorted {
            if note.instrument == 0 && note.timestamp == 1.0 {
                continue;
            }
            let line_index = (note.timestamp / time_per_line) as i32;
            if line_index >= pattern_length {
                continue;
            }
            let note_str = midi_to_renoise_note(note.pitch);
            line_map
                .entry(line_index)
                .or_default()
                .push((note_str, "00".to_string()));
        }
        for i in 0..pattern_length {
            if let Some(notes) = line_map.get(&i) {
                xml += &format!("          <Line index=\"{}\">\n", i);
                xml += "            <NoteColumns>\n";
                for (n, inst) in notes {
                    xml += "              <NoteColumn>\n";
                    xml += &format!("                <Note>{}</Note>\n", n);
                    xml += &format!("                <Instrument>{}</Instrument>\n", inst);
                    xml += "              </NoteColumn>\n";
                }
                xml += "            </NoteColumns>\n";
                xml += "          </Line>\n";
            } else {
                xml += &format!("          <Line index=\"{}\"/>\n", i);
            }
        }
        xml += "        </Lines>\n";
        xml += "        <ColumnType>NoteColumn</ColumnType>\n";
        xml += "        <SubColumnMask>true true true false false false false false</SubColumnMask>\n";
        xml += "      </Column>\n";
        xml += "    </Column>\n";
        xml += "  </Columns>\n";
        xml += "</PatternClipboard.BlockBuffer>\n";
        let temp_file = "/tmp/renoise_pattern.xml";
        std::fs::write(temp_file, &xml).ok();
        let ok = copy_to_clipboard(temp_file);
        if ok {
            println!(
                "Pattern copied to clipboard with {} lines",
                pattern_length
            );
        } else {
            println!("Pattern saved to file: {}", temp_file);
            println!("Note: xclip/xsel not found. Please install xclip or xsel, or copy the file manually.");
        }
    }

    fn paste_renoise_pattern(&mut self, pump: &mut EventPump) {
        let clipboard_text = get_clipboard_content();
        if !clipboard_text.is_empty() {
            println!("Clipboard content: {}", clipboard_text);
            println!("1 || 2 select || full Renoise Pattern");
            let select = self.eingabe(pump);
            if select.trim().parse::<i32>().unwrap_or(0) == 1 {
                self.convert_renoise_pattern_to_notes(&clipboard_text);
            } else {
                println!("Patternlänge?");
                let pl = self.eingabe(pump);
                self.convert_renoise_pattern_to_notes2(&clipboard_text, pl.parse().unwrap_or(0));
            }
        } else {
            println!("No clipboard content available");
        }
    }

    fn paste_renoise_pattern_to_schism(&mut self, pump: &mut EventPump) {
        let clipboard_text = get_clipboard_content();
        if !clipboard_text.is_empty() {
            println!("Clipboard content detected");
            println!("Pattern length?");
            let pl = self.eingabe(pump);
            self.convert_renoise_pattern_to_notes2_for_schism(
                &clipboard_text,
                pl.parse().unwrap_or(0),
            );
        } else {
            println!("No clipboard content available");
        }
    }

    fn convert_renoise_pattern_to_notes(&mut self, pattern_xml: &str) {
        println!("Starting to convert Renoise pattern...");
        let lines = parse_line_notes(pattern_xml);
        if lines.is_empty() {
            println!("No notes found in pattern");
            return;
        }
        self.notes4.clear();
        println!("Cleared existing notes");
        let first_line_index = lines.first().unwrap().0;
        let last_line_index = lines.last().unwrap().0;
        let start_time = first_line_index as f32 * (60.0 / self.sd().bpm);
        let end_time = last_line_index as f32 * (60.0 / self.sd().bpm);
        println!("lastlineIndex: {}", last_line_index);
        let total_pattern_length = 64.0 * (60.0 / self.sd().bpm);
        self.notes4.clear();
        // Pause am Anfang prüfen (Line 0 ohne Note/Instrument)
        if first_line_index == 0 {
            let has_line0 = lines.iter().any(|(i, _, _)| *i == 0);
            if !has_line0 {
                self.notes4.push(Note3::new(0.0, 0, 0));
                println!("Added pause at start of pattern");
            }
        }
        let mut notes_processed = 0;
        for (line_index, note_str, inst_str) in &lines {
            let midi_note = renoise_note_to_midi(note_str);
            let instrument_num = i32::from_str_radix(inst_str, 16).unwrap_or(0);
            let timestamp = *line_index as f32 * (60.0 / self.sd().bpm);
            let scaled_timestamp = timestamp / total_pattern_length;
            self.notes4.push(Note3::new(scaled_timestamp, instrument_num, midi_note));
            notes_processed += 1;
            println!(
                "Processed note: line={} note={} inst={} midi={} timestamp={}",
                line_index, note_str, inst_str, midi_note, scaled_timestamp
            );
        }
        if notes_processed == 0 {
            println!("No notes were processed from the XML");
            return;
        }
        self.notes4.push(Note3::new(1.0, 0, 0));
        self.current_pattern = (self.current_pattern + 1) % self.num_patterns;
        self.pnotes[self.current_pattern as usize] = self.notes4.clone();
        println!("Successfully converted {} notes", notes_processed);
    }

    fn convert_renoise_pattern_to_notes2(&mut self, pattern_xml: &str, patternlenght: i32) {
        println!("Starting to convert Renoise pattern...");
        let lines = parse_line_notes(pattern_xml);
        if lines.is_empty() {
            println!("No notes found in pattern");
            return;
        }
        self.notes4.clear();
        println!("Cleared existing notes");
        let first_line_index = lines.first().unwrap().0;
        let last_line_index = lines.last().unwrap().0;
        let start_time = first_line_index as f32 * (60.0 / self.sd().bpm);
        let end_time = last_line_index as f32 * (60.0 / self.sd().bpm);
        let total_pattern_length = patternlenght as f32 * (60.0 / self.sd().bpm);
        self.notes4.clear();
        if first_line_index == 0 {
            let has_line0 = lines.iter().any(|(i, _, _)| *i == 0);
            if !has_line0 {
                self.notes4.push(Note3::new(0.0, 0, 0));
                println!("Added pause at start of pattern");
            }
        }
        let mut notes_processed = 0;
        for (line_index, note_str, inst_str) in &lines {
            let midi_note = renoise_note_to_midi(note_str);
            let instrument_num = i32::from_str_radix(inst_str, 16).unwrap_or(0);
            let timestamp = *line_index as f32 * (60.0 / self.sd().bpm);
            let scaled_timestamp = timestamp / total_pattern_length;
            self.notes4.push(Note3::new(scaled_timestamp, instrument_num, midi_note));
            notes_processed += 1;
            println!(
                "Processed note: line={} note={} inst={} midi={} timestamp={}",
                line_index, note_str, inst_str, midi_note, scaled_timestamp
            );
        }
        if notes_processed == 0 {
            println!("No notes were processed from the XML");
            return;
        }
        self.notes4.push(Note3::new(1.0, 0, 0));
        self.current_pattern = (self.current_pattern + 1) % self.num_patterns;
        self.pnotes[self.current_pattern as usize] = self.notes4.clone();
        println!("Successfully converted {} notes", notes_processed);
        println!(
            "patternlenght: {}firstlineindex: {}lastlineindex: {}",
            patternlenght, first_line_index, last_line_index
        );
    }

    fn convert_renoise_pattern_to_notes2_for_schism(
        &mut self,
        pattern_xml: &str,
        pattern_length: i32,
    ) {
        println!("Converting Renoise pattern to Schismtracker Notes2 format...");
        let mut pl = pattern_length;
        if pl <= 0 {
            println!("Pattern length?");
            // C++ nutzt hier Eingabe(); wir lesen hier stdin-frei über Pump — dieser Zweig braucht pump.
            // Port-Abweichung: Eingabe fehlt, wir setzen 64 als Fallback.
            pl = 64;
        }
        let lines = parse_line_notes(pattern_xml);
        if lines.is_empty() {
            println!("No notes found in pattern");
            return;
        }
        let af = "|";
        let ed = ".\n";
        let ch_str = "..";
        let eff_str = "...";
        let empty = "..........";
        let mut buff = String::from("Pasted Pattern - IT\n");
        for i in 0..pl {
            let mut line_str = String::new();
            let mut bnote = false;
            if let Some((_, note_text, inst_text)) = lines.iter().find(|(idx, _, _)| *idx == i) {
                let instrument_num = i32::from_str_radix(inst_text, 16).unwrap_or(0);
                let instr = format!("{:02}", instrument_num);
                line_str += af;
                line_str += note_text;
                line_str += &instr;
                line_str += ch_str;
                line_str += eff_str;
                line_str += ed;
                bnote = true;
            }
            if !bnote {
                line_str += af;
                line_str += empty;
                line_str += ed;
            }
            buff += &line_str;
        }
        let temp_file = "/tmp/schism_pattern.pit";
        std::fs::write(temp_file, &buff).ok();
        let ok = copy_to_clipboard(temp_file);
        if ok {
            println!(
                "Renoise pattern converted to Schismtracker Notes2 format and copied to clipboard with {} lines",
                pl
            );
        } else {
            println!(
                "Renoise pattern converted to Schismtracker Notes2 format and saved to file: {}",
                temp_file
            );
            println!("Note: xclip/xsel not found. Please install xclip or xsel, or copy the file manually.");
            println!("Pattern content ({} lines):", pl);
            println!("{}", buff);
        }
    }

    fn interpolate_notes_with_sinus_pitch(
        &mut self,
        pump: &mut EventPump,
        notes4: &[Note3],
        steps_per_intervala: i32,
        steps_per_intervale: i32,
        factorpi: f32,
        noteanz: f32,
        curve: i32,
    ) -> Vec<Note3> {
        if notes4.is_empty() {
            return Vec::new();
        }
        // C++ ruft hier interaktiv anfendIndex() auf.
        self.anfend_index(pump);
        let start = self.start_index.clamp(0, notes4.len() as i32 - 1) as usize;
        let end = self
            .end_index
            .clamp(0, notes4.len() as i32 - 1) as usize;
        sinus_interpolate(
            notes4,
            start,
            end,
            steps_per_intervala,
            steps_per_intervale,
            factorpi,
            noteanz,
            curve,
        )
    }

    // ------------- CSV -------------
    fn save_to_csv(&self, filename: &str, num: usize) {
        let _ = std::fs::write(filename, notes_to_csv(&self.pnotes[num]));
    }

    fn read_from_csv(&mut self, filename: &str, num: usize) {
        if num >= self.pnotes.len() {
            for p in self.pnotes.iter_mut() {
                p.clear();
            }
            println!("Alle Pattern geleert");
            return;
        }
        self.pnotes[num].clear();
        let content = std::fs::read_to_string(filename).unwrap_or_default();
        self.pnotes[num] = notes_from_csv(&content);
        println!(
            "Loaded {} notes into pattern {}",
            self.pnotes[num].len(),
            num
        );
    }

    // ------------- Sonstiges -------------
    fn toggle_renoise_play(&mut self) {
        self.renoise_playing = !self.renoise_playing;
        let command = if self.renoise_playing { "start" } else { "stop" };
        osc::transport(self.renoise_playing);
        println!("Renoise Play {}", command);
    }

    fn toggle_renoise_record(&mut self) {
        self.renoise_recording = !self.renoise_recording;
        osc::edit_mode(self.renoise_recording);
        let command = if self.renoise_recording { "start" } else { "stop" };
        println!("Renoise Record {}", command);
    }

    fn start_renoise_play_and_record(&mut self) {
        self.renoise_play_and_record_enabled = true;
        self.toggle_renoise_record();
        self.toggle_renoise_play();
    }

    fn clear_notes(&mut self) {
        self.notes.clear();
        self.notes2.clear();
        self.notes3.clear();
        println!("Recorded notes cleared.");
    }

    fn clear_notes2(&mut self) {
        self.notes4.clear();
        self.new_notes.clear();
        self.scaled_notes.clear();
        self.scaled_notes2.clear();
        self.mod1.clear();
        println!("Recorded notes4 + scaledNotes cleared.");
    }

    fn select(&mut self, num: usize, pump: &mut EventPump) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        println!("{} num {}", self.pnotes[num].len(), num);
        if num >= self.pselect.len() {
            self.pselect.resize(num + 1, Vec::new());
        }
        self.pselect[num].clear();
        println!("1 for select, 0 for free");
        for i in 0..self.pnotes[num].len() {
            println!(
                "{} {}, {}, {}",
                i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
            );
            let value = self.eingabe(pump);
            let sel: i32 = value.parse().unwrap_or(0);
            self.pselect[num].push(sel);
        }
    }

    fn setpdauer(&mut self, num: usize, pump: &mut EventPump) {
        println!("Setze Dauer für Pattern {}", num);
        if num >= self.pdauer.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        self.pdauer[num][0] = self.eingabe(pump).parse().unwrap_or(0.0);
    }

    fn reverse(&mut self, num: usize) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        for i in 0..self.pnotes[num].len() {
            if self.pselect.get(num).and_then(|v| v.get(i)).copied().unwrap_or(0) == 1 {
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
                self.pnotes[num][i].timestamp = 1.0 - self.pnotes[num][i].timestamp;
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
            }
        }
        sort_notes_by_timestamp(&mut self.pnotes[num]);
        for i in 0..self.pnotes[num].len() {
            println!(
                "{} {}, {}, {}",
                i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
            );
        }
    }

    fn mal(&mut self, num: usize, factor: f32) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        for i in 0..self.pnotes[num].len() {
            if self.pselect.get(num).and_then(|v| v.get(i)).copied().unwrap_or(0) == 1 {
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
                self.pnotes[num][i].timestamp *= factor;
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
            }
        }
        sort_notes_by_timestamp(&mut self.pnotes[num]);
        for i in 0..self.pnotes[num].len() {
            println!(
                "{} {}, {}, {}",
                i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
            );
        }
    }

    fn durch(&mut self, num: usize, factor: f32) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        for i in 0..self.pnotes[num].len() {
            if self.pselect.get(num).and_then(|v| v.get(i)).copied().unwrap_or(0) == 1 {
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
                self.pnotes[num][i].timestamp /= factor;
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
            }
        }
        sort_notes_by_timestamp(&mut self.pnotes[num]);
        for i in 0..self.pnotes[num].len() {
            println!(
                "{} {}, {}, {}",
                i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
            );
        }
    }

    fn plus(&mut self, num: usize, factor: f32) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        for i in 0..self.pnotes[num].len() {
            if self.pselect.get(num).and_then(|v| v.get(i)).copied().unwrap_or(0) == 1 {
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
                self.pnotes[num][i].timestamp += factor;
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
            }
        }
        sort_notes_by_timestamp(&mut self.pnotes[num]);
        for i in 0..self.pnotes[num].len() {
            println!(
                "{} {}, {}, {}",
                i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
            );
        }
    }

    fn minus(&mut self, num: usize, factor: f32) {
        if num >= self.pnotes.len() {
            println!("Pattern {num} ungültig");
            return;
        }
        for i in 0..self.pnotes[num].len() {
            if self.pselect.get(num).and_then(|v| v.get(i)).copied().unwrap_or(0) == 1 {
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
                self.pnotes[num][i].timestamp -= factor;
                println!(
                    "{} {}, {}, {}",
                    i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
                );
            }
        }
        sort_notes_by_timestamp(&mut self.pnotes[num]);
        for i in 0..self.pnotes[num].len() {
            println!(
                "{} {}, {}, {}",
                i, self.pnotes[num][i].timestamp, self.pnotes[num][i].instrument, self.pnotes[num][i].pitch
            );
        }
    }

    fn vectorinsert(&mut self, num: usize) {
        let mut v = self.pnotes[num].clone();
        self.notes4.splice(0..0, v.drain(..));
        sort_notes_by_timestamp(&mut self.notes4);
        for i in 0..self.notes4.len() {
            println!(
                "{} {}, {}, {}",
                i, self.notes4[i].timestamp, self.notes4[i].instrument, self.notes4[i].pitch
            );
        }
    }

    fn recntr(&mut self, pump: &mut EventPump) {
        println!("Geben Sie den Anfangsindex ein: ");
        self.start_index = self.eingabe(pump).parse().unwrap_or(0);
        println!("Geben Sie den Endindex ein: ");
        self.end_index = self.eingabe(pump).parse().unwrap_or(0);
        let start = self.start_index.max(0);
        let end = self.end_index.min(self.notes4.len() as i32);
        for i in start..end {
            println!("Geben Sie den instrnum ein: ");
            self.notes4[i as usize].instrument = self.eingabe(pump).parse().unwrap_or(0);
        }
        self.dis_pl_notes();
    }

    fn recntrf(&mut self, pump: &mut EventPump) {
        println!("Geben Sie den Anfangsindex ein: ");
        self.start_index = self.eingabe(pump).parse().unwrap_or(0);
        println!("Geben Sie den Endindex ein: ");
        self.end_index = self.eingabe(pump).parse().unwrap_or(0);
        let start = self.start_index.max(0);
        let end = self.end_index.min(self.notes4.len() as i32);
        for i in start..end {
            println!("Geben Sie den instrnum ein: ");
            let v: i32 = self.eingabe(pump).parse().unwrap_or(0);
            self.notes4[i as usize].pitch = v + 60;
        }
        self.dis_pl_notes();
    }

    fn vecforrecf(&mut self, pump: &mut EventPump) {
        println!("Geben Sie die recsteps ein: ");
        let recsteps: i32 = self.eingabe(pump).parse().unwrap_or(0);
        self.veforec.clear();
        for i in 0..recsteps {
            let mut event = VecForRec::default();
            println!("Geben Sie den Anfangsindexnum ein: ");
            event.an_ix_um = self.eingabe(pump).parse().unwrap_or(0.0);
            println!("Geben Sie den Endindexnum ein: ");
            event.en_ix_um = self.eingabe(pump).parse().unwrap_or(0.0);
            println!("Geben Sie die schrittweitanz ein: ");
            event.steps = self.eingabe(pump).parse().unwrap_or(0);
            println!("Geben Sie den kis ein: ");
            event.kis = self.eingabe(pump).parse().unwrap_or(0);
            event.start_time = self.notes4[i as usize].timestamp;
            event.end_time = self.notes4[i as usize + 1].timestamp;
            event.recsteps = recsteps;
            event.recstep = i;
            self.veforec.push(event);
        }
        println!("veforec:");
        for n in &self.veforec {
            println!(
                "{}, {}, {}, {}, {}, {}",
                n.recsteps, n.recstep, n.an_ix_um, n.en_ix_um, n.steps, n.kis
            );
        }
        for i in 0..recsteps {
            match self.veforec[i as usize].kis {
                1 => self.kk = !self.kk,
                2 => self.kl = !self.kl,
                3 => self.kr = !self.kr,
                4 => {}
                _ => {}
            }
            for ii in 0..self.notes4.len() {
                if self.notes4[ii].timestamp == self.veforec[i as usize].start_time {
                    self.start_index = ii as i32;
                    self.end_index = ii as i32 + 1;
                }
            }
            self.start_indexnum = self.veforec[i as usize].an_ix_um;
            self.end_indexnum = self.veforec[i as usize].en_ix_um;
            self.schrittweitanz = self.veforec[i as usize].steps;
            self.start_time = self.notes4[i as usize].timestamp;
            self.end_time = self.notes4[i as usize + 1].timestamp;
            println!(
                "startIndex: {} endIndex: {} startIndexnum: {} endIndexnum: {} schrittweitanz: {} startTime: {} endTime: {}",
                self.start_index,
                self.end_index,
                self.start_indexnum,
                self.end_indexnum,
                self.schrittweitanz,
                self.start_time,
                self.end_time
            );
            self.remove();
            self.new_notes.clear();
            let mut nn = std::mem::take(&mut self.new_notes);
            self.scale_to_range_num(
                &mut nn,
                self.start_time,
                self.end_time,
                self.start_indexnum,
                self.end_indexnum,
                self.schrittweitanz,
                self.firstrec,
            );
            self.new_notes = nn;
            self.kk = false;
            self.kl = false;
            self.kr = false;
        }
        let _ = pump;
    }

    fn create_graph_window(&mut self, pump: &mut EventPump) {
        println!("DrawGraph");
        println!("Geben Sie den Anfangsindexnum ein: ");
        let start_indexnum: i32 = self.eingabe(pump).parse().unwrap_or(0);
        println!("Geben Sie den Endindexnum ein: ");
        let end_indexnum: i32 = self.eingabe(pump).parse().unwrap_or(0);
        if self.notes4.is_empty() {
            println!("notes4 ist leer");
            return;
        }
        let start_idx = start_indexnum.clamp(0, self.notes4.len() as i32) as usize;
        let end_idx = end_indexnum.clamp(start_idx as i32, self.notes4.len() as i32) as usize;
        let start_time = self.notes4[start_idx].timestamp;
        let end_time = self.notes4[end_idx.min(self.notes4.len() - 1)].timestamp;
        let total_duration = end_time - start_time;
        println!("{} {} {}", start_time, end_time, total_duration);
        self.notes4.drain(start_idx..end_idx);
        println!(
            "Noten im Bereich [{}, {}] gelöscht.",
            start_indexnum, end_indexnum
        );
        println!("Endgültige Noten4:");
        for n in &self.notes4 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }

        self.run_graph_window(pump, start_idx, start_time, end_time);
    }

    /// "Graph Window": Tonhöhen-/Schrittweitenkurven per Maus zeichnen,
    /// F füllt die Step-Kurve, V tastet sie ab, O druckt die Punkte,
    /// 0 beendet und wandelt die abgetasteten Punkte in Noten um.
    fn run_graph_window(
        &mut self,
        pump: &mut EventPump,
        start_idx: usize,
        start_time: f32,
        end_time: f32,
    ) {
        let _ = pump;
        let sdl = match sdl2::init() {
            Ok(s) => s,
            Err(e) => {
                println!("SDL init: {e}");
                return;
            }
        };
        let video = match sdl.video() {
            Ok(v) => v,
            Err(e) => {
                println!("Video: {e}");
                return;
            }
        };
        let window = match video.window("Graph Window", 800, 600).build() {
            Ok(w) => w,
            Err(e) => {
                println!("Fenster: {e}");
                return;
            }
        };
        let mut canvas = match window.into_canvas().accelerated().build() {
            Ok(c) => c,
            Err(e) => {
                println!("Renderer: {e}");
                return;
            }
        };

        let mut pitch_graph: Vec<(i32, i32)> = Vec::new();
        let mut step_graph: Vec<(i32, i32)> = Vec::new();
        let mut stepxy: Vec<(i32, i32)> = Vec::new();
        let mut pitchstep: Vec<(i32, i32)> = Vec::new();
        let mut quit = false;
        while !quit {
            unsafe {
                let mut raw = std::mem::MaybeUninit::<sdl2::sys::SDL_Event>::uninit();
                while sdl2::sys::SDL_PollEvent(raw.as_mut_ptr()) == 1 {
                    let ev = raw.assume_init_ref();
                    if ev.type_ == sdl2::sys::SDL_EventType::SDL_QUIT as u32 {
                        quit = true;
                    } else if ev.type_ == sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32 {
                        let sym = ev.key.keysym.sym;
                        if sym == Keycode::Num0.into_i32() {
                            quit = true;
                        } else if sym == Keycode::F.into_i32() {
                            stepxy = interpolate_step_graph(&step_graph);
                            println!("stepGraph2: {} Punkte", stepxy.len());
                        } else if sym == Keycode::V.into_i32() {
                            pitchstep = ae_points(&stepxy);
                            println!("pitchstepGraph2: {} Punkte", pitchstep.len());
                        } else if sym == Keycode::O.into_i32() {
                            println!("pitchGraph: {:?}", pitch_graph);
                            println!("stepGraph: {:?}", step_graph);
                        }
                    } else if ev.type_ == sdl2::sys::SDL_EventType::SDL_MOUSEBUTTONDOWN as u32 {
                        let (x, y) = (ev.button.x, ev.button.y);
                        if y < 200 {
                            pitch_graph.push((x, y));
                        } else if y < 400 {
                            step_graph.push((x, y - 200));
                        }
                    }
                }
            }

            let _ = canvas.set_draw_color(Color::RGB(0, 0, 0));
            canvas.clear();
            if pitch_graph.len() >= 2 {
                let _ = canvas.set_draw_color(Color::RGB(255, 0, 0));
                for w in pitch_graph.windows(2) {
                    let _ = canvas.draw_line(Point::new(w[0].0, w[0].1), Point::new(w[1].0, w[1].1));
                }
            }
            if step_graph.len() >= 2 {
                let _ = canvas.set_draw_color(Color::RGB(0, 255, 0));
                for w in step_graph.windows(2) {
                    let _ = canvas.draw_line(
                        Point::new(w[0].0, w[0].1 + 200),
                        Point::new(w[1].0, w[1].1 + 200),
                    );
                }
            }
            if pitchstep.len() >= 2 {
                let _ = canvas.set_draw_color(Color::RGB(0, 0, 255));
                for w in pitchstep.windows(2) {
                    let _ = canvas.draw_line(
                        Point::new(w[0].0, w[0].1 + 400),
                        Point::new(w[1].0, w[1].1 + 400),
                    );
                }
            }
            canvas.present();
            thread::sleep(Duration::from_millis(10));
        }

        if pitchstep.is_empty() {
            println!("Keine abgetasteten Graph-Punkte (V drücken) – keine Noten erzeugt.");
            return;
        }
        let new_notes = graph_points_to_notes(&pitchstep, start_time, end_time);
        let insert_at = start_idx.min(self.notes4.len());
        self.notes4.splice(insert_at..insert_at, new_notes);
        println!("Graph-Noten eingefügt: {}", pitchstep.len().saturating_sub(1));
        for n in &self.notes4 {
            println!("{}, {}, {}", n.timestamp, n.instrument, n.pitch);
        }
    }

    fn display(&self, renderer: &mut Canvas<Window>, ttf: &Sdl2TtfContext) {
        let font_path = "./sagan.ttf";
        let font = match ttf.load_font(font_path, 10) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to load font: {}", e);
                return;
            }
        };
        let text_color = Color::RGB(255, 255, 255);
        let texture_creator = renderer.texture_creator();
        let bpm_text = format!("BPM: {}", self.sd().bpm);
        if let Ok(surface) = font.render(&bpm_text).solid(text_color) {
            let w = surface.width();
            let h = surface.height();
            if let Ok(texture) = texture_creator.create_texture_from_surface(&surface) {
                let _ = renderer.copy(&texture, None, Some(Rect::new(10, 10, w, h)));
            }
        }
        let notes_text = {
            let mut s = String::from("Recorded Notes: ");
            for note in &self.notes {
                s += &format!("{} ", note.pitch);
            }
            s
        };
        let mut surface_dims = (0u32, 0u32);
        if let Ok(surface) = font.render(&notes_text).solid(text_color) {
            surface_dims = (surface.width(), surface.height());
            if let Ok(texture) = texture_creator.create_texture_from_surface(&surface) {
                let _ = renderer.copy(
                    &texture,
                    None,
                    Some(Rect::new(10, 50, surface_dims.0, surface_dims.1)),
                );
            }
        }
        if !self.notes4.is_empty() {
            renderer.set_draw_color(Color::RGB(63, 63, 63));
            let (w, h) = surface_dims;
            if h >= 5 {
                let _ = renderer.draw_line(
                    (0, h.saturating_sub(5) as i32),
                    (w.max(1) as i32, h.saturating_sub(5) as i32),
                );
                let _ = renderer.draw_line((0, 95), (0, h.saturating_sub(5) as i32));
            }
            renderer.set_draw_color(Color::RGB(255, 0, 0));
            for i in 0..self.notes4.len().saturating_sub(1) {
                let x1 = (self.notes4[i].timestamp * w.max(1) as f32) as i32;
                let x2 = (self.notes4[i + 1].timestamp * w.max(1) as f32) as i32;
                let y = h.saturating_sub(5) as i32;
                let _ = renderer.draw_line((x1, y), (x2, y));
            }
            renderer.set_draw_color(Color::RGB(0, 255, 0));
            for i in 0..self.notes4.len().saturating_sub(1) {
                let x1 = (self.notes4[i].timestamp * w.max(1) as f32) as i32;
                let x2 = (self.notes4[i + 1].timestamp * w.max(1) as f32) as i32;
                let y = h.saturating_sub(5) as i32;
                let _ = renderer.draw_line((x1, y), (x2, y));
            }
            renderer.set_draw_color(Color::RGB(0, 0, 255));
            for i in 0..self.notes4.len().saturating_sub(1) {
                let x1 = (self.notes4[i].timestamp * w.max(1) as f32) as i32;
                let x2 = (self.notes4[i + 1].timestamp * w.max(1) as f32) as i32;
                let y = h.saturating_sub(5) as i32;
                let _ = renderer.draw_line((x1, y), (x2, y));
            }
        }
    }
}

fn main() -> Result<(), String> {
    let sdl = sdl2::init().map_err(|e| e.to_string())?;
    let mut pump = sdl.event_pump().map_err(|e| e.to_string())?;
    let video_subsys = sdl.video().map_err(|e| e.to_string())?;
    let window = video_subsys
        .window("Pattern Recorder", 200, 200)
        .build()
        .map_err(|e| e.to_string())?;
    let mut renderer = window
        .into_canvas()
        .build()
        .map_err(|e| e.to_string())?;
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;
    let mut recorder = PatternRecorder::new(&sdl)?;
    inject_test_keys();

    let mut playrenoise = false;
    let mut q_pressed = false;
    let mut kpressed = false;
    let mut tpressed = false;
    let mut sbool = false;
    let mut sboolx = false;
    let mut sboolinsert = false;
    let mut dbool = false;
    let mut present = false;
    let mut playp = false;
    let mut playpm = false;
    let mut record_flag = false;
    let mut ntr = false;
    let mut save = false;
    let mut snotes4 = false;
    let mut sdlkd = false;
    let mut lbeats = false;
    let mut playpatanz: i32 = 0;

    loop {
        if let Some(event) = poll_event_compat() {
            match event {
                Event::Quit { .. } => break,
                Event::KeyUp {
                    keycode: Some(kc),
                    ..
                } => {
                    if kc == Keycode::Q {
                        q_pressed = false;
                    } else if kc == Keycode::K {
                        kpressed = true;
                    }
                }
                Event::KeyDown {
                    keycode: Some(kc),
                    ..
                } => {
                    let d = digit_value(kc);
                    let digit_1_8 = d.is_some_and(|x| (1..=8).contains(&x));
                    if kc == Keycode::K {
                        kpressed = true;
                        recorder.firstrec = false;
                        recorder.kk = false;
                        recorder.kl = false;
                        recorder.kr = false;
                    } else if kpressed && kc == Keycode::Num1 {
                        println!("Geben Sie den Anfangsindex ein: ");
                        let start_index2: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        println!("Geben Sie den Endindex ein: ");
                        let end_index2: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        println!("Geben Sie den Anfangsindexnum ein: ");
                        let start_indexnumstart: f32 = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("Geben Sie den Endindexnum ein: ");
                        let end_indexnumstart: f32 = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("nach timestamp 1 , vor nächstem timestamp 2");
                        let vornachtimestamp: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        println!("Geben Sie den selectpitch ein: ");
                        let selectpitch: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        let stepsi = if !recorder.total_di {
                            end_index2 - start_index2
                        } else {
                            1
                        };
                        let stepsin = (1.0 / end_indexnumstart) - (1.0 / start_indexnumstart);
                        let stepsizeadd = if stepsi != 0 {
                            stepsin / stepsi as f32
                        } else {
                            0.0
                        };
                        recorder.notes4p.clear();
                        recorder.notes4p = recorder.notes4.clone();
                        let n4len = recorder.notes4.len();
                        let mut count: f32 = 0.0;
                        for i in start_index2..end_index2 {
                            let iu = i as usize;
                            if iu + 1 >= n4len || iu >= recorder.notes4p.len() {
                                count += 1.0;
                                continue;
                            }
                            let currentstepsize =
                                recorder.notes4[iu + 1].timestamp - recorder.notes4[iu].timestamp;
                            let newstep = if vornachtimestamp == 1 {
                                recorder.notes4[iu].timestamp
                                    + ((1.0 / start_indexnumstart) + (count * stepsizeadd))
                                        * currentstepsize
                            } else if vornachtimestamp == 2 {
                                recorder.notes4[iu + 1].timestamp
                                    - ((1.0 / start_indexnumstart) + (count * stepsizeadd))
                                        * currentstepsize
                            } else {
                                recorder.notes4[iu].timestamp
                            };
                            println!("{} {}", recorder.notes4[iu].timestamp, newstep);
                            if recorder.notes4p[iu].pitch == selectpitch {
                                recorder.notes4p[iu].timestamp = newstep;
                                recorder.notes4p[iu].pitch = 72;
                            }
                            count += 1.0;
                        }
                        let ins = recorder.notes4p.clone();
                        recorder.notes4.splice(0..0, ins);
                        sort_notes_by_timestamp(&mut recorder.notes4);
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Num2 {
                        println!("Geben Sie den Anfangsindex ein: ");
                        let start_index2: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        println!("Geben Sie den Endindex ein: ");
                        let end_index2: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        println!("Geben Sie den Anfangsindexnum ein: ");
                        let start_indexnumstart: f32 = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("Geben Sie den Endindexnum ein: ");
                        let end_indexnumstart: f32 = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("nach timestamp 1 , vor nächstem timestamp 2");
                        let vornachtimestamp: i32 = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        let stepsi = if !recorder.total_di {
                            end_index2 - start_index2
                        } else {
                            1
                        };
                        let stepsin = (1.0 / end_indexnumstart) - (1.0 / start_indexnumstart);
                        let stepsizeadd = if stepsi != 0 {
                            stepsin / stepsi as f32
                        } else {
                            0.0
                        };
                        recorder.notes4p.clear();
                        recorder.notes4p = recorder.notes4.clone();
                        let n4len = recorder.notes4.len();
                        let mut count: f32 = 0.0;
                        for i in start_index2..end_index2 {
                            let iu = i as usize;
                            if iu + 1 >= n4len || iu >= recorder.notes4p.len() {
                                count += 1.0;
                                continue;
                            }
                            let currentstepsize =
                                recorder.notes4[iu + 1].timestamp - recorder.notes4[iu].timestamp;
                            let newstep = if vornachtimestamp == 1 {
                                recorder.notes4[iu].timestamp
                                    + ((1.0 / start_indexnumstart) + (count * stepsizeadd))
                                        * currentstepsize
                            } else if vornachtimestamp == 2 {
                                recorder.notes4[iu + 1].timestamp
                                    - ((1.0 / start_indexnumstart) + (count * stepsizeadd))
                                        * currentstepsize
                            } else {
                                recorder.notes4[iu].timestamp
                            };
                            recorder.notes4p[iu].timestamp = newstep;
                            count += 1.0;
                            println!("{} {}", recorder.notes4[iu].timestamp, newstep);
                        }
                        let si = start_index2.clamp(0, recorder.notes4.len() as i32) as usize;
                        let ins = recorder.notes4p.clone();
                        recorder.notes4.splice(si..si, ins);
                        println!("insert");
                        sort_notes_by_timestamp(&mut recorder.notes4);
                        println!("sorted");
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Num5 {
                        println!("pnotes");
                        let num_s = recorder.eingabe(&mut pump);
                        let num: usize = num_s.parse::<usize>().unwrap_or(1).saturating_sub(1);
                        if num >= recorder.pnotes.len() {
                            println!("Pattern {num} ungültig");
                            kpressed = false;
                            continue;
                        }
                        recorder.notes4p.clear();
                        recorder.notes4p = recorder.notes4.clone();
                        let n4len = recorder.notes4.len();
                        let pn_len = recorder.pnotes[num].len();
                        for i in 0..n4len {
                            if i + 1 >= n4len || i + 1 >= pn_len || i >= pn_len {
                                continue;
                            }
                            let currentstepsizefactor = 1.0 / (recorder.notes4[i + 1].timestamp
                                - recorder.notes4[i].timestamp);
                            let currentpnotestepsize = recorder.pnotes[num][i + 1].timestamp
                                - recorder.pnotes[num][i].timestamp;
                            let newstep = currentpnotestepsize * currentstepsizefactor;
                            recorder.notes4p[i + 1].timestamp =
                                recorder.pnotes[num][i].timestamp + newstep;
                        }
                        recorder.notes4 = recorder.notes4p.clone();
                        let pn = recorder.pnotes[num].clone();
                        recorder.notes4.splice(0..0, pn);
                        sort_notes_by_timestamp(&mut recorder.notes4);
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Num6 {
                        let mut n4 = std::mem::take(&mut recorder.notes4);
                        recorder.removeduplicates_orig(&mut n4);
                        recorder.notes4 = n4;
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Num7 {
                        recorder.anfend_index(&mut pump);
                        let st = recorder.start_time;
                        let et = recorder.end_time;
                        recorder.total_durationbreak(st, et);
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Num8 {
                        recorder.total_di = !recorder.total_di;
                        println!("totalDi: {}", recorder.total_di);
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Down {
                        recorder.anfend_index(&mut pump);
                        recorder.kk = true;
                        recorder.remove();
                        let st = recorder.start_time;
                        let et = recorder.end_time;
                        println!("startTime {} endTime {} totalDuration {}", st, et, st - et);
                        println!("Geben Sie den Anfangsindexnum ein: ");
                        recorder.start_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("Geben Sie den Endindexnum ein: ");
                        recorder.end_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        if !recorder.total_di {
                            recorder.indexnum_laenge =
                                1.0 - (1.0 / recorder.start_indexnum) - (1.0 / recorder.end_indexnum);
                        } else {
                            recorder.indexnum_laenge = (et - st)
                                - (1.0 / recorder.start_indexnum)
                                - (1.0 / recorder.end_indexnum);
                        }
                        println!("Indexnumlaenge: {}", recorder.indexnum_laenge);
                        println!(
                            "Geben Sie den schrittweitanz ein zwischen: {} und {}",
                            recorder.indexnum_laenge / (1.0 / recorder.start_indexnum),
                            recorder.indexnum_laenge / (1.0 / recorder.end_indexnum)
                        );
                        recorder.schrittweitanz = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        let mut new_notes = std::mem::take(&mut recorder.new_notes);
                        recorder.scale_to_range_num(
                            &mut new_notes,
                            st,
                            et,
                            recorder.start_indexnum,
                            recorder.end_indexnum,
                            recorder.schrittweitanz,
                            recorder.firstrec,
                        );
                        recorder.new_notes = new_notes;
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Up {
                        recorder.anfend_index(&mut pump);
                        recorder.kk = false;
                        recorder.remove();
                        let st = recorder.start_time;
                        let et = recorder.end_time;
                        println!("startTime {} endTime {} totalDuration {}", st, et, st - et);
                        println!("Geben Sie den Anfangsindexnum ein: ");
                        recorder.start_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("Geben Sie den Endindexnum ein: ");
                        recorder.end_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        if !recorder.total_di {
                            recorder.indexnum_laenge = 1.0;
                        } else {
                            recorder.indexnum_laenge = (et - st) - (1.0 / recorder.start_indexnum);
                        }
                        println!("Indexnumlaenge: {}", recorder.indexnum_laenge);
                        println!(
                            "Geben Sie den schrittweitanz ein zwischen: {} und {}",
                            recorder.indexnum_laenge / (1.0 / recorder.start_indexnum),
                            recorder.indexnum_laenge / (1.0 / recorder.end_indexnum)
                        );
                        recorder.schrittweitanz = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        let mut new_notes = std::mem::take(&mut recorder.new_notes);
                        recorder.scale_to_range_num(
                            &mut new_notes,
                            st,
                            et,
                            recorder.start_indexnum,
                            recorder.end_indexnum,
                            recorder.schrittweitanz,
                            recorder.firstrec,
                        );
                        recorder.new_notes = new_notes;
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Left {
                        recorder.anfend_index(&mut pump);
                        recorder.kl = true;
                        recorder.remove();
                        let st = recorder.start_time;
                        let et = recorder.end_time;
                        println!("startTime {} endTime {} totalDuration {}", st, et, st - et);
                        println!("Geben Sie den Anfangsindexnum ein: ");
                        recorder.start_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("Geben Sie den Endindexnum ein: ");
                        recorder.end_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        if !recorder.total_di {
                            recorder.indexnum_laenge = 1.0 - (1.0 / recorder.start_indexnum);
                        } else {
                            recorder.indexnum_laenge = (et - st) - (1.0 / recorder.start_indexnum);
                        }
                        println!("Indexnumlaenge: {}", recorder.indexnum_laenge);
                        println!(
                            "Geben Sie den schrittweitanz ein zwischen: {} und {}",
                            recorder.indexnum_laenge / (1.0 / recorder.start_indexnum),
                            recorder.indexnum_laenge / (1.0 / recorder.end_indexnum)
                        );
                        recorder.schrittweitanz = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        let mut new_notes = std::mem::take(&mut recorder.new_notes);
                        recorder.scale_to_range_num(
                            &mut new_notes,
                            st,
                            et,
                            recorder.start_indexnum,
                            recorder.end_indexnum,
                            recorder.schrittweitanz,
                            recorder.firstrec,
                        );
                        recorder.new_notes = new_notes;
                        kpressed = false;
                    } else if kpressed && kc == Keycode::Right {
                        recorder.anfend_index(&mut pump);
                        recorder.kr = true;
                        recorder.remove();
                        let st = recorder.start_time;
                        let et = recorder.end_time;
                        println!("startTime {} endTime {} totalDuration {}", st, et, st - et);
                        println!("Geben Sie den Anfangsindexnum ein: ");
                        recorder.start_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        println!("Geben Sie den Endindexnum ein: ");
                        recorder.end_indexnum = recorder.eingabe(&mut pump).parse().unwrap_or(0.0);
                        if !recorder.total_di {
                            recorder.indexnum_laenge = 1.0 - (1.0 / recorder.end_indexnum);
                        } else {
                            recorder.indexnum_laenge = (et - st) - (1.0 / recorder.end_indexnum);
                        }
                        println!("Indexnumlaenge: {}", recorder.indexnum_laenge);
                        println!(
                            "Geben Sie den schrittweitanz ein zwischen: {} und {}",
                            recorder.indexnum_laenge / (1.0 / recorder.start_indexnum),
                            recorder.indexnum_laenge / (1.0 / recorder.end_indexnum)
                        );
                        recorder.schrittweitanz = recorder.eingabe(&mut pump).parse().unwrap_or(0);
                        let mut new_notes = std::mem::take(&mut recorder.new_notes);
                        recorder.scale_to_range_num(
                            &mut new_notes,
                            st,
                            et,
                            recorder.start_indexnum,
                            recorder.end_indexnum,
                            recorder.schrittweitanz,
                            recorder.firstrec,
                        );
                        recorder.new_notes = new_notes;
                        kpressed = false;
                    } else if kc == Keycode::G {
                        recorder.create_graph_window(&mut pump);
                    } else if kc == Keycode::Z {
                        recorder.paste_renoise_pattern(&mut pump);
                    } else if kc == Keycode::R {
                        record_flag = !record_flag;
                        println!("record: {}", record_flag);
                    } else if record_flag && kc == Keycode::Num1 {
                        recorder.clear_notes();
                        recorder.record(&mut pump);
                        record_flag = false;
                    } else if kc == Keycode::Num2 {
                        recorder.clear_notes();
                        println!("Geben Sie die Anzahl der Noten ein: ");
                        let svalue = recorder.eingabe(&mut pump);
                        let nval: i32 = svalue.parse().unwrap_or(0);
                        for i in 0..=nval {
                            recorder.notes4.push(Note3::new(
                                (1.0 / nval.max(1) as f32) * i as f32,
                                0,
                                59 + i,
                            ));
                        }
                        record_flag = false;
                    } else if kc == Keycode::W {
                        let svalue = recorder.eingabe(&mut pump);
                        recorder.plsl = svalue.parse().unwrap_or(0);
                        println!("plsl: {}", recorder.plsl);
                    } else if kc == Keycode::T {
                        tpressed = true;
                    } else if tpressed && (kc == Keycode::Up || kc == Keycode::Down) {
                        if kc == Keycode::Down {
                            recorder.bpmtap(&mut pump);
                        } else {
                            let svalue = recorder.eingabe(&mut pump);
                            let value: f32 = svalue.parse().unwrap_or(0.0);
                            recorder.sd_mut().bpm = value;
                            recorder.set_bpm();
                            recorder.zeitfaktor();
                            println!("BPM: {}", recorder.sd().bpm);
                        }
                        tpressed = false;
                    } else if kc == Keycode::Q {
                        q_pressed = true;
                    } else if q_pressed && (kc == Keycode::Num3 || kc == Keycode::Num4) {
                        println!(
                            "Geben Sie den Quantisierungswert ein (z.B. 16 für 16tel, 32 für 32tel): "
                        );
                        let value = recorder.eingabe(&mut pump);
                        let quantization: i32 = value.parse().unwrap_or(0);
                        if kc == Keycode::Num3 {
                            let mut n3 = std::mem::take(&mut recorder.notes3);
                            recorder.quantize_notes(&mut n3, quantization);
                            recorder.notes3 = n3;
                            println!("notes3 quantisiert mit {}-tel", quantization);
                        } else {
                            let mut n4 = std::mem::take(&mut recorder.notes4);
                            recorder.quantize_notes(&mut n4, quantization);
                            recorder.notes4 = n4;
                            println!("notes4 quantisiert mit {}-tel", quantization);
                        }
                        q_pressed = false;
                    } else if kc == Keycode::M {
                        recorder.modul1(&mut pump);
                    } else if kc == Keycode::D {
                        sdlkd = true;
                    } else if sdlkd && kc == Keycode::Num1 {
                        recorder.dis_pl_notes();
                        sdlkd = false;
                    } else if sdlkd && kc == Keycode::Num2 {
                        sdlkd = false;
                    } else if sdlkd && kc == Keycode::Num3 {
                        recorder.notes5.clear();
                        recorder.notes4s.clear();
                        println!("Anz Interpolationen am Anfang?");
                        let svalue1a = recorder.eingabe(&mut pump);
                        println!("Anz Interpolationen am Ende?");
                        let svalue1e = recorder.eingabe(&mut pump);
                        println!("x*Pi?");
                        let svalue11 = recorder.eingabe(&mut pump);
                        println!("1/x*Pi?");
                        let svalue12 = recorder.eingabe(&mut pump);
                        println!("Oszillationsamplitude in Halbtontasten?");
                        let svalue111 = recorder.eingabe(&mut pump);
                        println!("Curve? 1 = / 2 = \\ 3 = between the timestamps");
                        let svalue1111 = recorder.eingabe(&mut pump);
                        let sa: i32 = svalue1a.parse().unwrap_or(0);
                        let se: i32 = svalue1e.parse().unwrap_or(0);
                        let pi: f32 = svalue11.parse::<f32>().unwrap_or(1.0)
                            / svalue12.parse::<f32>().unwrap_or(1.0);
                        let amp: f32 = svalue111.parse().unwrap_or(0.0);
                        let curve: i32 = svalue1111.parse().unwrap_or(0);
                        let n4 = recorder.notes4.clone();
                        let notes5 = recorder.interpolate_notes_with_sinus_pitch(
                            &mut pump, &n4, sa, se, pi, amp, curve,
                        );
                        recorder.notes5 = notes5.clone();
                        recorder.notes4 = notes5;
                        sdlkd = false;
                    } else if sdlkd && kc == Keycode::Num4 {
                        recorder.notes4 = recorder.notes4s.clone();
                        sdlkd = false;
                    } else if sdlkd && kc == Keycode::Num5 {
                        recorder.vmidi = !recorder.vmidi;
                        sdlkd = false;
                    } else if sdlkd && kc == Keycode::Num6 {
                        recorder.notes5.clear();
                        recorder.notes4s.clear();
                        println!("Anz Pattern?");
                        let svaluept = recorder.eingabe(&mut pump);
                        playpatanz = svaluept.parse().unwrap_or(0);
                        println!("Anz Interpolationen am Anfang?");
                        let svalue1a = recorder.eingabe(&mut pump);
                        println!("Anz Interpolationen am Ende?");
                        let svalue1e = recorder.eingabe(&mut pump);
                        println!("x*Pi?");
                        let svalue11 = recorder.eingabe(&mut pump);
                        println!("1/x*Pi?");
                        let svalue12 = recorder.eingabe(&mut pump);
                        println!("Oszillationsamplitude in Halbtontasten?");
                        let svalue111 = recorder.eingabe(&mut pump);
                        println!("Curve? 1 = < 2 = > 3 = between the timestamps");
                        let svalue1111 = recorder.eingabe(&mut pump);
                        let sa: i32 = svalue1a.parse().unwrap_or(0);
                        let se: i32 = svalue1e.parse().unwrap_or(0);
                        let interp = se - sa;
                        let interpadd = interp as f32 / playpatanz.max(1) as f32;
                        let pi_ratio: f32 = svalue11.parse().unwrap_or(0.0)
                            / svalue12.parse::<f32>().unwrap_or(1.0);
                        let osz: i32 = svalue111.parse().unwrap_or(0);
                        let oszadd = osz as f32 / playpatanz.max(1) as f32;
                        let curve: i32 = svalue1111.parse().unwrap_or(0);
                        for i in 0..8 {
                            recorder.notes55[i].clear();
                        }
                        let n4 = recorder.notes4.clone();
                        let patanz = playpatanz.clamp(0, 8) as usize;
                        for i in 0..patanz {
                            recorder.notes55[i] = recorder.interpolate_notes_with_sinus_pitch(
                                &mut pump,
                                &n4,
                                sa + (i as f32 * interpadd) as i32,
                                sa + ((i + 1) as f32 * interpadd) as i32,
                                pi_ratio as i32 as f32,
                                (oszadd * (i as f32 + 1.0)) as i32 as f32,
                                curve,
                            );
                        }
                        for i in 0..patanz {
                            for ii in 0..recorder.notes55[i].len() {
                                let n = &recorder.notes55[i][ii];
                                println!(
                                    "{} {} {}",
                                    n.timestamp, n.instrument, n.pitch
                                );
                            }
                        }
                        sdlkd = false;
                    } else if kc == Keycode::O {
                        if playrenoise {
                            recorder.toggle_renoise_play();
                            recorder.play2();
                            recorder.toggle_renoise_play();
                        } else {
                            recorder.play2();
                        }
                    } else if kc == Keycode::J {
                        present = true;
                        println!("jpresav");
                    } else if present && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        println!("pnotes{}", num_i);
                        recorder.pnotes[num_i as usize] = recorder.notes4.clone();
                        for note in &recorder.pnotes[num_i as usize] {
                            println!(
                                "{},{},{},{}",
                                note.timestamp, note.instrument, note.pitch, note.p2
                            );
                        }
                        let dauer = recorder.sd().dauer;
                        recorder.pdauer[num_i as usize][0] = dauer;
                        println!("{}", recorder.pdauer[num_i as usize][0]);
                        present = false;
                    } else if kc == Keycode::H {
                        println!("playp on");
                        playp = true;
                    } else if playp && kc == Keycode::Num0 {
                        let p = playpatanz.clamp(0, 8) as usize;
                        if playrenoise {
                            recorder.toggle_renoise_play();
                            for i in 0..p {
                                recorder.play2add(i);
                            }
                            recorder.toggle_renoise_play();
                        } else {
                            for i in 0..p {
                                recorder.play2add(i);
                            }
                        }
                    } else if playp && kc == Keycode::M {
                        playpm = !playpm;
                        println!("playpm on/off{}", playpm);
                    } else if playp && !playpm && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        println!("playnum: {}", num_i);
                        let bpm = recorder.sd().bpm;
                        let sixteenth = (60.0f32 / bpm) * 1000.0 / 4.0;
                        let current_time = ticks();
                        let next_sixteenth =
                            current_time + (sixteenth as u32 - (current_time % sixteenth as u32));
                        let delay = next_sixteenth.saturating_sub(current_time);
                        thread::sleep(Duration::from_millis(delay as u64));
                        recorder.play4(num_i as usize);
                    } else if playp && playpm && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        println!("playnum: {}", num_i);
                        recorder.play34(num_i as usize);
                    } else if (playp || playpm) && kc == Keycode::Num9 {
                        playp = false;
                        playpm = false;
                        println!("playp/m off");
                    } else if kc == Keycode::S {
                        sbool = !sbool;
                        println!("sbool: {}", sbool);
                    } else if sbool && kc == Keycode::Up {
                        sboolinsert = !sboolinsert;
                        println!("sboolinsert: {}", sboolinsert);
                    } else if sboolinsert && sbool && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        recorder.vectorinsert(num_i as usize);
                        sboolinsert = false;
                        sbool = false;
                    } else if sbool && kc == Keycode::Down {
                        sboolx = !sboolx;
                        println!("sboolx: {}", sboolx);
                    } else if sbool && sboolx && kc == Keycode::Left {
                        println!("convert notes to Renoise pattern with length?");
                        let length = recorder.eingabe(&mut pump);
                        recorder
                            .convert_notes_to_renoise_pattern(length.parse().unwrap_or(0));
                        sboolx = false;
                        sbool = false;
                    } else if sbool && sboolx && kc == Keycode::Right {
                        println!("convert notes to Schism pattern with length?");
                        let length = recorder.eingabe(&mut pump);
                        recorder.convert_notes_to_schism_pattern_with_input(
                            length.parse().unwrap_or(0),
                            &mut pump,
                        );
                        sboolx = false;
                        sbool = false;
                    } else if sbool && kc == Keycode::Num0 {
                        save = !save;
                        sbool = false;
                    } else if sbool && kc == Keycode::U {
                        println!("Shared Memory Update Delay in Millisekunden eingeben:");
                        let delay_value = recorder.eingabe(&mut pump);
                        recorder
                            .set_shared_memory_update_delay(delay_value.parse().unwrap_or(0));
                        sbool = false;
                    } else if sbool && kc == Keycode::I {
                        println!("snotes3 on/off");
                        snotes4 = !snotes4;
                    } else if sbool && kc == Keycode::Num9 {
                        for i in 0..8 {
                            recorder.save_to_csv(&csv_filename_for_slot(i), i);
                            println!("Pattern {} saved to pattern.csv", i);
                        }
                        sbool = false;
                    } else if snotes4 && sbool && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        recorder.notes4 = recorder.pnotes[num_i as usize].clone();
                        println!("Pattern {} notes4", num_i);
                        snotes4 = false;
                        sbool = false;
                    } else if sbool && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        recorder.save_to_csv(&format!("pattern {}.csv", num_i), num_i as usize);
                        println!("Pattern {} saved to pattern.csv", num_i);
                        sbool = false;
                    } else if kc == Keycode::F {
                        dbool = true;
                    } else if dbool && kc == Keycode::Num9 {
                        for i in 0..8 {
                            recorder.read_from_csv(&csv_filename_for_slot(i), i);
                            println!("Pattern {} loaded from pattern.csv", i);
                        }
                        dbool = false;
                    } else if dbool && digit_1_8 {
                        let num_i = d.unwrap() - 1;
                        recorder.read_from_csv(&format!("pattern {}.csv", num_i), num_i as usize);
                        println!("Pattern {} loaded from pattern.csv", num_i);
                        dbool = false;
                    } else if kc == Keycode::C {
                        recorder.clear_notes();
                    } else if kc == Keycode::Y {
                        println!("Pattern Select Mode: Press 1-8 to play pattern with sound, 9 to exit");
                        let mut pattern_select_mode = true;
                        while pattern_select_mode {
                            if let Some(ev) = poll_event_compat() {
                                if let Event::KeyDown {
                                    keycode: Some(kc2),
                                    ..
                                } = ev
                                {
                                    if let Some(d2) = digit_value(kc2) {
                                        if (1..=8).contains(&d2) {
                                            let pattern_num = d2 - 1;
                                            println!("Playing pattern {} with sound", pattern_num + 1);
                                            recorder.play_pattern_as_sound(pattern_num as i32);
                                        } else if d2 == 9 {
                                            println!("Exiting pattern select mode");
                                            pattern_select_mode = false;
                                            break;
                                        }
                                    }
                                }
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                    } else if kc == Keycode::X {
                        recorder.clear_notes2();
                    } else if kc == Keycode::B {
                        ntr = !ntr;
                        println!("ntr:{}", ntr);
                    } else if ntr && kc == Keycode::PageUp {
                        recorder.transform_timestamps(&mut pump);
                        ntr = !ntr;
                    } else if ntr && kc == Keycode::PageDown {
                        recorder.reverse_transform_timestamps(&mut pump);
                        ntr = !ntr;
                    } else if ntr && kc == Keycode::Down {
                        recorder.recntr(&mut pump);
                        ntr = !ntr;
                    } else if ntr && kc == Keycode::Right {
                        recorder.recntrf(&mut pump);
                        ntr = !ntr;
                    } else if ntr && kc == Keycode::Left {
                        recorder.notelenght(&mut pump);
                        ntr = !ntr;
                    } else if ntr && kc == Keycode::Up {
                        println!("Glide Rate?");
                        recorder.noteglide(&mut pump);
                        ntr = !ntr;
                    } else if kc == Keycode::V {
                        recorder.fdx(&mut pump);
                    } else if kc == Keycode::E {
                        recorder.anfend_index(&mut pump);
                        recorder.remove_and_record(&mut pump);
                    } else if !lbeats && kc == Keycode::P {
                        println!("Set Pattern Dauer für Pattern?");
                        let svalue = recorder.eingabe(&mut pump);
                        let n = svalue.parse::<usize>().unwrap_or(1).saturating_sub(1);
                        recorder.setpdauer(n, &mut pump);
                    } else if kc == Keycode::L {
                        lbeats = !lbeats;
                    } else if lbeats && kc == Keycode::P {
                        let svalue = recorder.eingabe(&mut pump);
                        let dv: f32 = svalue.parse().unwrap_or(0.0);
                        recorder.sd_mut().dauer = dv;
                        println!("Dauer in Beats? {}", recorder.sd().dauer);
                        recorder.zeitfaktor();
                        lbeats = false;
                    } else if lbeats && kc == Keycode::Left {
                        let bd = recorder.sd().dauer / 2.0;
                        recorder.sd_mut().dauer = bd;
                        recorder.zeitfaktor();
                        lbeats = false;
                    } else if lbeats && kc == Keycode::Right {
                        let bd = recorder.sd().dauer * 2.0;
                        recorder.sd_mut().dauer = bd;
                        recorder.zeitfaktor();
                        lbeats = false;
                    } else if lbeats && kc == Keycode::Up {
                        let bd = recorder.sd().dauer / 3.0;
                        recorder.sd_mut().dauer = bd;
                        recorder.zeitfaktor();
                        lbeats = false;
                    } else if lbeats && kc == Keycode::Down {
                        let bd = recorder.sd().dauer * 3.0;
                        recorder.sd_mut().dauer = bd;
                        recorder.zeitfaktor();
                        lbeats = false;
                    } else if kc == Keycode::U {
                        let svalue = recorder.eingabe(&mut pump);
                        recorder.loops = svalue.parse().unwrap_or(0);
                        println!("Loops {}", recorder.loops);
                    } else if kc == Keycode::F5 {
                        recorder.toggle_renoise_play();
                    } else if kc == Keycode::F8 {
                        playrenoise = !playrenoise;
                    } else if kc == Keycode::F9 {
                        recorder.toggle_renoise_record();
                    } else if kc == Keycode::F2 {
                        println!("Select pnotes[num]");
                        let svalue = recorder.eingabe(&mut pump);
                        let n = svalue.parse::<usize>().unwrap_or(1).saturating_sub(1);
                        recorder.select(n, &mut pump);
                    } else if kc == Keycode::F3 {
                        println!("Select for pnotes[num] to reverse");
                        let svalue = recorder.eingabe(&mut pump);
                        let n = svalue.parse::<usize>().unwrap_or(1).saturating_sub(1);
                        recorder.reverse(n);
                    } else if kc == Keycode::F4 {
                        println!("Select pnotes[num] to operate");
                        let svalue = recorder.eingabe(&mut pump);
                        let n = svalue.parse::<usize>().unwrap_or(1).saturating_sub(1);
                        println!("1 *, 2 /, 3 +, 4 -");
                        let svalue = recorder.eingabe(&mut pump);
                        let operate: i32 = svalue.parse().unwrap_or(0);
                        println!("Faktor: ?");
                        let svalue = recorder.eingabe(&mut pump);
                        recorder.factor = svalue.parse().unwrap_or(0.0);
                        let f = recorder.factor;
                        match operate {
                            1 => recorder.mal(n, f),
                            2 => recorder.durch(n, f),
                            3 => recorder.plus(n, f),
                            4 => recorder.minus(n, f),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if recorder.sd().p1 {
            if playrenoise {
                recorder.toggle_renoise_play();
                recorder.play();
                recorder.toggle_renoise_play();
            } else {
                recorder.play();
            }
        } else if recorder.sd().p2 {
            if playrenoise {
                recorder.toggle_renoise_play();
                recorder.play2();
                recorder.toggle_renoise_play();
            } else {
                recorder.play2();
            }
        } else if recorder.sd().p3 {
            if playrenoise {
                recorder.toggle_renoise_play();
                recorder.play3();
                recorder.toggle_renoise_play();
            } else {
                recorder.play3();
            }
        }

        if save {
            println!("Width?");
            let svalue = recorder.eingabe(&mut pump);
            let width: i32 = svalue.parse().unwrap_or(0);
            println!("Start?");
            let svalue = recorder.eingabe(&mut pump);
            let widthstart: i32 = svalue.parse().unwrap_or(0);
            let loops = recorder.loops.max(1);
            let width_per_loop = width / loops;
            let Ok(mut file) = std::fs::File::create("./KeyEventRender.txt") else {
                println!("could not create file");
                save = false;
                continue;
            };
            for la in 0..loops {
                if widthstart > 0 {
                    for widthfill in 0..widthstart {
                        writeln!(file, "{};{};{};{}", widthfill, 0, 0, 0).ok();
                    }
                }
                let mut ii: usize = 0;
                let mut iii = 0;
                for _a in 0..width_per_loop.max(0) {
                    let a = _a as f32;
                    let ts = recorder
                        .notes4
                        .get(ii)
                        .map(|n| n.timestamp)
                        .unwrap_or(0.0);
                    println!(
                        "{}  {}  {}  {}",
                        a + (la as f32 * width as f32 / loops as f32),
                        a,
                        ts * width as f32,
                        ii
                    );
                    if a >= (ts * width as f32 / loops as f32) {
                        iii = 1;
                        println!("set {} {}", a + (width as f32 * la as f32), ts * a);
                        if ii + 1 < recorder.notes4.len() {
                            ii += 1;
                        }
                    } else {
                        iii = 0;
                    }
                    writeln!(
                        file,
                        "{};{};{};{}",
                        widthstart as f32 + a + (width as f32 / loops as f32) * la as f32,
                        iii,
                        iii,
                        iii
                    )
                    .ok();
                }
            }
            println!("txt saved");
            save = !save;
        }

        renderer.set_draw_color(Color::RGB(0, 0, 0));
        let _ = renderer.clear();
        recorder.display(&mut renderer, &ttf_context);
        renderer.present();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_keydown(sym: i32) {
        unsafe {
            let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
            raw.key.type_ = sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32;
            raw.key.timestamp = 0;
            raw.key.windowID = 1;
            raw.key.state = 1; // SDL_PRESSED
            raw.key.repeat = 0;
            raw.key.keysym.sym = sym;
            sdl2::sys::SDL_PushEvent(&mut raw);
        }
    }

    #[test]
    fn midi_to_renoise_note_handles_off_and_valid_pitches() {
        assert_eq!(midi_to_renoise_note(-1), "Off");
        assert_eq!(midi_to_renoise_note(60), "C-4");
        assert_eq!(midi_to_renoise_note(0), "C--1");
    }

    #[test]
    fn remove_and_record_maps_fkeys_and_digits() {
        assert_eq!(remove_and_record_note_value(Keycode::F1, false), Some(10));
        assert_eq!(remove_and_record_note_value(Keycode::F10, false), Some(19));
        assert_eq!(remove_and_record_note_value(Keycode::F5, true), Some(14));
        assert_eq!(remove_and_record_note_value(Keycode::Num7, false), Some(7));
        assert_eq!(remove_and_record_note_value(Keycode::Num7, true), Some(1));
        assert_eq!(remove_and_record_note_value(Keycode::PageUp, false), None);
        assert_eq!(remove_and_record_note_value(Keycode::Num0, false), None);
    }

    #[test]
    fn renoise_note_midi_roundtrip() {
        assert_eq!(renoise_note_to_midi("C-4"), 60);
        assert_eq!(renoise_note_to_midi("E-4"), 64);
        assert_eq!(renoise_note_to_midi("C-5"), 72);
        assert_eq!(renoise_note_to_midi("Off"), -1);
        assert_eq!(midi_to_renoise_note(renoise_note_to_midi("C-4")), "C-4");
        assert_eq!(midi_to_renoise_note(renoise_note_to_midi("A#3")), "A#3");
    }

    #[test]
    fn append_input_token_allows_single_decimal_point() {
        let mut s = String::new();
        append_input_token(&mut s, '1');
        append_input_token(&mut s, '.');
        append_input_token(&mut s, '5');
        append_input_token(&mut s, '.');
        assert_eq!(s, "1.5");
    }

    #[test]
    fn sinus_interpolate_inserts_between_notes() {
        let notes = vec![
            Note3::new(0.0, 0, 60),
            Note3::new(0.5, 0, 62),
            Note3::new(1.0, 0, 64),
        ];
        let out = sinus_interpolate(&notes, 0, 2, 1, 1, 1.0, 0.0, 3);
        assert_eq!(out.len(), 5);
        assert!((out[1].timestamp - 0.25).abs() < 1e-6);
        assert!((out[3].timestamp - 0.75).abs() < 1e-6);
        assert_eq!(out[1].pitch, 60);
        assert_eq!(out[3].pitch, 62);
        assert_eq!(out[4].pitch, 64);
        assert!(sinus_interpolate(&[], 0, 0, 1, 1, 1.0, 0.0, 3).is_empty());
    }

    #[test]
    fn sinus_interpolate_degrades_gracefully_on_bad_range() {
        let notes = vec![Note3::new(0.0, 0, 60), Note3::new(1.0, 0, 62)];
        let out = sinus_interpolate(&notes, 1, 0, 1, 1, 1.0, 0.0, 3);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pitch, 60);
    }

    #[test]
    fn interpolate_step_graph_is_piecewise_linear() {
        let out = interpolate_step_graph(&[(0, 0), (10, 10)]);
        assert_eq!(out.len(), 11);
        assert_eq!(out[5], (5, 5));
        assert_eq!(out[10], (10, 10));
        let out = interpolate_step_graph(&[(10, 10), (20, 0)]);
        assert_eq!(out.len(), 21);
        assert_eq!(out[15], (15, 5));
        assert_eq!(out[20], (20, 0));
        assert!(interpolate_step_graph(&[]).is_empty());
    }

    #[test]
    fn ae_points_samples_by_step_height() {
        let stepxy = vec![(0, 2), (1, 2), (2, 2), (3, 2), (4, 2), (5, 2)];
        assert_eq!(ae_points(&stepxy), vec![(0, 2), (2, 2), (4, 2)]);
        assert!(ae_points(&[]).is_empty());
        let with_zero = vec![(0, 0), (1, 0), (2, 3), (3, 3)];
        assert_eq!(ae_points(&with_zero), vec![(2, 3)]);
    }

    #[test]
    fn graph_points_to_notes_maps_time_and_pitch() {
        let notes = graph_points_to_notes(&[(0, 10), (10, 50), (20, 30)], 1.0, 3.0);
        assert_eq!(notes.len(), 2);
        assert!((notes[0].timestamp - 1.0).abs() < 1e-6);
        assert_eq!(notes[0].pitch, 61);
        assert_eq!(notes[0].instrument, 30);
        assert!((notes[1].timestamp - 2.0).abs() < 1e-6);
        assert_eq!(notes[1].pitch, 65);
        assert!(graph_points_to_notes(&[], 0.0, 1.0).is_empty());
        assert!(graph_points_to_notes(&[(0, 5)], 0.0, 1.0).is_empty());
    }

    #[test]
    fn safe_divisor_avoids_nan_and_inf() {
        assert_eq!(safe_divisor(0.0), 1.0);
        assert_eq!(safe_divisor(f32::NAN), 1.0);
        assert_eq!(safe_divisor(2.5), 2.5);
        assert_eq!(safe_divisor(-1.0), 1.0);
    }

    #[test]
    fn total_cmp_f32_orders_nan_last() {
        use std::cmp::Ordering;
        assert_eq!(total_cmp_f32(1.0, 2.0), Ordering::Less);
        assert_eq!(total_cmp_f32(2.0, 1.0), Ordering::Greater);
        assert_eq!(total_cmp_f32(1.0, 1.0), Ordering::Equal);
        assert_eq!(total_cmp_f32(f32::NAN, 2.0), Ordering::Greater);
        assert_eq!(total_cmp_f32(2.0, f32::NAN), Ordering::Less);
        assert_eq!(total_cmp_f32(f32::NAN, f32::NAN), Ordering::Equal);
    }

    #[test]
    fn sort_by_timestamp_tolerates_nan() {
        let mut notes = vec![note(1.0), note(f32::NAN), note(0.5)];
        sort_notes_by_timestamp(&mut notes);
        assert_eq!(notes[0].timestamp, 0.5);
        assert_eq!(notes[1].timestamp, 1.0);
        assert!(notes[2].timestamp.is_nan());
    }

    #[test]
    fn csv_roundtrip_writes_cpp_bool_format() {
        let notes = vec![
            Note3 {
                timestamp: 0.5,
                instrument: 1,
                pitch: 60,
                glide: true,
                p2: 0.0,
            },
            Note3 {
                timestamp: 1.25,
                instrument: 2,
                pitch: 62,
                glide: false,
                p2: 0.0,
            },
        ];
        let csv = notes_to_csv(&notes);
        assert_eq!(csv, "0.5,1,60,1\n1.25,2,62,0\n");
        assert_eq!(notes_from_csv(&csv), notes);
        let tolerant = notes_from_csv("0.5,1,60,true\n1.25,2,62,false\n");
        assert_eq!(tolerant, notes);
        assert!(notes_from_csv("kaputt\n").is_empty());
    }

    #[test]
    fn csv_bulk_filenames_follow_slot_index() {
        assert_eq!(csv_filename_for_slot(0), "pattern 0.csv");
        assert_eq!(csv_filename_for_slot(7), "pattern 7.csv");
    }

    const RENOISE_XML: &str = r#"<?xml version="1.0"?>
<PatternClipboard.BlockBuffer doc_version="0">
<Columns>
<Column>
<Column>
<Lines>
<Line index="0"/>
<Line index="1"/>
<Line index="2">
<NoteColumns><NoteColumn><Note>C-4</Note><Instrument>00</Instrument></NoteColumn></NoteColumns>
</Line>
<Line index="3"/>
<Line index="4">
<NoteColumns><NoteColumn><Note>D-4</Note><Instrument>01</Instrument></NoteColumn></NoteColumns>
</Line>
</Lines>
<ColumnType>NoteColumn</ColumnType>
</Column>
</Column>
</Columns>
</PatternClipboard.BlockBuffer>
"#;

    #[test]
    fn parse_line_notes_reads_indices_and_skips_self_closing_lines() {
        let lines = parse_line_notes(RENOISE_XML);
        assert_eq!(
            lines,
            vec![
                (2, "C-4".to_string(), "00".to_string()),
                (4, "D-4".to_string(), "01".to_string()),
            ]
        );
    }

    #[test]
    fn parse_line_notes_ignores_second_column() {
        let xml = r#"<PatternClipboard.BlockBuffer doc_version="0">
<Columns>
<Column><Column><Lines>
<Line index="3"><NoteColumns><NoteColumn><Note>E-5</Note><Instrument>02</Instrument></NoteColumn></NoteColumns></Line>
</Lines></Column></Column>
<Column><Column><Lines>
<Line index="0"><NoteColumns><NoteColumn><Note>C-0</Note><Instrument>09</Instrument></NoteColumn></NoteColumns></Line>
</Lines></Column></Column>
</Columns>
</PatternClipboard.BlockBuffer>"#;
        let lines = parse_line_notes(xml);
        assert_eq!(lines, vec![(3, "E-5".to_string(), "02".to_string())]);
    }

    #[test]
    fn parse_line_notes_returns_empty_without_lines() {
        assert!(parse_line_notes("<PatternClipboard.BlockBuffer doc_version=\"0\"/>").is_empty());
        assert!(parse_line_notes("kein xml").is_empty());
    }

    fn note(ts: f32) -> Note3 {
        Note3 {
            timestamp: ts,
            ..Default::default()
        }
    }

    #[test]
    fn play_time_old_handles_short_and_full_patterns() {
        let p = vec![note(0.5), note(1.5), note(2.0)];
        assert_eq!(play_time_old(&p, 0, 1.0), -0.5);
        assert_eq!(play_time_old(&p, 1, 1.0), 0.5);
        assert_eq!(play_time_old(&p, 2, 1.0), 1.5);
        // note_index == len ist am Loop-Ende erlaubt (greift auf die letzte Note zu)
        assert_eq!(play_time_old(&p, 3, 1.0), 2.0);
        assert_eq!(play_time_old(&p, 0, 2.0), -1.0);
        let short = vec![note(1.0)];
        assert_eq!(play_time_old(&short, 0, 1.0), 0.0);
        assert_eq!(play_time_old(&[], 0, 1.0), 0.0);
    }

    #[test]
    fn thread_note_time_scales_by_shared_memory_values() {
        assert_eq!(thread_note_time(2.0, 1.0, 1.0, 1.0), 2.0);
        assert_eq!(thread_note_time(2.0, 2.0, 4.0, 1.0), 1.0);
        assert_eq!(thread_note_time(1.0, 1.0, 1.0, 3.0), 3.0);
    }

    #[test]
    fn loop_cycle_offset_advances_per_loop() {
        let p = vec![note(0.5), note(2.0)];
        assert_eq!(loop_cycle_offset(&p, 0, 1.0, 1.0, 1.0), 0.0);
        assert_eq!(loop_cycle_offset(&p, 1, 1.0, 1.0, 1.0), 2.0);
        assert_eq!(loop_cycle_offset(&p, 3, 1.0, 1.0, 1.0), 6.0);
        assert_eq!(loop_cycle_offset(&[], 2, 1.0, 1.0, 1.0), 0.0);
    }

    #[test]
    fn loop_start_advance_uses_last_timestamp() {
        let p = vec![note(0.5), note(1.5)];
        assert_eq!(loop_start_advance(&p, 2.0), 3.0);
        assert_eq!(loop_start_advance(&[], 2.0), 0.0);
    }

    #[test]
    fn record_key_selects_modes_instead_of_being_swallowed_by_digits() {
        assert_eq!(
            record_key_action(Keycode::S, false, false),
            RecordAction::SelectKk
        );
        assert_eq!(
            record_key_action(Keycode::S, false, true),
            RecordAction::SelectKk
        );
        assert_eq!(
            record_key_action(Keycode::A, false, false),
            RecordAction::ToggleAlleseins
        );
        assert_eq!(
            record_key_action(Keycode::D, false, false),
            RecordAction::SelectKr
        );
        assert_eq!(
            record_key_action(Keycode::W, false, false),
            RecordAction::SelectNone
        );
        assert_eq!(
            record_key_action(Keycode::PageUp, false, false),
            RecordAction::PageUp(true)
        );
        assert_eq!(
            record_key_action(Keycode::PageDown, false, false),
            RecordAction::PageUp(false)
        );
        assert_eq!(
            record_key_action(Keycode::Y, false, false),
            RecordAction::Schritt3
        );
    }

    #[test]
    fn record_key_classifies_notes_by_mode() {
        assert_eq!(
            record_key_action(Keycode::Num5, false, false),
            RecordAction::NoteDigit(5)
        );
        assert_eq!(
            record_key_action(Keycode::Num5, false, true),
            RecordAction::NoteDigit(5)
        );
        assert_eq!(
            record_key_action(Keycode::Num5, true, false),
            RecordAction::NoteFixedOne
        );
        assert_eq!(
            record_key_action(Keycode::F5, false, false),
            RecordAction::NoteFKey(5)
        );
        assert_eq!(
            record_key_action(Keycode::Num0, false, false),
            RecordAction::Stop
        );
    }

    #[test]
    fn record_key_maps_factors_and_step_widths() {
        assert_eq!(
            record_key_action(Keycode::Q, false, false),
            RecordAction::NumFactor(2)
        );
        assert_eq!(
            record_key_action(Keycode::E, false, false),
            RecordAction::NumFactor(4)
        );
        assert_eq!(
            record_key_action(Keycode::X, false, false),
            RecordAction::RStep(7.0)
        );
        assert_eq!(
            record_key_action(Keycode::C, false, false),
            RecordAction::RStep(15.0)
        );
        assert_eq!(
            record_key_action(Keycode::V, false, false),
            RecordAction::RStep(2.0)
        );
        assert_eq!(
            record_key_action(Keycode::B, false, false),
            RecordAction::RStep(5.0)
        );
        assert_eq!(
            record_key_action(Keycode::N, false, false),
            RecordAction::RStep(11.0)
        );
        assert_eq!(
            record_key_action(Keycode::M, false, false),
            RecordAction::RStep(23.0)
        );
        assert_eq!(
            record_key_action(Keycode::Escape, false, false),
            RecordAction::None
        );
    }

    #[test]
    fn compat_poll_delivers_keydown_and_drops_window_events() {
        unsafe {
            sdl2::sys::SDL_Init(0);
            sdl2::sys::SDL_InitSubSystem(sdl2::sys::SDL_INIT_EVENTS);
        }
        // sdl2-compat-leck: SDL3-Window-Events (0x202..0x2FF) dürfen nicht
        // zu einem Panic führen, sondern werden verworfen.
        for ty in 0x202u32..0x21e {
            unsafe {
                let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
                raw.type_ = ty;
                sdl2::sys::SDL_PushEvent(&mut raw);
            }
        }
        push_keydown(113); // SDLK_q
        while let Some(ev) = poll_event_compat() {
            if let Event::KeyDown {
                keycode: Some(kc),
                ..
            } = ev
            {
                assert_eq!(kc, Keycode::Q);
                return;
            }
        }
        panic!("KeyDown wurde nicht zugestellt");
    }
}