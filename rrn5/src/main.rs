// rrn5 – Rust-Port von rrn5.cpp.
// Einzige Änderungen gegenüber der C++-Version:
//   - ein SDL-Fenster (Position 3840-xw, Größe xw x yh); die zweiten/dritten
//     "Selected color"-Renderer sind CPU-Pixelbuffer ((second)/(third)), da
//     SDL2-compat in rrn5.cpp drei Fenster schafft und nur das Hauptfenster
//     wirklich gebraucht wird.
//   - Eingaben (BPM, Harmonien usw.) über stdin statt SDL-Tasteneingabe.
//   - OSC-Empfang (Port 7010) entfällt (in rrn5.cpp auskommentiert).
mod osc;
mod shared;
mod x11cap;

use shared::SharedMemoryEditor;
use sdl2::event::{Event, EventType};
use sdl2::keyboard::{Keycode, Mod, Scancode};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::Window;
use std::time::{Duration, Instant};

/// Test-Hook: E2E_KEYS="Tab,Right" beim Start in die SDL-Queue legen.
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
            "f3" => (K::F3, S::F3),
            "f4" => (K::F4, S::F4),
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

fn mouse_xy() -> (i32, i32) {
    let mut x = 0;
    let mut y = 0;
    unsafe { sdl2::sys::SDL_GetMouseState(&mut x, &mut y) };
    (x, y)
}

const QUINTENZIRKEL: [i32; 12] = [60, 67, 62, 69, 64, 71, 66, 61, 68, 63, 70, 65];
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const RENOISE_NOTE_NAMES: [&str; 12] = [
    "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
];

// ---------------------------------------------------------------------------
// Event-Kompatibilität (SDL2-compat leakt SDL3-Events; nur relevante Typen
// weiterreichen, sonst panict sdl2 0.37 bei Event::from_ll).
// ---------------------------------------------------------------------------
fn is_kept_event(t: u32) -> bool {
    matches!(
        t,
        x if x == EventType::Quit as u32
            || x == EventType::KeyDown as u32
            || x == EventType::KeyUp as u32
            || x == EventType::MouseButtonDown as u32
            || x == EventType::MouseButtonUp as u32
    )
}

fn poll_event_compat() -> Option<Event> {
    unsafe {
        loop {
            let mut raw = std::mem::MaybeUninit::uninit();
            if sdl2::sys::SDL_PollEvent(raw.as_mut_ptr()) != 1 {
                return None;
            }
            let ev = raw.assume_init();
            if is_kept_event(ev.type_) {
                return Some(Event::from_ll(ev));
            }
        }
    }
}

fn wait_event_compat() -> Option<Event> {
    unsafe {
        loop {
            let mut raw = std::mem::MaybeUninit::uninit();
            if sdl2::sys::SDL_WaitEvent(raw.as_mut_ptr()) != 1 {
                return None;
            }
            let ev = raw.assume_init();
            if is_kept_event(ev.type_) {
                return Some(Event::from_ll(ev));
            }
        }
    }
}

