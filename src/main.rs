mod openmpt;

use openmpt::Mod;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
    const SAMPLE_RATE: c_int = 44100;

/// Beat-Erkennung für taktkompatible Order-Wechsel (offline und live identisch).
struct BeatTracker {
    prev_row: i32,
    beat_count: u32,
    creative: u8,
}

impl BeatTracker {
    fn new() -> Self {
        BeatTracker {
            prev_row: -1,
            beat_count: 0,
            creative: 0,
        }
    }

    /// Liefert `Some(creative)` wenn eine 4er-Phrase beginnt, sonst None.
    fn observe(&mut self, row: i32, rpb: i32) -> Option<u8> {
        let result = if self.prev_row < 0 {
            None
        } else if row % rpb == 0 && row != self.prev_row {
            self.beat_count += 1;
            if self.beat_count % 4 == 0 {
                self.creative = self.creative.wrapping_add(1);
                Some(self.creative)
            } else {
                None
            }
        } else {
            None
        };
        if row != self.prev_row {
            self.prev_row = row;
        }
        result
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Verwendung: it_live <datei.it> [--dump] [--wav out.wav] [--device NAME] [--seconds N]"
        );
        std::process::exit(1);
    }
    let path = &args[1];
    let dump = args.iter().any(|a| a == "--dump");
    let wav_out = args.iter().position(|a| a == "--wav").map(|i| args[i + 1].clone());

    let m = Mod::from_file(path).expect("Modul laden");
    println!("=== Loaded ===");
    println!("{}", m.info());
    let ns = unsafe { openmpt::openmpt_module_get_num_samples(m.ptr) };
    for s in 0..ns {
        let n = m.sample_name(s);
        println!("  sample {s}: {n}");
    }

    if dump {
        dump_structure(&m);
    }

    if let Some(out) = wav_out {
        render_variation(&m, &out);
    } else {
        let device = args.iter().position(|a| a == "--device").map(|i| args[i + 1].clone());
        let seconds = args
            .iter()
            .position(|a| a == "--seconds")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok());
        if let Err(e) = live_playback(&m, device, seconds) {
            eprintln!("Live-Wiedergabe fehlgeschlagen: {e}");
            std::process::exit(1);
        }
    }
}

struct ModSend(*mut openmpt::openmpt_module);
unsafe impl Send for ModSend {}

impl ModSend {
    fn ptr(&self) -> *mut openmpt::openmpt_module {
        self.0
    }
}

