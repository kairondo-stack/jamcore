// 1:1-Port der Datenstrukturen aus pattern_recorder_202601.cpp

#[derive(Clone, Debug, Default)]
pub struct Note {
    pub timestamp: f32,
    pub instrument: i32,
    pub pitch: i32,
    pub scl: i32,
    pub p2: f32,
    pub mschrittweitanz: bool,
    pub schrittweitanz: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Note2 {
    pub timestamp: f32,
    pub instrument: i32,
    pub pitch: i32,
    pub glide: bool,
    pub p2: f32,
    pub mschrittweitanz: bool,
    pub schrittweitanz: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note3 {
    pub timestamp: f32,
    pub instrument: i32,
    pub pitch: i32,
    pub glide: bool,
    pub p2: f32,
}

impl Default for Note3 {
    fn default() -> Self {
        Note3 {
            timestamp: 0.0,
            instrument: 0,
            pitch: 0,
            glide: false,
            p2: 0.0,
        }
    }
}

impl Note3 {
    pub fn new(t: f32, inst: i32, p: i32) -> Self {
        Note3 {
            timestamp: t,
            instrument: inst,
            pitch: p,
            glide: false,
            p2: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Notescale {
    pub timestamp1: f32,
    pub timestamp2: f32,
    pub notev2: f32,
    pub notev1: f32,
    pub start_indexrecord: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Modu {
    pub time: f32,
    pub instrument: i32,
    pub pitch: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Xn {
    pub i: f32,
    pub xn: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Xy {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Default)]
pub struct VecForRec {
    pub recsteps: i32,
    pub recstep: i32,
    pub an_ix_um: f32,
    pub en_ix_um: f32,
    pub steps: i32,
    pub kis: i32,
    pub start_time: f32,
    pub end_time: f32,
}

#[derive(Clone, Debug, Default)]
pub struct PlayFofoHi {
    pub current_indices: u32,
    pub start_times: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Festerfester {
    pub start_time: f32,
    pub end_time: f32,
    pub start_indexnum: i32,
    pub end_indexnum: i32,
    pub schrittweitanz: i32,
    pub firstrec: bool,
}

pub const NOTE_NAMES: [&str; 12] = [
    "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
];

/// convertMidiNoteToRenoiseNote
pub fn midi_to_renoise_note(midi_note: i32) -> String {
    if midi_note < 0 {
        return "Off".to_string();
    }
    let note_number = midi_note % 12;
    let octave = midi_note / 12 - 1;
    format!("{}{}", NOTE_NAMES[note_number as usize], octave)
}

/// convertRenoiseNoteToMidi – parst auch Kreuz-Noten (C#4), die im C++-Port
/// durch nicht erreichbare Multi-Char-Cases verloren gingen.
pub fn renoise_note_to_midi(note_str: &str) -> i32 {
    if note_str == "Off" {
        return -1;
    }
    let bytes = note_str.as_bytes();
    if bytes.is_empty() {
        return -1;
    }
    let letter = bytes[0] as char;
    let sharp = bytes.get(1) == Some(&b'#');
    let rest_idx = 2;
    let octave: i32 = note_str[rest_idx.min(note_str.len())..]
        .trim()
        .parse()
        .unwrap_or(0);
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => 0,
    };
    let note_number = if sharp { base + 1 } else { base };
    (octave + 1) * 12 + note_number
}

pub fn gcd(mut a: i32, mut b: i32) -> i32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// floatToFraction – exakt wie C++ (best_error startet bei |value-1.0|, bricht bei epsilon ab).
pub fn float_to_fraction(value: f64, max_denominator: u32, epsilon: f64) -> (i32, i32) {
    let mut best_num = 1_i32;
    let mut best_den = 1_i32;
    let mut best_error = (value - 1.0).abs();

    for den in 1..=max_denominator {
        let numer = (value * den as f64).round() as i32;
        let error = (value - numer as f64 / den as f64).abs();
        if error < best_error {
            best_error = error;
            best_num = numer;
            best_den = den as i32;
        }
        if error < epsilon {
            break;
        }
    }
    let divisor = gcd(best_num, best_den);
    (best_num / divisor, best_den / divisor)
}

pub fn rhythm_fraction(timestampvalue: f64) -> String {
    let (a, b) = float_to_fraction(timestampvalue, 192, 1e-4);
    format!("{a}/{b}")
}