fn wait_enter() {
    loop {
        if let Some(ev) = wait_event_compat() {
            if let Event::KeyDown {
                keycode: Some(Keycode::Return),
                ..
            } = ev
            {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// Ein-/Ausgabe-Helfer
// ---------------------------------------------------------------------------
fn read_stdin_f32() -> f32 {
    use std::io::BufRead;
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().parse().unwrap_or(0.0)
}

fn read_stdin_i32() -> i32 {
    use std::io::BufRead;
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().parse().unwrap_or(0)
}

fn read_stdin_str() -> String {
    use std::io::BufRead;
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_string()
}

fn read_filenames(directory: &str) -> Vec<String> {
    filter_image_files(directory)
}

/// stdin-Tokens sammeln, bis zwei Zahlen vorliegen (C++: `cin >> w >> h`).
fn read_two_i32() -> (i32, i32) {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut tokens: Vec<i32> = Vec::new();
    let mut line = String::new();
    while tokens.len() < 2 {
        line.clear();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        tokens.extend(line.split_whitespace().filter_map(|t| t.parse::<i32>().ok()));
    }
    wh_from_tokens(&tokens)
}

/// Bildverzeichnis aus Argumenten/Env (--dir <pfad>, FRAMES_DIR, sonst cwd).
fn resolve_frames_dir(args: &[String]) -> String {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--dir" {
            if let Some(p) = it.next() {
                return p.clone();
            }
        }
    }
    if let Some(p) = args.iter().find(|a| !a.starts_with("--")) {
        return p.clone();
    }
    std::env::var("FRAMES_DIR").unwrap_or_else(|_| ".".to_string())
}

// ---------------------------------------------------------------------------
// Pixelbuffer (RGB-Alpha-puffer statt zweitem/drittem Renderer)
// ---------------------------------------------------------------------------
fn set_px(buf: &mut [u8], w: i32, x: i32, y: i32, c: Color) {
    if x < 0 || x >= w || y < 0 {
        return;
    }
    let idx = (y * w + x) as usize * 4;
    if idx + 3 >= buf.len() {
        return;
    }
    buf[idx] = c.r;
    buf[idx + 1] = c.g;
    buf[idx + 2] = c.b;
    buf[idx + 3] = c.a;
}

fn get_px(buf: &[u8], w: i32, x: i32, y: i32) -> Color {
    if x < 0 || x >= w || y < 0 {
        return Color::RGB(0, 0, 0);
    }
    let idx = (y * w + x) as usize * 4;
    if idx + 3 >= buf.len() {
        return Color::RGB(0, 0, 0);
    }
    Color::RGBA(buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3])
}

fn fill_buf(buf: &mut [u8], c: Color) {
    for px in buf.chunks_exact_mut(4) {
        px[0] = c.r;
        px[1] = c.g;
        px[2] = c.b;
        px[3] = c.a;
    }
}

/// Bilde den Inhalt eines RGBA-Puffers in den Hauptbildpuffer ab (nearest).
fn scale_rgba_into(w: i32, h: i32, buf: &mut [u8], sw: i32, sh: i32, src: &[u8]) {
    for y in 0..h {
        let sy = (y * sh / h).clamp(0, sh - 1) as usize;
        for x in 0..w {
            let sx = (x * sw / w).clamp(0, sw - 1) as usize;
            let si = (sy * sw as usize + sx) * 4;
            let di = (y * w + x) as usize * 4;
            buf[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
}

fn load_image_to_rgba(path: &str) -> Option<(i32, i32, Vec<u8>)> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = (img.width() as i32, img.height() as i32);
    Some((w, h, img.into_raw()))
}

fn save_rgba_bmp(path: &str, w: i32, h: i32, buf: &[u8]) {
    let img = image::RgbaImage::from_raw(w as u32, h as u32, buf.to_vec());
    if let Some(img) = img {
        let _ = img.save(path);
    }
}

// ---------------------------------------------------------------------------
// Datenstrukturen
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Default)]
struct KeyEventxyes {
    x: i32,
    key: i32,
    iialpha: f32,
    xkeyk: i32,
}

#[derive(Clone, Copy, Default)]
struct KeyEventxyrgbt {
    x: i32,
    y: i32,
    r: u8,
    g: u8,
    b: u8,
    p: i32,
    t: i32,
    a: i32,
}

struct App {
    xw: i32,
    yh: i32,
    xl: i32,
    leftrand: i32,
    rightrand: i32,
    xoff: f32,
    xofflr: i32,
    xoffmove: i32,
    iadd: i32,
    iadd2: i32,
    jadd: i32,
    iaddud: i32,
    iaddf: bool,
    xValuesfar: Vec<i32>,
    noteValue: i32,
    noteScale: i32,
    noteScalebefore: i32,
    noteScalePart: i32,
    noteScaleadd: i32,
    noteScaleaddPart: i32,
    noteScaleaddPartbefore: i32,
    noteScalePartStart: bool,
    noteadjust: i32,
    useMajorChords: bool,
    usePerfectFifth: bool,
    usePlagalCadence: bool,
    noteValueadd: i32,
    useNotesSet: bool,
    notesset: Vec<Vec<i32>>,
    notessetx: Vec<Vec<i32>>,
    notessetcount: i32,
    chordTypes: Vec<(bool, bool, i32)>,
    chordparts: bool,
    harmonics: bool,
    rr: i32,
    gg: i32,
    bb: i32,
    rrr: i32,
    ggg: i32,
    bbb: i32,
    rz: i32,
    gz: i32,
    bz: i32,
    radier: bool,
    rrange: i32,
    grange: i32,
    brange: i32,
    selectedR: u8,
    selectedG: u8,
    selectedB: u8,
    bpmk: bool,
    bpmz: bool,
    bpmo: bool,
    bpmo3: bool,
    biialpha: bool,
    cvcamload: bool,
    surfacesh: bool,
    sendbpm: bool,
    leftset: bool,
    rightset: bool,
    bpmscanfirstpress: bool,
    bpmmal: f32,
    bpmmultipli: f32,
    fixedBpm: f32,
    fixedBpm2: f32,
    fixedBpmTeilerBool: bool,
    fixedBpmTeiler: f32,
    fixedBpmTeile: f32,
    patternrack: bool,
    Bpm: f32,
    pixelsPerBeat: f32,
    secPerBeat: f32,
    timePerPixel: f32,
    timePerPixelns: i64,
    bpmValues: Vec<f32>,
    bpmCount: i32,
    fixedfaktor: f32,
    fixedfaktor2: f32,
    y_ashape: f32,
    funcalc: bool,
    rec: bool,
    currentFrame: usize,
    filenames: Vec<String>,
    frames_dir: String,
    renoisenote: bool,
    renoiseplay: bool,
    renoisepatternfollow: bool,
    renoisepatternloop: bool,
    renoiseswitch: bool,
    cc: bool,
    isLeftMouseButtonHeld: bool,
    isRightMouseButtonHeld: bool,
    isMiddleMouseButtonHeld: bool,
    keyEventsxyes: Vec<KeyEventxyes>,
    xyes: Vec<i32>,
    keyEventsxyrgbt: Vec<KeyEventxyrgbt>,
    xValuesTime: Vec<f64>,
    xValues: Vec<f32>,
    xValueschords: Vec<f32>,
    previousTime: Instant,
    firstTime: Instant,
    l: i32,
    lb: i32,
    pixi: i32,
    pixj: i32,
}

impl App {
    fn new(xw: i32, yh: i32) -> Self {
        let now = Instant::now();
        App {
            xw,
            yh,
            xl: 10,
            leftrand: 0,
            rightrand: 0,
            xoff: 1.0,
            xofflr: 0,
            xoffmove: 0,
            iadd: 1,
            iadd2: 1,
            jadd: 1,
            iaddud: 1,
            iaddf: false,
            xValuesfar: Vec::new(),
            noteValue: 0,
            noteScale: 120,
            noteScalebefore: 120,
            noteScalePart: 12,
            noteScaleadd: 0,
            noteScaleaddPart: 0,
            noteScaleaddPartbefore: 0,
            noteScalePartStart: true,
            noteadjust: 0,
            useMajorChords: true,
            usePerfectFifth: true,
            usePlagalCadence: true,
            noteValueadd: 0,
            useNotesSet: false,
            notesset: Vec::new(),
            notessetx: Vec::new(),
            notessetcount: 0,
            chordTypes: Vec::new(),
            chordparts: false,
            harmonics: true,
            rr: 0,
            gg: 0,
            bb: 0,
            rrr: 0,
            ggg: 0,
            bbb: 0,
            rz: 0,
            gz: 0,
            bz: 0,
            radier: false,
            rrange: 0,
            grange: 0,
            brange: 0,
            selectedR: 0,
            selectedG: 0,
            selectedB: 0,
            bpmk: false,
            bpmz: false,
            bpmo: false,
            bpmo3: false,
            biialpha: true,
            cvcamload: false,
            surfacesh: false,
            sendbpm: true,
            leftset: false,
            rightset: false,
            bpmscanfirstpress: true,
            bpmmal: 1.0,
            bpmmultipli: 1.0,
            fixedBpm: 120.0,
            fixedBpm2: 0.0,
            fixedBpmTeilerBool: false,
            fixedBpmTeiler: 1.0,
            fixedBpmTeile: 1.0,
            patternrack: true,
            Bpm: 0.0,
            pixelsPerBeat: 0.0,
            secPerBeat: 0.0,
            timePerPixel: 0.0,
            timePerPixelns: 0,
            bpmValues: Vec::new(),
            bpmCount: 0,
            fixedfaktor: 1.0,
            fixedfaktor2: 1.0,
            y_ashape: 1.0,
            funcalc: false,
            rec: true,
            currentFrame: 1,
            filenames: Vec::new(),
            frames_dir: String::new(),
            renoisenote: true,
            renoiseplay: true,
            renoisepatternfollow: true,
            renoisepatternloop: true,
            renoiseswitch: true,
            cc: true,
            isLeftMouseButtonHeld: false,
            isRightMouseButtonHeld: false,
            isMiddleMouseButtonHeld: false,
            keyEventsxyes: Vec::new(),
            xyes: Vec::new(),
            keyEventsxyrgbt: Vec::new(),
            xValuesTime: Vec::new(),
            xValues: Vec::new(),
            xValueschords: Vec::new(),
            previousTime: now,
            firstTime: now,
            l: 0,
            lb: 0,
            pixi: 0,
            pixj: 0,
        }
    }

    fn calc_bpm(&mut self) {
        self.calc_bpm_base(self.fixedBpm);
    }

    fn calc_bpm_base(&mut self, base_bpm: f32) {
        self.Bpm = bpm_from(base_bpm, self.bpmmal, self.fixedfaktor2);
        self.pixelsPerBeat = (self.xw - self.leftrand - self.rightrand) as f32 * self.yh as f32;
        self.secPerBeat = 60.0f32 / self.Bpm;
        self.timePerPixel = self.secPerBeat / self.pixelsPerBeat;
        self.timePerPixelns = (self.timePerPixel * 1.0e9) as i64;
        println!("{} {}", self.Bpm, self.timePerPixelns);
    }

    fn send_bpm(&mut self) {
        self.calc_bpm();
        osc::send_bpm(self.fixedBpm2, self.fixedBpmTeiler);
    }

    fn is_note_in_chord(&self, note: i32, x: i32) -> bool {
        if !self.harmonics {
            return true;
        }
        let n = (((note + self.noteValueadd) % 12) + 12) % 12;
        if self.useNotesSet {
            for v in 0..self.notessetcount as usize {
                if v == 0 {
                    if x < self.notessetx[v].first().copied().unwrap_or(0) {
                        for &nn in self.notesset[v].iter() {
                            if n == nn {
                                return true;
                            }
                        }
                    }
                } else if let (Some(cx), Some(px)) = (
                    self.notessetx[v].first().copied(),
                    self.notessetx[v - 1].first().copied(),
                ) {
                    if x < cx && x >= px {
                        for &nn in self.notesset[v].iter() {
                            if n == nn {
                                return true;
                            }
                        }
                    }
                }
            }
            return false;
        }
        if self.useMajorChords && !self.useNotesSet {
            n == 0 || n == 4 || (self.usePerfectFifth && n == 7)
        } else if self.usePlagalCadence && !self.useNotesSet {
            n == 0 || n == 5 || n == 7
        } else {
            n == 0 || n == 3 || (self.usePerfectFifth && n == 7)
        }
    }

    fn is_note_in_chordpatt(&self, note: i32, i: i32, j: i32, hgt: i32) -> bool {
        if self.xValueschords.is_empty() || self.chordTypes.is_empty() {
            let nv = j * self.noteScale / hgt;
            self.is_note_in_chord(nv, 0)
        } else {
            let mut hit = false;
            for c in 0..self.xValueschords.len() {
                let (maj, fifth, add) = self.chordTypes[c];
                let nv = j * self.noteScale / hgt + add;
                if (i as f32) < self.xValueschords[c] {
                    if self.is_note_in_chord_ext(nv, maj, fifth) {
                        hit = true;
                    }
                } else if c + 1 < self.xValueschords.len()
                    && (i as f32) < self.xValueschords[c + 1]
                    && (i as f32) >= self.xValueschords[c]
                {
                    let nv2 = j * self.noteScale / hgt;
                    if self.is_note_in_chord_ext(nv2, maj, fifth) {
                        hit = true;
                    }
                }
            }
            hit
        }
    }

    fn is_note_in_chord_ext(&self, note: i32, major: bool, fifth: bool) -> bool {
        if !self.harmonics {
            return true;
        }
        let n = (((note + self.noteValueadd) % 12) + 12) % 12;
        if self.useNotesSet {
            for v in self.notesset.iter() {
                for &nn in v.iter() {
                    if n == nn {
                        return true;
                    }
                }
            }
            return false;
        }
        if major {
            n == 0 || n == 4 || (fifth && n == 7)
        } else {
            n == 0 || n == 3 || (fifth && n == 7)
        }
    }
}

fn calculate_x_coordinates(xValuesTime: &[f64], xw: i32) -> Vec<f32> {
    if xValuesTime.is_empty() {
        return Vec::new();
    }
    let min_time = xValuesTime.iter().cloned().fold(f64::MAX, f64::min);
    let max_time = xValuesTime.iter().cloned().fold(f64::MIN, f64::max);
    let time_range = max_time - min_time;
    println!("{}minmax {}", min_time, max_time);
    println!(
        "{}timerange {}",
        time_range,
        xValuesTime.len() as f64
    );
    let mut out = Vec::new();
    for t in xValuesTime {
        let x = ((t - min_time) / time_range * xw as f64) as f32;
        out.push(x);
    }
    out
}

fn color_to_note_value_and_volume(r: u8, g: u8, b: u8, note_scale: i32, add: i32) -> (i32, i32) {
    let volume = (r as i32 + g as i32 + b as i32) / 6;
    let note_value = if r > g && r > b {
        (r as i32 * note_scale / 255) + add
    } else if g > r && g > b {
        (g as i32 * note_scale / 255) + add + (43 * note_scale / 120)
    } else {
        (b as i32 * note_scale / 255) + add + (85 * note_scale / 120)
    };
    (note_value, volume)
}

/// Vorschau-Notenwert/Lautstärke für die Left/Right-Tonauswahl (C++: rrn5.cpp:2842-2886).
fn preview_note_vol(
    noteadjust: i32,
    px: i32,
    pj: i32,
    w: i32,
    h: i32,
    note_scale: i32,
    note_scale_add: i32,
    rgb: (u8, u8, u8),
) -> (i32, i32) {
    let (mut pnote, mut pvol) = match noteadjust {
        0 => color_to_note_value_and_volume(rgb.0, rgb.1, rgb.2, note_scale, note_scale_add),
        1 => (note_scale - (pj * note_scale / h) + note_scale_add, 127),
        2 => (
            (pj * note_scale / h) + note_scale_add,
            127 - (px * 127 / w),
        ),
        3 => (note_scale - (pj * note_scale / h) + note_scale_add, px * 127 / w),
        4 => (
            note_scale - (pj * note_scale / h) + note_scale_add,
            127 - (px * 127 / w),
        ),
        5 => ((px * note_scale / w) + note_scale_add, px * 127 / w),
        6 => (
            (px * note_scale / w) + note_scale_add,
            127 - (px * 127 / w),
        ),
        7 => (
            note_scale - (px * note_scale / w) + note_scale_add,
            px * 127 / w,
        ),
        8 => (
            note_scale - (px * note_scale / w) + note_scale_add,
            127 - (px * 127 / w),
        ),
        _ => (60, 127),
    };
    pnote = pnote.clamp(0, 127);
    pvol = pvol.clamp(0, 127);
    (pnote, pvol)
}

/// Nächster Bildindex mit Umlauf (None bei leerer Liste).
fn next_frame_index(current: usize, len: usize, step: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some((current as i32 + step).rem_euclid(len as i32) as usize)
}

/// Bild anzeigen und zusätzlich in MyImageShm veröffentlichen (imgsh-Ersatz).
fn show_frame(
    app: &mut App,
    editor: &mut SharedMemoryEditor,
    main_view: &mut View,
    main_buffer: &mut [u8],
    w: i32,
    h: i32,
    step: i32,
) {
    let Some(idx) = next_frame_index(app.currentFrame, app.filenames.len(), step) else {
        return;
    };
    app.currentFrame = idx;
    if let Some((iw, ih, img)) = load_image_to_rgba(&app.filenames[idx]) {
        scale_rgba_into(w, h, main_buffer, iw, ih, &img);
        main_view.present(main_buffer, w, h);
        if let Err(e) = editor.write_image(iw, ih, &img) {
            eprintln!("MyImageShm: {e}");
        }
    }
}

/// 32-bpp-Pixel aus dem Shared-Memory-Format in RGBA umwandeln (Pitch beachtet).
fn convert_to_rgba(
    pixels: &[u8],
    width: i32,
    height: i32,
    pitch: i32,
    bpp: i32,
    rmask: u32,
    gmask: u32,
    bmask: u32,
    amask: u32,
) -> Vec<u8> {
    let mut out = vec![0u8; (width.max(0) * height.max(0)) as usize * 4];
    if width <= 0 || height <= 0 || pitch <= 0 || bpp != 32 {
        return out;
    }
    let channel = |v: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 255;
        }
        let shift = mask.trailing_zeros();
        let bits = mask.count_ones();
        let raw = (v & mask) >> shift;
        let max = (1u32 << bits) - 1;
        (raw * 255 / max) as u8
    };
    for y in 0..height {
        for x in 0..width {
            let si = (y * pitch + x * 4) as usize;
            if si + 3 >= pixels.len() {
                return out;
            }
            let v = u32::from_le_bytes([
                pixels[si],
                pixels[si + 1],
                pixels[si + 2],
                pixels[si + 3],
            ]);
            let oi = ((y * width + x) * 4) as usize;
            out[oi] = channel(v, rmask);
            out[oi + 1] = channel(v, gmask);
            out[oi + 2] = channel(v, bmask);
            out[oi + 3] = if amask == 0 {
                255
            } else {
                channel(v, amask)
            };
        }
    }
    out
}