/// Live-Wiedergabe über cpal mit taktkompatiblen Order-Wechseln.
fn live_playback(m: &Mod, device_name: Option<String>, seconds: Option<f64>) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => {
            let devices: Vec<cpal::Device> = host
                .output_devices()
                .map_err(|e| format!("Geräteliste: {e}"))?
                .collect();
            let found = devices
                .iter()
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .cloned();
            match found {
                Some(d) => d,
                None => {
                    let names: Vec<String> = devices
                        .iter()
                        .filter_map(|d| d.name().ok())
                        .collect();
                    return Err(format!(
                        "Gerät '{name}' nicht gefunden. Verfügbar: {}",
                        names.join(", ")
                    ));
                }
            }
        }
        None => host
            .default_output_device()
            .ok_or_else(|| "kein Standard-Ausgabegerät".to_string())?,
    };
    let name = device.name().unwrap_or_else(|_| "<unbekannt>".to_string());
    let supported = device
        .default_output_config()
        .map_err(|e| format!("Ausgabekonfiguration: {e}"))?;
    let sample_rate = supported.sample_rate().0 as i32;
    let channels = supported.channels() as usize;
    let stream_config: cpal::StreamConfig = supported.into();
    println!("Live-Wiedergabe auf: {name} ({sample_rate} Hz, {channels} Kanäle)");

    let cur_pat = unsafe { openmpt::openmpt_module_get_current_pattern(m.ptr) };
    let rpb = unsafe { openmpt::openmpt_module_get_pattern_rows_per_beat(m.ptr, cur_pat) }.max(1);
    let num_orders = unsafe { openmpt::openmpt_module_get_num_orders(m.ptr) };

    let chunk = 512usize;
    let mod_ptr = ModSend(m.ptr);
    let mut left = vec![0.0f32; chunk];
    let mut right = vec![0.0f32; chunk];
    let mut tracker = BeatTracker::new();

    let stream = device
        .build_output_stream(
            &stream_config,
            move |out: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                let m = mod_ptr.ptr();
                let mut idx = 0usize;
                while idx < out.len() {
                    let avail = (out.len() - idx) / channels;
                    if avail == 0 {
                        for s in &mut out[idx..] {
                            *s = 0.0;
                        }
                        break;
                    }
                    let n = unsafe {
                        openmpt::openmpt_module_read_stereo(
                            m,
                            sample_rate,
                            chunk.min(avail),
                            left.as_mut_ptr() as *mut _,
                            right.as_mut_ptr() as *mut _,
                        )
                    } as usize;
                    if n == 0 {
                        for s in &mut out[idx..] {
                            *s = 0.0;
                        }
                        break;
                    }
                    let cur_row = unsafe { openmpt::openmpt_module_get_current_row(m) };
                    let cur_order = unsafe { openmpt::openmpt_module_get_current_order(m) };
                    if let Some(creative) = tracker.observe(cur_row, rpb) {
                        let target = (creative as i32) % 8;
                        for o in cur_order + 1..num_orders {
                            let op = unsafe { openmpt::openmpt_module_get_order_pattern(m, o) };
                            if op == target {
                                unsafe {
                                    openmpt::openmpt_module_set_position_order_row(m, o, cur_row);
                                }
                                println!(
                                    "BEAT-Trigger (live): Order {cur_order} -> {o} Pattern {op} | rpb={rpb} row={cur_row}"
                                );
                                break;
                            }
                        }
                    }
                    for i in 0..n {
                        for c in 0..channels {
                            out[idx + i * channels + c] = if c % 2 == 0 { left[i] } else { right[i] };
                        }
                    }
                    idx += n * channels;
                }
            },
            |err| eprintln!("Audio-Fehler: {err}"),
            None,
        )
        .map_err(|e| format!("Stream: {e}"))?;
    stream.play().map_err(|e| format!("Play: {e}"))?;

    match seconds {
        Some(secs) => std::thread::sleep(std::time::Duration::from_secs_f64(secs.max(0.0) + 0.3)),
        None => {
            println!("Live läuft – Enter beendet.");
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
        }
    }
    drop(stream);
    println!("Live-Wiedergabe beendet.");
    Ok(())
}

fn dump_structure(m: &Mod) {
    let pat = unsafe { openmpt::openmpt_module_get_num_patterns(m.ptr) };
    let ord = unsafe { openmpt::openmpt_module_get_num_orders(m.ptr) };
    let ch = unsafe { openmpt::openmpt_module_get_num_channels(m.ptr) };
    println!("\n=== Struktur ===");
    println!("patterns={pat} orders={ord} channels={ch}");
    for p in 0..pat {
        let rows = unsafe { openmpt::openmpt_module_get_pattern_num_rows(m.ptr, p) };
        println!("  Pattern {p}: rows={rows}");
    }
    println!("Orders (Break=65534=0xFFFE):");
    for o in 0..ord {
        let p = unsafe { openmpt::openmpt_module_get_order_pattern(m.ptr, o) };
        if p == 65534 || p == 0xfffe {
            println!("  Order {o}: <BREAK>");
        } else {
            println!("  Order {o}: Pattern {p}");
        }
    }
}

// ---------------------------------------------------------------------------
// Takt-kompatible Variation: rendert das Modul und steuert Order-/Pattern-Switch
// NUR an Beatgrenzen (rows_per_beat), damit der Groove intakt bleibt.
// ---------------------------------------------------------------------------