/// Bildverzeichnis scannen und nur unterstützte Endungen sortiert zurückgeben.
fn filter_image_files(directory: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ["png", "jpg", "jpeg", "bmp"].contains(&ext.as_str()) {
                files.push(path.to_string_lossy().into_owned());
            }
        }
    }
    files.sort();
    files
}

/// Breite/Höhe aus gesammelten stdin-Tokens (C++: `cin >> w >> h`).
fn wh_from_tokens(tokens: &[i32]) -> (i32, i32) {
    (
        tokens.first().copied().unwrap_or(0),
        tokens.get(1).copied().unwrap_or(0),
    )
}

/// Renoise-PatternClipboard-XML: genau eine Line je Index, Noten als NoteColumns.
fn build_renoise_xml(pattern_length: i32, notes: &[(i32, String)]) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\"?>\n");
    xml.push_str("<PatternClipboard.BlockBuffer doc_version=\"0\">\n");
    xml.push_str("<Columns>\n<Column>\n<Column>\n<Lines>\n");
    for i in 0..pattern_length.max(0) {
        xml.push_str(&format!("<Line index=\"{i}\">\n"));
        for (line_index, note) in notes {
            if *line_index == i {
                xml.push_str("<NoteColumns>\n<NoteColumn>\n");
                xml.push_str(&format!("<Note>{note}</Note>\n"));
                xml.push_str("<Instrument>00</Instrument>\n");
                xml.push_str("</NoteColumn>\n</NoteColumns>\n");
            }
        }
        xml.push_str("</Line>\n");
    }
    xml.push_str("</Lines>\n<ColumnType>NoteColumn</ColumnType>\n");
    xml.push_str("<SubColumnMask>true true true false false false false false</SubColumnMask>\n");
    xml.push_str("</Column>\n</Column>\n</Columns>\n</PatternClipboard.BlockBuffer>\n");
    xml
}

/// Pixel für Harmonics zeichnen? C++: r != 255 && g != 0 && b != 0 (rrn5.cpp:1676).
fn harmonic_pixel_eligible(r: u8, g: u8, b: u8) -> bool {
    r != 255 && g != 0 && b != 0
}

/// BPM aus Basis, Mal-Faktor und fixedfaktor2 (C++: calculateBpmParameters).
fn bpm_from(base_bpm: f32, bpmmal: f32, fixedfaktor2: f32) -> f32 {
    base_bpm * (bpmmal * fixedfaktor2)
}

/// noteScaleadd-Berechnung für F3/F4 (C++: rrn5.cpp:2786-2823).
fn scale_add(note_scale: i32, note_scale_before: i32, part_before: i32, part_start: bool) -> i32 {
    if part_start || part_before == 0 {
        let half = (120 - note_scale) / 2;
        if (half % 12) < 6 {
            half - (half % 12)
        } else {
            half - (half % 12) + 12
        }
    } else {
        let half = (note_scale_before - note_scale) / 2;
        let d = part_before / 2;
        if (half % part_before) < d {
            half - (half % d)
        } else {
            half - (half % d) + part_before
        }
    }
}

/// Skalierter Notenwert an Pixelposition für die Ecken-Ausgabe (C++: scaledNoteAt).
fn scaled_note_at(
    noteadjust: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    note_scale: i32,
    note_scale_add: i32,
) -> i32 {
    let n = match noteadjust {
        0 => 60,
        1 | 3 | 4 => note_scale - (y * note_scale / h) + note_scale_add,
        2 => (y * note_scale / h) + note_scale_add,
        5 | 6 => (x * note_scale / w) + note_scale_add,
        7 | 8 => note_scale - (x * note_scale / w) + note_scale_add,
        _ => 60,
    };
    n.clamp(0, 127)
}

fn number_to_note_name(note_value: i32) -> String {
    let note = NOTE_NAMES[(((note_value % 12) + 12) % 12) as usize];
    format!("{}{}", note, note_value / 12)
}

fn convert_midi_note_to_renoise_note(note: i32) -> String {
    let n = (((note % 12) + 12) % 12) as usize;
    let octave = note / 12 - 1;
    format!("{}{}", RENOISE_NOTE_NAMES[n], octave)
}

fn is_color_in_range(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8, rr: i32, gg: i32, bb: i32) -> bool {
    (r1 as i32 >= r2 as i32 - rr && r1 as i32 <= r2 as i32 + rr)
        && (g1 as i32 >= g2 as i32 - gg && g1 as i32 <= g2 as i32 + gg)
        && (b1 as i32 >= b2 as i32 - bb && b1 as i32 <= b2 as i32 + bb)
}

fn sleep_ns(ns: i64) {
    if ns > 0 {
        std::thread::sleep(Duration::from_nanos(ns as u64));
    }
}

// ---------------------------------------------------------------------------
// View = ein Fenster mit eigenem Renderer + Streaming-Texture.
// Ein View präsentiert genau einen CPU-Puffer (main/second/third).
// ---------------------------------------------------------------------------
struct View {
    canvas: Canvas<Window>,
    #[allow(dead_code)]
    _creator: TextureCreator<sdl2::video::WindowContext>,
    pub tex: Texture,
}

impl View {
    fn present(&mut self, buf: &[u8], w: i32, h: i32) {
        let row = (w as usize) * 4;
        let _ = self.tex.with_lock(None, |dst, pitch| {
            for y in 0..h as usize {
                let src = &buf[y * row..(y + 1) * row];
                let dst_r = &mut dst[y * pitch..y * pitch + row];
                dst_r.copy_from_slice(src);
            }
        });
        let _ = self.canvas.copy(&self.tex, None, None);
        self.canvas.present();
    }

    fn clear(&mut self, w: i32, h: i32) {
        self.present(&vec![0u8; (w * h) as usize * 4], w, h);
    }
}

fn make_view(video: &sdl2::VideoSubsystem, title: &str, xw: i32, yh: i32, x: i32, y: i32) -> View {
    let window = match video
        .window(title, xw as u32, yh as u32)
        .position(x, y)
        .opengl()
        .build()
    {
        Ok(w) => w,
        Err(e) => {
            println!("Failed to create window {title}: {e}");
            std::process::exit(1);
        }
    };
    let canvas = match window.into_canvas().accelerated().build() {
        Ok(c) => c,
        Err(e) => {
            println!("Failed to create renderer for {title}: {e}");
            std::process::exit(1);
        }
    };
    let texture_creator: TextureCreator<sdl2::video::WindowContext> = canvas.texture_creator();
    let tex = match texture_creator.create_texture_streaming(
        PixelFormatEnum::RGBA32,
        xw as u32,
        yh as u32,
    ) {
        Ok(t) => t,
        Err(e) => {
            println!("Failed to create texture for {title}: {e}");
            std::process::exit(1);
        }
    };
    View {
        canvas,
        _creator: texture_creator,
        tex,
    }
}

// ---------------------------------------------------------------------------
// KeyEventRender.txt einlesen (Taste "Leer"), wie readKeyEvent2()
// ---------------------------------------------------------------------------
fn read_key_event2(app: &mut App) {
    app.keyEventsxyes.clear();
    app.xyes.clear();
    let Ok(content) = std::fs::read_to_string("KeyEventRender.txt") else {
        println!("Unable to open KeyEventRender.txt");
        return;
    };
    for line in content.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() >= 4 {
            if let (Ok(x), Ok(key), Ok(ial), Ok(xkeyk)) = (
                fields[0].trim().parse::<i32>(),
                fields[1].trim().parse::<i32>(),
                fields[2].trim().parse::<f32>(),
                fields[3].trim().parse::<i32>(),
            ) {
                app.keyEventsxyes.push(KeyEventxyes {
                    x,
                    key,
                    iialpha: ial,
                    xkeyk,
                });
            } else {
                println!("Failed to parse line: {line}");
            }
        }
    }
    for i in 0..app.keyEventsxyes.len() {
        println!("{}", app.keyEventsxyes[i].iialpha);
    }
}

// ---------------------------------------------------------------------------
// Pattern als Renoise-XML in die Zwischenablage (Taste "g")
// ---------------------------------------------------------------------------
fn copy_pattern_to_clipboard(app: &App) {
    println!("Copying pattern to clipboard...");
    println!("Patternlänge? ");
    let pattern_len_str = read_stdin_str();
    println!("Patternanfang? ");
    let pattern_start_str = read_stdin_str();
    println!("Patternende? ");
    let pattern_end_str = read_stdin_str();
    println!(
        "Patternlänge: {} Patternanfang: {} Patternende: {}",
        pattern_len_str, pattern_start_str, pattern_end_str
    );

    let pattern_len: i32 = pattern_len_str.trim().parse().unwrap_or(0);
    let pattern_start: i32 = pattern_start_str.trim().parse().unwrap_or(0);
    let pattern_end: i32 = pattern_end_str.trim().parse().unwrap_or(0);
    if pattern_len <= 0 {
        return;
    }
    let pattern_length = pattern_end - pattern_start + 1;
    if pattern_length <= 0 {
        println!("Ungültiger Bereich");
        return;
    }
    let pixels_anfang = app.xw * pattern_start / pattern_len;
    let pixels_ende = app.xw * pattern_end / pattern_len;
    let pixels_anzahl = pixels_ende - pixels_anfang + 1;

    let mut sorted: Vec<&KeyEventxyrgbt> = app.keyEventsxyrgbt.iter().collect();
    sorted.sort_by_key(|e| e.p);

    let mut count = 0;
    for e in &sorted {
        if e.p >= pixels_anfang && e.p <= pixels_ende {
            count += 1;
        }
    }
    println!("Events im Bereich: {count}");

    let pixel_per_line = pixels_anzahl as f32 / pattern_length as f32;
    let mut notes: Vec<(i32, String)> = Vec::new();
    for e in sorted {
        if e.p < pixels_anfang || e.p > pixels_ende || pixel_per_line <= 0.0 {
            continue;
        }
        let pixel_position = e.p - pixels_anfang;
        let line_index = (pixel_position as f32 / pixel_per_line) as i32;
        if line_index < 0 || line_index >= pattern_length {
            continue;
        }
        let mut note_value = 0.0f32;
        if app.noteadjust == 1 || app.noteadjust == 3 || app.noteadjust == 4 {
            note_value = (app.noteScale - (e.y * app.noteScale / app.yh) + app.noteScaleadd) as f32;
        } else if app.noteadjust == 2 {
            note_value = (e.y * app.noteScale / app.yh + app.noteScaleadd) as f32;
        } else if app.noteadjust == 5 || app.noteadjust == 6 {
            note_value = (e.x * app.noteScale / app.xw + app.noteScaleadd) as f32;
        } else if app.noteadjust == 7 || app.noteadjust == 8 {
            note_value = (app.noteScale - (e.x * app.noteScale / app.xw) + app.noteScaleadd) as f32;
        }
        notes.push((line_index, convert_midi_note_to_renoise_note(note_value as i32)));
    }
    let xml = build_renoise_xml(pattern_length, &notes);

    let temp_file = "/tmp/renoise_pattern.xml";
    if std::fs::write(temp_file, xml).is_err() {
        eprintln!("Konnte {temp_file} nicht schreiben");
        return;
    }
    let xclip_ok = std::process::Command::new("which")
        .arg("xclip")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !xclip_ok {
        eprintln!("Error: xclip not found. Please install xclip first.");
        return;
    }
    let cmd = format!("xclip -selection clipboard -i {}", temp_file);
    let _ = std::process::Command::new("sh").arg("-c").arg(&cmd).status();
    std::thread::sleep(Duration::from_millis(100));
    let cmd2 = format!("xclip -selection primary -i {}", temp_file);
    let _ = std::process::Command::new("sh").arg("-c").arg(&cmd2).status();
    println!("Pattern copied to clipboard with {} lines.", pattern_length);
}