fn render_variation(m: &Mod, out_path: &str) {
    let channels = unsafe { openmpt::openmpt_module_get_num_channels(m.ptr) };
    let _ = channels;
    // Länge: optional überschreibbar via ENV RLENGTH (Sekunden)
    let total_seconds: f64 = std::env::var("RLENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6.0 * 60.0 + 40.0);
    let cur_pat = unsafe { openmpt::openmpt_module_get_current_pattern(m.ptr) };
    let rpb = unsafe { openmpt::openmpt_module_get_pattern_rows_per_beat(m.ptr, cur_pat) };
    let rpb =     if rpb <= 0 { 32 } else { rpb };

    // Buffer
    let chunk: usize = 512;
    let total_frames: usize = (total_seconds * SAMPLE_RATE as f64) as usize;
    let mut left = Vec::with_capacity(total_frames + chunk);
    let mut right = Vec::with_capacity(total_frames + chunk);
    unsafe { left.set_len(total_frames + chunk) };
    unsafe { right.set_len(total_frames + chunk) };

    // Einfaches Beatgrid: jede Pattern-Änderung nur auf Row % rpb == 0
    let mut written = 0usize;
    let mut tracker = BeatTracker::new();

    while written < total_frames {
        let n = unsafe {
            openmpt::openmpt_module_read_stereo(
                m.ptr,
                SAMPLE_RATE,
                chunk,
                left[written..].as_mut_ptr() as *mut _,
                right[written..].as_mut_ptr() as *mut _,
            ) as usize
        };
        if n == 0 {
            break;
        }

        let cur_row = unsafe { openmpt::openmpt_module_get_current_row(m.ptr) };
        let cur_order = unsafe { openmpt::openmpt_module_get_current_order(m.ptr) };
        if let Some(creative) = tracker.observe(cur_row, rpb) {
            let order_patterns: Vec<i32> = (0..8).collect();
            let target = order_patterns[(creative as usize) % order_patterns.len()];
            let num_orders = unsafe { openmpt::openmpt_module_get_num_orders(m.ptr) };
            for o in cur_order + 1..num_orders {
                let op = unsafe { openmpt::openmpt_module_get_order_pattern(m.ptr, o) };
                if op == target {
                    m.set_order(o, cur_row);
                    println!(
                        "BEAT-Trigger ({}s): Order {cur_order} -> {o} Pattern {op} | rpb={rpb} row={cur_row}",
                        written as f64 / SAMPLE_RATE as f64
                    );
                    break;
                }
            }
        }

        written += n;
    }

    left.truncate(written);
    right.truncate(written);
    write_wav(out_path, &left, &right);
    println!("WAV geschrieben: {out_path} ({} frames)", written);
}

// ---------------------------------------------------------------------------

fn write_wav(path: &str, left: &[f32], right: &[f32]) {
    use std::io::Write;
    let samples = 2 * left.len();
    let bytes = samples * 2;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + bytes as u32).to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&2u16.to_le_bytes()).unwrap(); // stereo
    f.write_all(&(SAMPLE_RATE as u32).to_le_bytes()).unwrap();
    f.write_all(&(SAMPLE_RATE as u32 * 4).to_le_bytes()).unwrap(); // byte rate
    f.write_all(&4u16.to_le_bytes()).unwrap(); // block align
    f.write_all(&16u16.to_le_bytes()).unwrap(); // bits
    f.write_all(b"data").unwrap();
    f.write_all(&(bytes as u32).to_le_bytes()).unwrap();
    for i in 0..left.len() {
        for v in [left[i], right[i]] {
            let s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_tracker_returns_phrase_boundaries() {
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(0, 4), None);
        assert_eq!(bt.observe(1, 4), None);
        assert_eq!(bt.observe(4, 4), None);
        assert_eq!(bt.observe(8, 4), None);
        assert_eq!(bt.observe(12, 4), None);
        assert_eq!(bt.observe(16, 4), Some(1));
        assert_eq!(bt.observe(20, 4), None);
        assert_eq!(bt.observe(24, 4), None);
        assert_eq!(bt.observe(28, 4), None);
        assert_eq!(bt.observe(32, 4), Some(2));
    }

    #[test]
    fn beat_tracker_ignores_repeated_rows_and_uses_rpb() {
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(3, 4), None);
        assert_eq!(bt.observe(3, 4), None);
        assert_eq!(bt.observe(4, 4), None);
        assert_eq!(bt.observe(8, 4), None);
        assert_eq!(bt.observe(12, 4), None);
        assert_eq!(bt.observe(16, 4), Some(1));
    }

    #[test]
    fn beat_tracker_respects_other_rpb() {
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(0, 8), None);
        for row in [8, 16, 24] {
            assert_eq!(bt.observe(row, 8), None);
        }
        assert_eq!(bt.observe(32, 8), Some(1));
        assert_eq!(bt.observe(40, 8), None);
    }
}