// ---------------------------------------------------------------------------
// findAndDrawHarmonics – liest secondsave, zeichnet in second
// ---------------------------------------------------------------------------
fn find_and_draw_harmonics(
    app: &App,
    second: &mut [u8],
    secondsave: &[u8],
    w: i32,
    h: i32,
) {
    let colors: [(u8, u8, u8); 6] = [
        (255, 0, 0),
        (255, 255, 0),
        (0, 255, 0),
        (0, 255, 255),
        (0, 0, 255),
        (255, 0, 255),
    ];
    for i in 0..w {
        for j in 0..h {
            let px = get_px(secondsave, w, i, j);
            if !harmonic_pixel_eligible(px.r, px.g, px.b) {
                continue;
            }
            if app.xValueschords.is_empty() || app.chordTypes.is_empty() {
                let nv = j * app.noteScale / h;
                if app.is_note_in_chord(nv, i) {
                    let ci = (nv / 12 % 6).clamp(0, 5) as usize;
                    for x in i..(i + app.iadd) {
                        set_px(second, w, x.clamp(0, w - 1), j, Color::RGB(colors[ci].0, colors[ci].1, colors[ci].2));
                    }
                }
            } else {
                for c in 0..app.xValueschords.len() {
                    let (maj, fifth, add) = app.chordTypes[c];
                    let nv = (j + add) * app.noteScale / h;
                    let hit = if (i as f32) < app.xValueschords[c] {
                        app.is_note_in_chord_ext(nv, maj, fifth)
                    } else if c + 1 < app.xValueschords.len()
                        && (i as f32) < app.xValueschords[c + 1]
                        && (i as f32) >= app.xValueschords[c]
                    {
                        app.is_note_in_chord_ext(nv, maj, fifth)
                    } else {
                        false
                    };
                    if hit {
                        let ci = (nv / 12 % 6).clamp(0, 5) as usize;
                        for x in i..(i + app.iadd) {
                            set_px(second, w, x.clamp(0, w - 1), j, Color::RGB(colors[ci].0, colors[ci].1, colors[ci].2));
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tastaturbehandlung (KEYDOWN), orientiert an rrn5.cpp
// ---------------------------------------------------------------------------
fn handle_key(
    app: &mut App,
    editor: &mut SharedMemoryEditor,
    main_view: &mut View,
    second_view: &mut View,
    third_view: &mut View,
    main_buffer: &mut [u8],
    second: &mut Vec<u8>,
    third: &mut Vec<u8>,
    secondsave: &mut Vec<u8>,
    keycode: Option<Keycode>,
    scancode: Option<Scancode>,
    keymod: Mod,
) {
    let w = app.xw;
    let h = app.yh;
    let Some(kc) = keycode else { return };
    let Some(sc) = scancode else { return };

    if keymod.contains(Mod::LSHIFTMOD) || keymod.contains(Mod::RSHIFTMOD) {
        match kc {
            Keycode::Greater => {
                app.noteValueadd += 1;
                println!("{}", app.noteValueadd);
            }
            Keycode::Tab => {
                show_frame(app, editor, main_view, main_buffer, w, h, -1);
            }
            _ => {}
        }
    }
    if keymod.contains(Mod::RCTRLMOD) {
        match kc {
            Keycode::PageUp => {
                app.jadd += 1;
                println!("{}", app.jadd);
            }
            Keycode::PageDown => {
                app.jadd -= 1;
                println!("{}", app.jadd);
            }
            Keycode::J => {
                app.fixedfaktor2 += 0.00765625;
                app.calc_bpm();
                println!("{}", app.fixedfaktor2);
            }
            Keycode::M => {
                app.fixedfaktor2 -= 0.00765625;
                app.calc_bpm();
                println!("{}", app.fixedfaktor2);
            }
            Keycode::Up => {
                app.xoff += 0.25;
                println!("{}", app.xoff);
            }
            Keycode::Down => {
                app.xoff -= 0.25;
                println!("{}", app.xoff);
            }
            Keycode::Left => {
                app.xofflr -= 1;
                println!("{}", app.xoff);
            }
            Keycode::Right => {
                app.xofflr += 1;
                println!("{}", app.xoff);
            }
            _ => {}
        }
    }
    if keymod.contains(Mod::LCTRLMOD) {
        let kp = |s: Scancode| -> Option<i32> {
            match s {
                Scancode::Kp1 => Some(1),
                Scancode::Kp2 => Some(2),
                Scancode::Kp3 => Some(3),
                Scancode::Kp4 => Some(4),
                Scancode::Kp5 => Some(5),
                Scancode::Kp6 => Some(6),
                Scancode::Kp7 => Some(7),
                Scancode::Kp8 => Some(8),
                Scancode::Kp9 => Some(9),
                _ => None,
            }
        };
        if let Some(n) = kp(sc) {
            if !app.patternrack {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeiler *= n as f32;
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmal *= n as f32;
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
        }
        match kc {
            Keycode::Comma => {
                app.filenames = read_filenames(&app.frames_dir);
            }
            Keycode::F9 => {
                app.fixedBpm -= 0.0625;
                println!("{}", app.fixedBpm);
                app.fixedBpm2 = app.fixedBpm;
                app.send_bpm();
            }
            Keycode::F10 => {
                app.fixedBpm += 0.0625;
                println!("{}", app.fixedBpm);
                app.fixedBpm2 = app.fixedBpm;
                app.send_bpm();
            }
            Keycode::F11 => {
                app.y_ashape -= 0.0625;
                println!("{}", app.y_ashape);
            }
            Keycode::F12 => {
                app.y_ashape += 0.0625;
                println!("{}", app.y_ashape);
            }
            Keycode::Q => {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeile += 1.0;
                    if app.fixedBpmTeile != 0.0 {
                        app.fixedBpmTeiler = 1.0 / app.fixedBpmTeile;
                    } else if app.fixedBpmTeile > 1.0 {
                        app.fixedBpmTeiler = app.fixedBpmTeile;
                    }
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmultipli += 1.0;
                    if app.bpmmultipli != 0.0 {
                        app.bpmmal = 1.0 / app.bpmmultipli;
                    } else if app.bpmmultipli > 1.0 {
                        app.bpmmal = app.bpmmultipli;
                    }
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
            Keycode::A => {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeile -= 1.0;
                    if app.fixedBpmTeile != 0.0 {
                        app.fixedBpmTeiler = 1.0 / app.fixedBpmTeile;
                    } else if app.fixedBpmTeile < -1.0 {
                        app.fixedBpmTeiler = app.fixedBpmTeile;
                    }
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmultipli -= 1.0;
                    if app.bpmmultipli != 0.0 {
                        app.bpmmal = 1.0 / app.bpmmultipli;
                    } else if app.bpmmultipli < -1.0 {
                        app.bpmmal = app.bpmmultipli;
                    }
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
            Keycode::Num0 => {
                if app.bpmscanfirstpress {
                    app.previousTime = Instant::now();
                    app.bpmscanfirstpress = false;
                } else {
                    let cur = Instant::now();
                    let elaps = cur.duration_since(app.previousTime).as_nanos() as f64;
                    app.previousTime = cur;
                    let bpms2 = 60_000_000_000.0 / elaps;
                    app.bpmValues.push(bpms2 as f32);
                    app.bpmCount += 1;
                    if app.bpmCount == 4 {
                        let total: f32 = app.bpmValues.iter().sum();
                        let avg = total / app.bpmValues.len() as f32;
                        app.fixedBpm = avg;
                        app.fixedBpm2 = avg;
                        app.bpmValues.clear();
                        app.bpmCount = 0;
                        app.bpmscanfirstpress = true;
                        osc::send_bpm(app.fixedBpm2, app.fixedBpmTeiler);
                        app.funcalc = true;
                    }
                }
            }
            Keycode::Home => {
                if app.bpmscanfirstpress {
                    app.firstTime = Instant::now();
                    app.previousTime = app.firstTime;
                    app.bpmscanfirstpress = false;
                    app.xValuesTime.clear();
                } else {
                    let cur = Instant::now();
                    let span = cur.duration_since(app.previousTime).as_secs_f64();
                    app.xValuesTime.push(span);
                }
            }
            Keycode::End => {
                app.xValues = calculate_x_coordinates(&app.xValuesTime, app.xw);
                for v in &app.xValues {
                    println!("{}", v);
                }
                app.bpmscanfirstpress = true;
            }
            Keycode::Insert => {
                app.chordparts = true;
                if app.bpmscanfirstpress {
                    app.firstTime = Instant::now();
                    app.previousTime = app.firstTime;
                    app.bpmscanfirstpress = false;
                    app.xValuesTime.clear();
                } else {
                    let cur = Instant::now();
                    let span = cur.duration_since(app.previousTime).as_secs_f64();
                    app.xValuesTime.push(span);
                }
            }
            Keycode::Delete => {
                app.chordparts = false;
                app.xValueschords = calculate_x_coordinates(&app.xValuesTime, app.xw);
                for v in &app.xValueschords {
                    println!("{}", v);
                }
                app.bpmscanfirstpress = true;
            }
            _ => {}
        }
    }
    if keymod.contains(Mod::LALTMOD) {
        let kp = |s: Scancode| -> Option<i32> {
            match s {
                Scancode::Kp1 => Some(1),
                Scancode::Kp2 => Some(2),
                Scancode::Kp3 => Some(3),
                Scancode::Kp4 => Some(4),
                Scancode::Kp5 => Some(5),
                Scancode::Kp6 => Some(6),
                Scancode::Kp7 => Some(7),
                Scancode::Kp8 => Some(8),
                Scancode::Kp9 => Some(9),
                _ => None,
            }
        };
        if let Some(n) = kp(sc) {
            if !app.patternrack {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeiler /= n as f32;
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmal /= n as f32;
                    app.calc_bpm_base(app.fixedBpm + app.fixedfaktor);
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
        }
        match kc {
            Keycode::Q => {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeiler *= 2.0;
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmal *= 2.0;
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
            Keycode::A => {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeiler /= 2.0;
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmal /= 2.0;
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
            Keycode::W => {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeiler *= 3.0;
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmal *= 3.0;
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
            Keycode::S => {
                if app.fixedBpmTeilerBool {
                    app.fixedBpmTeiler /= 3.0;
                    println!("{}", app.fixedBpmTeiler);
                } else {
                    app.bpmmal /= 3.0;
                    app.calc_bpm();
                    println!("{} {}", app.bpmmal, app.timePerPixelns);
                }
            }
            Keycode::G => {
                app.fixedBpmTeilerBool = !app.fixedBpmTeilerBool;
                println!("fixedBpmTeilerBool {}", app.fixedBpmTeilerBool);
            }
            Keycode::D => {
                app.leftset = true;
            }
            Keycode::C => {
                app.rightset = true;
            }
            _ => {}
        }
    }

    match kc {
        Keycode::F5 => {
            if app.renoiseplay {
                osc::transport_start();
                app.renoiseplay = false;
            } else {
                osc::transport_stop();
                app.renoiseplay = true;
            }
        }
        Keycode::F8 => {
            app.renoiseplay = true;
        }
        Keycode::F7 => {
            if app.renoiseswitch {
                if app.renoisepatternfollow {
                    osc::t7000("/renoise/song/edit/pattern_follow", vec![rosc::OscType::Bool(true)]);
                    app.renoisepatternfollow = false;
                } else {
                    osc::t7000("/renoise/song/edit/pattern_follow", vec![rosc::OscType::Bool(false)]);
                    app.renoisepatternfollow = true;
                }
            } else {
                if app.renoisepatternfollow {
                    osc::t7001("/renoise/song/edit/pattern_follow", vec![rosc::OscType::Bool(true)]);
                    app.renoisepatternfollow = false;
                } else {
                    osc::t7001("/renoise/song/edit/pattern_follow", vec![rosc::OscType::Bool(false)]);
                    app.renoisepatternfollow = true;
                }
            }
        }
        Keycode::F6 => {
            if app.renoiseswitch {
                if app.renoisepatternloop {
                    osc::t7000("/renoise/transport/loop/pattern", vec![rosc::OscType::Bool(true)]);
                    app.renoisepatternloop = false;
                } else {
                    osc::t7000("/renoise/transport/loop/pattern", vec![rosc::OscType::Bool(false)]);
                    app.renoisepatternloop = true;
                }
            } else {
                if app.renoisepatternloop {
                    osc::t7001("/renoise/transport/loop/pattern", vec![rosc::OscType::Bool(true)]);
                    app.renoisepatternloop = false;
                } else {
                    osc::t7001("/renoise/transport/loop/pattern", vec![rosc::OscType::Bool(false)]);
                    app.renoisepatternloop = true;
                }
            }
        }
        Keycode::W => {
            app.rr = 255;
            app.gg = 255;
            app.bb = 255;
        }
        Keycode::S => {
            app.rr = 0;
            app.gg = 0;
            app.bb = 0;
        }
        Keycode::F1 => {
            app.noteScaleadd -= 1;
            println!("{}", app.noteScaleadd);
        }
        Keycode::F2 => {
            app.noteScaleadd += 1;
            println!("{}", app.noteScaleadd);
        }
        Keycode::F4 => {
            app.noteScale += 10;
            app.noteScaleaddPart = app.noteScale / 10;
            app.noteScaleadd = scale_add(
                app.noteScale,
                app.noteScalebefore,
                app.noteScaleaddPartbefore,
                app.noteScalePartStart,
            );
            if !app.noteScalePartStart {
                app.noteScalebefore = app.noteScale;
                app.noteScaleaddPartbefore = app.noteScaleaddPart;
            }
            println!("{}{}", app.noteScale, app.noteScaleadd);
        }
        Keycode::F3 => {
            app.noteScale -= 10;
            app.noteScaleaddPart = app.noteScale / 10;
            app.noteScaleadd = scale_add(
                app.noteScale,
                app.noteScalebefore,
                app.noteScaleaddPartbefore,
                app.noteScalePartStart,
            );
            if !app.noteScalePartStart {
                app.noteScalebefore = app.noteScale;
                app.noteScaleaddPartbefore = app.noteScaleaddPart;
            }
            println!("{}{}", app.noteScale, app.noteScaleadd);
        }
        Keycode::D => {
            app.rrr = app.rr;
            app.rr = app.rz;
            app.ggg = app.gg;
            app.gg = app.gz;
            app.bbb = app.bb;
            app.bb = app.bz;
        }
        Keycode::C => {
            app.rr = app.rrr;
            app.gg = app.ggg;
            app.bb = app.bbb;
        }
        Keycode::Left | Keycode::Right => {
            if kc == Keycode::Left {
                if app.noteadjust == 8 {
                    app.noteadjust = 0;
                } else {
                    app.noteadjust += 1;
                }
            } else {
                if app.noteadjust == 0 {
                    app.noteadjust = 8;
                } else {
                    app.noteadjust -= 1;
                }
            }
            if w > 0 && h > 0 && second.len() >= (w * h * 4) as usize {
                let px = w / 2;
                let pj = h / 2;
                let center = get_px(second, w, px, pj);
                let (pnote, pvol) = preview_note_vol(
                    app.noteadjust,
                    px,
                    pj,
                    w,
                    h,
                    app.noteScale,
                    app.noteScaleadd,
                    (center.r, center.g, center.b),
                );
                let corner = |x: i32, y: i32| {
                    scaled_note_at(app.noteadjust, x, y, w, h, app.noteScale, app.noteScaleadd)
                };
                let (ol, orr, ul, ur) = (
                    corner(0, 0),
                    corner(w - 1, 0),
                    corner(0, h - 1),
                    corner(w - 1, h - 1),
                );
                println!(
                    "Skala({}) Ecken: OL={} {} OR={} {} UL={} {} UR={} {}",
                    app.noteadjust,
                    ol,
                    number_to_note_name(ol),
                    orr,
                    number_to_note_name(orr),
                    ul,
                    number_to_note_name(ul),
                    ur,
                    number_to_note_name(ur)
                );
                println!(
                    "Tonauswahl: noteadjust={} Note={} {} Scale={} add={} vol={}",
                    app.noteadjust,
                    pnote,
                    number_to_note_name(pnote),
                    app.noteScale,
                    app.noteScaleadd,
                    pvol
                );
                if app.renoisenote {
                    if app.renoiseswitch {
                        osc::note_on(7000, pnote, pvol);
                        osc::trigger_midi(7002, pvol);
                    } else {
                        osc::note_on(7001, pnote, pvol);
                    }
                }
            }
        }
        Keycode::Num3 => {
            println!("Geben Sie die Anzahl der Harmonien ein: ");
            app.notessetcount = read_stdin_i32();
            app.notesset.clear();
            app.notessetx.clear();
            app.notesset.resize(app.notessetcount.max(0) as usize, Vec::new());
            app.notessetx.resize(app.notessetcount.max(0) as usize, Vec::new());
        }
        Keycode::Num4 => {
            app.harmonics = !app.harmonics;
        }
        Keycode::Num5 => {
            for c in 0..app.notessetcount.max(0) as usize {
                println!("notessetcount {} von {}", c, app.notessetcount);
                let (mx, _my) = mouse_xy();
                let px = mx;
                println!("Geben Sie die Frequenzspanne in Halbtönen ein: ");
                let fd = read_stdin_f32();
                println!("Geben Sie den Frequenzspannenteiler ein: ");
                let fdx = read_stdin_f32();
                let df = fd / fdx;
                let mut df2 = 60.0f32;
                for i in 1..=(fdx as i32) {
                    df2 = df * i as f32;
                    app.notesset[c].push(df2 as i32);
                    app.notessetx[c].push(px);
                }
                println!("{} {} {}", app.notesset.len(), app.notessetx.len(), px);
                let mut line = String::new();
                for j in 0..app.notesset[c].len().saturating_sub(1) {
                    line.push_str(&format!("{} ", app.notesset[c][j]));
                }
                println!("{}", line);
            }
        }
        Keycode::Num7 => {
            app.useNotesSet = !app.useNotesSet;
            println!("useNotesSet: {}", app.useNotesSet);
            for i in 0..app.notessetcount.max(0) as usize {
                println!("{}", i);
                let mut line = String::new();
                for j in 0..app.notesset[i].len().saturating_sub(1) {
                    line.push_str(&format!("{} ", app.notesset[i][j]));
                }
                println!("{}", line);
            }
        }
        Keycode::Num9 => {
            app.bpmmal += 1.0;
        }
        Keycode::Comma => {
            app.rec = !app.rec;
        }
        Keycode::Tab => {
            if !(keymod.contains(Mod::LSHIFTMOD) || keymod.contains(Mod::RSHIFTMOD)) {
                show_frame(app, editor, main_view, main_buffer, w, h, 1);
            }
        }
        Keycode::X => {
            app.surfacesh = !app.surfacesh;
            println!("surfacesh: {}", app.surfacesh);
        }
        Keycode::Greater => {
            app.noteValueadd -= 1;
            println!("{}", app.noteValueadd);
        }
        Keycode::R => {
            println!(" BPM?");
            app.fixedBpm2 = read_stdin_f32();
            app.fixedBpm = app.fixedBpm2;
            app.calc_bpm();
            osc::t7000(
                "/renoise/song/bpm",
                vec![rosc::OscType::Float(app.fixedBpm2)],
            );
        }
        Keycode::E => {
            if app.cc {
                app.rrange -= 1;
            } else {
                app.rr -= 1;
            }
        }
        Keycode::T => {
            if app.cc {
                app.rrange += 1;
            } else {
                app.rr += 1;
            }
        }
        Keycode::G => {
            copy_pattern_to_clipboard(app);
        }
        Keycode::F => {
            if app.cc {
                app.grange -= 1;
            } else {
                app.gg -= 1;
            }
        }
        Keycode::H => {
            if app.cc {
                app.grange += 1;
            } else {
                app.gg += 1;
            }
        }
        Keycode::B => {
            app.biialpha = !app.biialpha;
        }
        Keycode::V => {
            if app.cc {
                app.brange -= 1;
            } else {
                app.bb -= 1;
            }
        }
        Keycode::N => {
            if app.cc {
                app.brange += 1;
            } else {
                app.bb += 1;
            }
        }
        Keycode::J => {
            app.noteScaleadd += app.noteScaleaddPart;
            println!("{}", app.noteScaleadd);
        }
        Keycode::M => {
            app.noteScaleadd -= app.noteScaleaddPart;
            println!("{}", app.noteScaleadd);
        }
        Keycode::P => {
            app.bpmk = !app.bpmk;
        }
        Keycode::Y => {
            app.noteScalePartStart = !app.noteScalePartStart;
        }
        Keycode::PageUp => {
            app.iaddud += 1;
            println!("{}", app.iaddud);
        }
        Keycode::PageDown => {
            app.iaddud -= 1;
            println!("{}", app.iaddud);
        }
        Keycode::L => {
            save_rgba_bmp("frame4save.bmp", w, h, second);
            if let Some((iw, ih, img)) = load_image_to_rgba("frame4save.bmp") {
                scale_rgba_into(w, h, second, iw, ih, &img);
            }
            second_view.present(second, w, h);
            app.bpmo3 = true;
        }
        Keycode::K => {
            let new_width = w - app.leftrand - app.rightrand;
            if new_width > 0 {
                let mut new_surf = vec![0u8; (new_width * h) as usize * 4];
                for i in app.leftrand..(w - app.rightrand) {
                    for j in 0..h {
                        let px = get_px(main_buffer, w, i, j);
                        set_px(&mut new_surf, new_width, i - app.leftrand, j, px);
                    }
                }
                app.leftrand = 0;
                app.rightrand = 0;
                save_rgba_bmp("frame4.bmp", new_width, h, &new_surf);
                if let Some((iw, ih, img)) = load_image_to_rgba("frame4.bmp") {
                    scale_rgba_into(w, h, main_buffer, iw, ih, &img);
                }
                main_view.present(main_buffer, w, h);
            }
        }
        Keycode::Space => {
            read_key_event2(app);
            editor.print_current_values();
            let d = editor.shared_data();
            app.fixedBpm2 = d.bpm;
            app.bpmmal = 1.0 / d.dauer;
            app.fixedBpm = app.fixedBpm2;
            app.calc_bpm();
            osc::t7000(
                "/renoise/song/bpm",
                vec![rosc::OscType::Float(app.fixedBpm2)],
            );
        }
        Keycode::O => {
            app.bpmo = !app.bpmo;
        }
        Keycode::Semicolon => {
            app.xl += 1;
            println!("{}", app.xl);
        }
        Keycode::Underscore => {
            app.xl -= 1;
            println!("{}", app.xl);
        }
        Keycode::I => {
            unsafe {
                if let Ok(()) = x11cap::capture_clicked_window() {
                    if let Some((iw, ih, img)) = load_image_to_rgba("output.ppm") {
                        scale_rgba_into(w, h, main_buffer, iw, ih, &img);
                        save_rgba_bmp("frame0.bmp", w, h, main_buffer);
                        main_view.present(main_buffer, w, h);
                    }
                }
            }
        }
        Keycode::U => {
            app.bpmz = !app.bpmz;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Loop-Body nach der Ereignisbehandlung (Maus, bpmk/bpmo/bpmo3, surfacesh…)
// ---------------------------------------------------------------------------
fn frame_logic(
    app: &mut App,
    editor: &SharedMemoryEditor,
    video: &sdl2::VideoSubsystem,
    main_view: &mut View,
    second_view: &mut View,
    third_view: &mut View,
    main_buffer: &mut [u8],
    second: &mut Vec<u8>,
    third: &mut Vec<u8>,
    secondsave: &mut Vec<u8>,
) {
    let w = app.xw;
    let h = app.yh;

    if app.isRightMouseButtonHeld {
        fill_buf(second, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        second_view.present(second, w, h);
        third_view.present(third, w, h);
    }

    if app.isLeftMouseButtonHeld {
        let (mx, my) = mouse_xy();
        let (selr, selg, selb) = {
            let px = get_px(main_buffer, w, mx as i32, my as i32);
            (px.r, px.g, px.b)
        };
        app.selectedR = selr;
        app.selectedG = selg;
        app.selectedB = selb;
        let mut i = app.leftrand;
        while i < (w - app.rightrand) {
            if i.rem_euclid(app.iaddud.max(1)) == 0 {
                if app.biialpha {
                    let ial = app.keyEventsxyes.get(i as usize).map(|k| k.iialpha).unwrap_or(0.0);
                    if ial != 0.0 {
                        for j in 0..h {
                            let px = get_px(main_buffer, w, i, j);
                            let (r, g, b) = (px.r, px.g, px.b);
                            if !app.radier {
                                if r != app.rr as u8 && g != app.gg as u8 && b != app.bb as u8 {
                                    if r != 255 && g != 0 && b != 0 {
                                        if is_color_in_range(r, g, b, selr, selg, selb, app.rrange, app.grange, app.brange) {
                                            let ar = (r as f32 * ial) as u8;
                                            let ag = (g as f32 * ial) as u8;
                                            let ab = (b as f32 * ial) as u8;
                                            set_px(second, w, i, j, Color::RGB(ar, ag, ab));
                                        }
                                    }
                                }
                            } else {
                                set_px(second, w, i, j, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
                            }
                        }
                    }
                } else {
                    for j in 0..h {
                        let px = get_px(main_buffer, w, i, j);
                        let (r, g, b) = (px.r, px.g, px.b);
                        if !app.radier {
                            if r != app.rr as u8 && g != app.gg as u8 && b != app.bb as u8 {
                                if r != 255 && g != 0 && b != 0 {
                                    if is_color_in_range(r, g, b, selr, selg, selb, app.rrange, app.grange, app.brange) {
                                        set_px(second, w, i, j, Color::RGB(r, g, b));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        set_px(second, w, 0, 0, Color::RGB(1, 1, 1));
        second_view.present(second, w, h);
        main_view.present(main_buffer, w, h);
    }

    if app.isMiddleMouseButtonHeld {
        fill_buf(second, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        second_view.present(second, w, h);
        app.l = 0;
        app.lb = 0;
        app.pixi = 0;
        app.pixj = 0;
        let (mx, my) = mouse_xy();
        let (selr, selg, selb) = {
            let px = get_px(main_buffer, w, mx, my);
            (px.r, px.g, px.b)
        };
        app.selectedR = selr;
        app.selectedG = selg;
        app.selectedB = selb;
        let mut i = app.leftrand;
        while i < (w - app.rightrand) {
            let mut iadd2 = 1;
            if app.iaddf && !app.xValuesfar.is_empty() {
                let indexi = (i as usize * app.xValuesfar.len()) / (w - app.rightrand - app.leftrand).max(1) as usize;
                if indexi < app.xValuesfar.len() {
                    iadd2 = app.xValuesfar[indexi];
                }
            }
            for j in 0..h {
                let denom = app.iadd * iadd2;
                let im = if denom == 0 { 0 } else { i % denom };
                let jm = if app.jadd == 0 { 0 } else { j % app.jadd };
                if im == 0 && jm == 0 {
                    let px = get_px(main_buffer, w, i, j);
                    let (r, g, b) = (px.r, px.g, px.b);
                    if is_color_in_range(r, g, b, selr, selg, selb, app.rrange, app.grange, app.brange) {
                        set_px(second, w, i, j, Color::RGB(r, g, b));
                    }
                    app.l += 1;
                }
                app.pixi += 1;
            }
            app.pixj += 1;
            i += 1;
        }
        *secondsave = second.clone();
        if app.harmonics {
            find_and_draw_harmonics(app, second, secondsave, w, h);
        }
        second_view.present(second, w, h);
        main_view.present(main_buffer, w, h);
    }

    if app.leftset {
        let (mx, _my) = mouse_xy();
        app.leftrand = mx;
        println!("{}", app.leftrand);
        app.calc_bpm();
        app.leftset = false;
    }
    if app.rightset {
        let (mx, _my) = mouse_xy();
        app.rightrand = app.xw - mx;
        println!("{}", app.rightrand);
        app.calc_bpm();
        app.rightset = false;
    }

    if app.sendbpm {
        println!("{}", app.fixedBpm2);
        app.funcalc = true;
        app.sendbpm = false;
    }

    if app.bpmk {
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        third_view.present(third, w, h);
        for i in app.leftrand..(w - app.rightrand) {
            for j in 0..h {
                let start = Instant::now();
                let px = get_px(secondsave, w, i, j);
                let (r, g, b) = (px.r, px.g, px.b);
                if r != app.rr as u8 || g != app.gg as u8 || b != app.bb as u8 {
                    if r != 255 || g != 0 || b != 0 {
                        app.noteValue = (j * app.noteScale / h) + app.noteScaleadd;
                        if app.is_note_in_chordpatt(app.noteValue, i, j, h) {
                            set_px(third, w, i, j, Color::RGB(r, g, b));
                            third_view.present(third, w, h);
                            app.noteValue = (j * app.noteScale / h) + app.noteScaleadd;
                            if app.renoisenote {
                                if app.renoiseswitch {
                                    osc::note_on(7000, app.noteValue, 127);
                                } else {
                                    osc::note_on(7001, app.noteValue, 127);
                                }
                            }
                        }
                    }
                }
                let elapsed = start.elapsed().as_nanos() as i64;
                sleep_ns(app.timePerPixelns - elapsed);
            }
        }
        app.bpmk = false;
    }

    if app.bpmo {
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        fill_buf(third, Color::RGB(app.rr as u8, app.gg as u8, app.bb as u8));
        third_view.present(third, w, h);

        if app.renoiseplay {
            osc::transport_start();
        }
        if app.rec {
            for _ in 0..3 {
                osc::note_on(7000, 65, 128);
            }
        }

        for n in 0..app.keyEventsxyrgbt.len() {
            let start = Instant::now();
            let ev = app.keyEventsxyrgbt[n];
            if n == 0 && ev.p != 0 {
                let el = start.elapsed().as_nanos() as i64;
                sleep_ns((ev.p as i64 * app.timePerPixelns) - el);
            } else {
                let mut volume = 127;
                if app.noteadjust == 0 {
                    let (nv, vol) = color_to_note_value_and_volume(
                        ev.r,
                        ev.g,
                        ev.b,
                        app.noteScale,
                        app.noteScaleadd,
                    );
                    app.noteValue = nv;
                    volume = vol;
                    println!("Note value: {}, Volume: {}", nv, vol);
                } else if app.noteadjust == 2 {
                    app.noteValue = (ev.y * app.noteScale / h) + app.noteScaleadd;
                    volume = 127 - (ev.x * 127 / w);
                } else if app.noteadjust == 3 || app.noteadjust == 4 {
                    app.noteValue = app.noteScale - (ev.y * app.noteScale / h) + app.noteScaleadd;
                    volume = if app.noteadjust == 3 {
                        ev.x * 127 / w
                    } else {
                        127 - (ev.x * 127 / w)
                    };
                } else if app.noteadjust == 5 || app.noteadjust == 6 {
                    app.noteValue = (ev.x * app.noteScale / w) + app.noteScaleadd;
                    volume = if app.noteadjust == 5 {
                        ev.x * 127 / w
                    } else {
                        127 - (ev.x * 127 / w)
                    };
                } else if app.noteadjust == 7 || app.noteadjust == 8 {
                    app.noteValue = app.noteScale - (ev.x * app.noteScale / w) + app.noteScaleadd;
                    volume = if app.noteadjust == 7 {
                        ev.x * 127 / w
                    } else {
                        127 - (ev.x * 127 / w)
                    };
                } else if app.noteadjust == 1 {
                    app.noteValue = app.noteScale - (ev.y * app.noteScale / h) + app.noteScaleadd;
                    volume = ev.a;
                }

                if app.is_note_in_chordpatt(app.noteValue, ev.x, ev.y, h) {
                    set_px(third, w, ev.x, ev.y, Color::RGB(ev.r, ev.g, ev.b));
                    third_view.present(third, w, h);
                }
                if app.renoisenote {
                    if app.renoiseswitch {
                        println!("{} noteValue: {} Volume: {}", app.noteadjust, app.noteValue, volume);
                        osc::note_on(7000, app.noteValue, volume);
                        osc::trigger_midi(7002, volume);
                    } else {
                        osc::note_on(7001, app.noteValue, volume);
                    }
                }
            }
            let el = start.elapsed().as_nanos() as i64;
            sleep_ns((ev.t as i64 * app.timePerPixelns) - el);
        }

        if app.rec {
            for _ in 0..3 {
                osc::note_on(7000, 65, 128);
            }
        }
        if app.renoiseplay {
            osc::transport_stop();
            app.renoiseplay = false;
        }
        app.bpmo = false;
    }

    if app.bpmo3 {
        app.xValueschords.clear();
        app.keyEventsxyrgbt.clear();
        let mut p = 0;
        let mut ibefore = -1;
        let empty = KeyEventxyes::default();
        if app.biialpha {
            for i in 0..w {
                for j in 0..h {
                    let ke = app.keyEventsxyes.get(i as usize).copied().unwrap_or(empty);
                    if ke.key == 1 && ibefore != i {
                        app.xValueschords.push(i as f32);
                        ibefore = i;
                    }
                    if ke.iialpha != 0.0 || ke.key == 1 {
                        let px = get_px(secondsave, w, i, j);
                        let (r, g, b) = (px.r, px.g, px.b);
                        if r != app.rr as u8 && g != app.gg as u8 && b != app.bb as u8 {
                            if r != 255 && g != 0 && b != 0 {
                                if app.is_note_in_chordpatt(app.noteValue, i, j, h) {
                                    app.keyEventsxyrgbt.push(KeyEventxyrgbt {
                                        x: i,
                                        y: j,
                                        r,
                                        g,
                                        b,
                                        p,
                                        t: 0,
                                        a: (ke.iialpha * 127.0) as i32,
                                    });
                                }
                            }
                        }
                    }
                    p += 1;
                }
                println!("{}", p);
            }
        } else {
            for i in 0..w {
                for j in 0..h {
                    let px = get_px(secondsave, w, i, j);
                    let (r, g, b) = (px.r, px.g, px.b);
                    if r != app.rr as u8 && g != app.gg as u8 && b != app.bb as u8 {
                        if r != 255 && g != 0 && b != 0 {
                            if app.is_note_in_chordpatt(app.noteValue, i, j, h) {
                                app.keyEventsxyrgbt.push(KeyEventxyrgbt {
                                    x: i,
                                    y: j,
                                    r,
                                    g,
                                    b,
                                    p,
                                    t: 0,
                                    a: 127,
                                });
                            }
                        }
                    }
                    p += 1;
                }
                println!("{}", p);
            }
        }
        println!("{}", p);
        let total_pixels = w * h;
        let n = app.keyEventsxyrgbt.len();
        for i in 0..n {
            let pt = if i == n - 1 {
                total_pixels - app.keyEventsxyrgbt[i].p
            } else {
                app.keyEventsxyrgbt[i + 1].p - app.keyEventsxyrgbt[i].p
            };
            app.keyEventsxyrgbt[i].t = pt;
        }
        let mut spt = 0;
        for i in 0..n {
            spt += app.keyEventsxyrgbt[i].t;
            println!(
                "x {} p {} {}",
                app.keyEventsxyrgbt[i].x,
                app.keyEventsxyrgbt[i].p,
                app.keyEventsxyrgbt[i].t
            );
        }
        println!("{}", spt);
        for x in &app.xValueschords {
            println!("{}", x);
        }
        app.bpmo3 = false;
    }

    if app.cvcamload {
        if let Some((iw, ih, img)) = load_image_to_rgba("savesurf.bmp") {
            scale_rgba_into(w, h, main_buffer, iw, ih, &img);
            main_view.present(main_buffer, w, h);
            std::thread::sleep(Duration::from_nanos(1000000));
        }
    }
    if app.surfacesh {
        let (hdr, pix) = editor.image_header_and_pixels();
        let sw = hdr.width;
        let sh = hdr.height;
        if sw > 0 && sh > 0 {
            let buf = convert_to_rgba(
                pix,
                sw,
                sh,
                hdr.pitch,
                hdr.bpp,
                hdr.rmask,
                hdr.gmask,
                hdr.bmask,
                hdr.amask,
            );
            let mut sh_view = make_view(video, "Surface View", sw, sh, 960, 0);
            sh_view.present(&buf, sw, sh);
            scale_rgba_into(w, h, main_buffer, sw, sh, &buf);
            main_view.present(main_buffer, w, h);
            wait_enter();
            drop(sh_view);
        }
        app.surfacesh = false;
    }
}

// ---------------------------------------------------------------------------
// Hauptprogramm
// ---------------------------------------------------------------------------
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let frames_dir = resolve_frames_dir(&args);
    let filenames = read_filenames(&frames_dir);

    println!("Bildbreite in pixel? ");
    println!("Bildhoehe in pixel? ");
    let (xw, yh) = read_two_i32();

    let mut app = App::new(xw, yh);
    app.frames_dir = frames_dir;
    app.filenames = filenames;

    let sdl = match sdl2::init() {
        Ok(s) => s,
        Err(e) => {
            println!("Failed to initialize SDL: {e}");
            std::process::exit(1);
        }
    };
    let video = match sdl.video() {
        Ok(v) => v,
        Err(e) => {
            println!("Failed: {e}");
            std::process::exit(1);
        }
    };
    let mut main_buffer = vec![0u8; (xw * yh) as usize * 4];
    let mut second = vec![0u8; (xw * yh) as usize * 4];
    let mut third = vec![0u8; (xw * yh) as usize * 4];
    let mut secondsave = vec![0u8; (xw * yh) as usize * 4];

    let mut main_view = make_view(&video, "Picture", xw, yh, 3840 - xw, 0);
    let mut second_view = make_view(&video, "Selected color", xw, yh, 3840 - xw, 480);
    let mut third_view = make_view(&video, "Selected color", xw, yh, 3840 - xw, 960);

    if let Some((iw, ih, img)) = load_image_to_rgba("frame.bmp") {
        scale_rgba_into(xw, yh, &mut main_buffer, iw, ih, &img);
    }
    main_view.present(&main_buffer, xw, yh);
    second_view.present(&second, xw, yh);
    third_view.present(&third, xw, yh);

    let mut editor = match SharedMemoryEditor::open() {
        Ok(e) => e,
        Err(err) => {
            println!("Failed to open shared memory: {err}");
            std::process::exit(1);
        }
    };
    let _ = editor.print_current_values();

    inject_test_keys();
    let mut quit = false;
    while !quit {
        match poll_event_compat() {
            Some(Event::Quit { .. }) => {
                quit = true;
            }
            Some(Event::KeyDown {
                keycode,
                scancode,
                keymod,
                ..
            }) => {
                handle_key(
                    &mut app,
                    &mut editor,
                    &mut main_view,
                    &mut second_view,
                    &mut third_view,
                    &mut main_buffer,
                    &mut second,
                    &mut third,
                    &mut secondsave,
                    keycode,
                    scancode,
                    keymod,
                );
            }
            Some(Event::MouseButtonDown { mouse_btn, .. }) => {
                match mouse_btn {
                    sdl2::mouse::MouseButton::Left => {
                        app.isLeftMouseButtonHeld = true;
                        app.isRightMouseButtonHeld = false;
                        app.isMiddleMouseButtonHeld = false;
                    }
                    sdl2::mouse::MouseButton::Right => {
                        app.isRightMouseButtonHeld = true;
                        app.isLeftMouseButtonHeld = false;
                        app.isMiddleMouseButtonHeld = false;
                    }
                    sdl2::mouse::MouseButton::Middle => {
                        app.isMiddleMouseButtonHeld = true;
                        app.isLeftMouseButtonHeld = false;
                        app.isRightMouseButtonHeld = false;
                    }
                    _ => {}
                }
            }
            Some(Event::MouseButtonUp { mouse_btn, .. }) => {
                match mouse_btn {
                    sdl2::mouse::MouseButton::Left => {
                        app.isLeftMouseButtonHeld = false;
                    }
                    sdl2::mouse::MouseButton::Right => {
                        app.isRightMouseButtonHeld = false;
                    }
                    sdl2::mouse::MouseButton::Middle => {
                        app.isMiddleMouseButtonHeld = false;
                    }
                    _ => {}
                }
            }
            _ => {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        frame_logic(
            &mut app,
            &mut editor,
            &video,
            &mut main_view,
            &mut second_view,
            &mut third_view,
            &mut main_buffer,
            &mut second,
            &mut third,
            &mut secondsave,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_center_pixel_in_mode_zero() {
        assert_eq!(
            preview_note_vol(0, 0, 0, 100, 100, 120, 0, (255, 0, 0)),
            (120, 42)
        );
    }

    #[test]
    fn preview_vertical_and_horizontal_modes() {
        assert_eq!(
            preview_note_vol(1, 0, 0, 100, 100, 120, 0, (0, 0, 0)),
            (120, 127)
        );
        assert_eq!(
            preview_note_vol(2, 0, 50, 100, 100, 120, 0, (0, 0, 0)),
            (60, 127)
        );
        assert_eq!(
            preview_note_vol(2, 100, 50, 100, 100, 120, 0, (0, 0, 0)),
            (60, 0)
        );
        assert_eq!(
            preview_note_vol(5, 50, 0, 100, 100, 120, 0, (0, 0, 0)),
            (60, 63)
        );
        assert_eq!(
            preview_note_vol(7, 50, 0, 100, 100, 120, 0, (0, 0, 0)),
            (60, 63)
        );
        assert_eq!(
            preview_note_vol(3, 100, 0, 100, 100, 120, 0, (0, 0, 0)),
            (120, 127)
        );
    }

    #[test]
    fn preview_clamps_note_range() {
        assert_eq!(
            preview_note_vol(1, 0, 0, 100, 100, 200, 100, (0, 0, 0)).0,
            127
        );
        assert_eq!(
            preview_note_vol(2, 0, 100, 100, 100, 200, -250, (0, 0, 0)).0,
            0
        );
    }

    #[test]
    fn next_frame_index_wraps_in_both_directions() {
        assert_eq!(next_frame_index(0, 3, 1), Some(1));
        assert_eq!(next_frame_index(2, 3, 1), Some(0));
        assert_eq!(next_frame_index(0, 3, -1), Some(2));
        assert_eq!(next_frame_index(1, 3, -1), Some(0));
        assert_eq!(next_frame_index(0, 0, 1), None);
    }

    #[test]
    fn convert_to_rgba_handles_rgba_and_bgra_masks() {
        let rgba = vec![1u8, 2, 3, 4];
        assert_eq!(
            convert_to_rgba(
                &rgba,
                1,
                1,
                4,
                32,
                0x000000ff,
                0x0000ff00,
                0x00ff0000,
                0xff000000
            ),
            vec![1, 2, 3, 4]
        );
        let bgra = vec![3u8, 2, 1, 4];
        assert_eq!(
            convert_to_rgba(
                &bgra,
                1,
                1,
                4,
                32,
                0x00ff0000,
                0x0000ff00,
                0x000000ff,
                0xff000000
            ),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn convert_to_rgba_respects_pitch_and_short_buffers() {
        let padded = vec![1u8, 2, 3, 4, 9, 9, 9, 9];
        let out = convert_to_rgba(
            &padded,
            1,
            1,
            8,
            32,
            0x000000ff,
            0x0000ff00,
            0x00ff0000,
            0xff000000,
        );
        assert_eq!(out, vec![1, 2, 3, 4]);
        assert_eq!(
            convert_to_rgba(&[], 1, 1, 4, 32, 0xff, 0xff00, 0xff0000, 0xff000000),
            vec![0, 0, 0, 0]
        );
    }

    #[test]
    fn wh_from_tokens_accepts_one_or_two_lines() {
        assert_eq!(wh_from_tokens(&[384, 384]), (384, 384));
        assert_eq!(wh_from_tokens(&[384]), (384, 0));
        assert_eq!(wh_from_tokens(&[]), (0, 0));
    }

    #[test]
    fn filter_image_files_keeps_supported_extensions() {
        let dir = std::env::temp_dir().join(format!("rrn5_filter_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a.PNG", "b.jpg", "c.txt", "d.bmp", "e.jpeg"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let files = filter_image_files(dir.to_str().unwrap());
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
        assert_eq!(names, vec!["a.PNG", "b.jpg", "d.bmp", "e.jpeg"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renoise_xml_has_unique_line_indices_and_nested_notes() {
        let notes = vec![
            (0, "C-4".to_string()),
            (0, "D-4".to_string()),
            (2, "E-4".to_string()),
        ];
        let xml = build_renoise_xml(4, &notes);
        for i in 0..4 {
            let tag = format!("<Line index=\"{i}\"");
            assert_eq!(
                xml.matches(&tag).count(),
                1,
                "Line {i} muss genau einmal vorkommen"
            );
        }
        assert_eq!(xml.matches("<NoteColumns>").count(), 3);
        assert_eq!(xml.matches("<Note>C-4</Note>").count(), 1);
        assert_eq!(xml.matches("<Instrument>00</Instrument>").count(), 3);
        assert!(xml.contains("</Lines>"));
        assert!(xml.contains("<ColumnType>NoteColumn</ColumnType>"));
        assert!(xml.contains("<SubColumnMask>true true true false false false false false</SubColumnMask>"));
        let empty = build_renoise_xml(2, &[]);
        assert_eq!(empty.matches("<Line index=").count(), 2);
        assert!(!empty.contains("<NoteColumns>"));
    }

    #[test]
    fn harmonic_pixel_eligible_matches_cpp() {
        assert!(!harmonic_pixel_eligible(255, 0, 0));
        assert!(!harmonic_pixel_eligible(0, 255, 0));
        assert!(!harmonic_pixel_eligible(0, 0, 255));
        assert!(!harmonic_pixel_eligible(10, 0, 10));
        assert!(harmonic_pixel_eligible(10, 10, 10));
        assert!(harmonic_pixel_eligible(254, 1, 1));
    }

    #[test]
    fn bpm_from_uses_base_offset() {
        assert_eq!(bpm_from(121.0, 1.0, 1.0), 121.0);
        assert_eq!(bpm_from(120.0, 2.0, 1.0), 240.0);
        assert_eq!(bpm_from(120.0, 1.0, 0.5), 60.0);
    }

    #[test]
    fn scale_add_matches_cpp_formula() {
        assert_eq!(scale_add(120, 100, 10, true), 0);
        assert_eq!(scale_add(60, 100, 10, true), 36);
        assert_eq!(scale_add(60, 120, 24, false), 24);
        assert_eq!(scale_add(60, 120, 16, false), 40);
    }

    #[test]
    fn scale_add_survives_zero_part_before() {
        assert_eq!(scale_add(60, 120, 0, false), 36);
    }

    #[test]
    fn scaled_note_at_clamps_and_maps_modes() {
        assert_eq!(scaled_note_at(0, 5, 5, 100, 100, 120, 0), 60);
        assert_eq!(scaled_note_at(2, 0, 50, 100, 100, 120, 0), 60);
        assert_eq!(scaled_note_at(5, 50, 0, 100, 100, 120, 0), 60);
        assert_eq!(scaled_note_at(7, 50, 0, 100, 100, 120, 0), 60);
        assert_eq!(scaled_note_at(2, 0, 200, 100, 100, 120, 0), 127);
    }
}
