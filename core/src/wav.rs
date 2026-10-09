//! WAV-Ausgabe und Beat-Raster für den Modul-Renderer (Portierung von
//! `it_live`: `write_wav`, `BeatTracker`, `render_variation`).
//!
//! - [`wav_stereo`] schreibt 16-Bit-PCM (stereo) aus zwei Float-Kanälen.
//! - [`BeatTracker`] meldet den Beginn jeder vierten Beat-Phrase (wie der
//!   Live-Trigger im Desktop).
//! - [`order_mit_pattern`] sucht die nächste Order mit einem Ziel-Pattern —
//!   damit springt die Variation nur an Beatgrenzen.

use crate::openmpt::Mod;

/// 16-Bit-PCM-WAV aus zwei Float-Kanälen (Werte -1.0 … 1.0), Vorgabe 44100 Hz
/// wie im Desktop. Aufbau wie `write_wav` in `src/main.rs` (RIFF/WAVE/fmt/data,
/// block align 4, 16 Bit).
pub fn wav_stereo(links: &[f32], rechts: &[f32], rate: u32) -> Vec<u8> {
    let anzahl = links.len().min(rechts.len());
    let daten_bytes = anzahl * 4; // zwei Kanaele * 16 Bit
    let mut out = Vec::with_capacity(44 + daten_bytes);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + daten_bytes as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes()); // stereo
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes()); // byte rate
    out.extend_from_slice(&4u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits je Sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(daten_bytes as u32).to_le_bytes());
    for i in 0..anzahl {
        for wert in [links[i], rechts[i]] {
            let s = (wert.clamp(-1.0, 1.0) * 32767.0) as i16;
            out.extend_from_slice(&s.to_le_bytes());
        }
    }
    out
}

/// Meldet den Beginn jeder vierten Beat-Phrase (`Some(creative)`), sonst
/// `None` — Portierung von `BeatTracker::observe` aus dem Desktop-it_live.
/// `rpb` sind die Rows je Beat des laufenden Patterns.
#[derive(Debug, Default)]
pub struct BeatTracker {
    prev_row: i32,
    beat_count: u32,
    creative: u8,
}

impl BeatTracker {
    pub fn new() -> Self {
        Self {
            prev_row: -1,
            beat_count: 0,
            creative: 0,
        }
    }

    /// Eine Modulzeile beobachten. `Some(creative)` heißt: jetzt beginnt eine
    /// neue 4er-Phrase, der Wert zählt ab 1 hoch.
    pub fn observe(&mut self, row: i32, rpb: i32) -> Option<u8> {
        let rpb = rpb.max(1);
        let ergebnis = if self.prev_row < 0 {
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
        ergebnis
    }
}

/// Nächste Order **nach** `nach` (exklusiv), deren Pattern `ziel` ist —
/// genau wie `render_variation` im Desktop nach dem Ziel sucht.
pub fn order_mit_pattern(order_patterns: &[i32], ziel: i32, nach: i32) -> Option<i32> {
    (nach + 1..order_patterns.len() as i32).find(|o| order_patterns.get(*o as usize) == Some(&ziel))
}

/// Rendert eine taktkompatible Variation als WAV (Portierung von
/// `render_variation`): alle vier Beats wird die Order auf das Pattern
/// `(creative - 1) % 8` umgeschaltet, sofern es später noch vorkommt; die
/// Zeile bleibt erhalten.
pub fn wav_variation(modul: &Mod, sekunden: f64, rate: u32) -> Vec<u8> {
    let rate = if rate == 0 { 44100 } else { rate };
    let chunk = 512usize;
    let gesamt = ((sekunden.max(0.1) * rate as f64) as usize).max(chunk);
    let rpb = {
        let wert = modul.rows_per_beat(modul.current_pattern());
        if wert <= 0 {
            32
        } else {
            wert
        }
    };
    let order_patterns: Vec<i32> = (0..modul.num_orders())
        .map(|o| modul.order_pattern(o))
        .collect();

    let mut links = Vec::with_capacity(gesamt);
    let mut rechts = Vec::with_capacity(gesamt);
    let mut tracker = BeatTracker::new();
    let mut block_l = vec![0.0f32; chunk];
    let mut block_r = vec![0.0f32; chunk];

    while links.len() < gesamt {
        let n = modul.read_stereo(rate as i32, &mut block_l, &mut block_r);
        if n == 0 {
            break;
        }
        links.extend_from_slice(&block_l[..n]);
        rechts.extend_from_slice(&block_r[..n]);

        let cur_row = modul.current_row();
        let cur_order = modul.current_order();
        if let Some(creative) = tracker.observe(cur_row, rpb) {
            let ziel = ((creative as i32 - 1).rem_euclid(8)) as i32;
            if let Some(o) = order_mit_pattern(&order_patterns, ziel, cur_order) {
                modul.set_order(o, cur_row);
            }
        }
    }
    links.truncate(gesamt);
    rechts.truncate(gesamt);
    wav_stereo(&links, &rechts, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn u16_le(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
    }

    fn i16_at(bytes: &[u8], sample: usize) -> i16 {
        i16::from_le_bytes(bytes[44 + sample * 2..46 + sample * 2].try_into().unwrap())
    }

    #[test]
    fn wav_kopf_und_samples_stimmen() {
        let b = wav_stereo(&[0.0, 1.0, -1.0], &[0.0, 0.5, -0.5], 44100);
        assert_eq!(&b[..4], b"RIFF");
        assert_eq!(&b[8..12], b"WAVE");
        assert_eq!(&b[12..16], b"fmt ");
        assert_eq!(u32_le(&b, 16), 16, "fmt-Groesse");
        assert_eq!(u16_le(&b, 20), 1, "PCM");
        assert_eq!(u16_le(&b, 22), 2, "stereo");
        assert_eq!(u32_le(&b, 24), 44100, "Abtastrate");
        assert_eq!(u32_le(&b, 28), 44100 * 4, "Byte-Rate");
        assert_eq!(u16_le(&b, 32), 4, "Block-Align");
        assert_eq!(u16_le(&b, 34), 16, "Bits");
        assert_eq!(&b[36..40], b"data");
        assert_eq!(u32_le(&b, 40) as usize, b.len() - 44, "Datenlaenge");
        assert_eq!(u32_le(&b, 4) as usize, b.len() - 8, "RIFF-Groesse");
        assert_eq!(b.len(), 44 + 3 * 4, "drei Frames stereo");
        // Verschraenkt links/rechts, geklemmt und gerundet.
        assert_eq!(i16_at(&b, 0), 0);
        assert_eq!(i16_at(&b, 1), 0);
        assert_eq!(i16_at(&b, 2), 32767);
        assert_eq!(i16_at(&b, 3), 16383);
        assert_eq!(i16_at(&b, 4), -32767);
        assert_eq!(i16_at(&b, 5), -16383);
    }

    #[test]
    fn wav_klemmt_und_verkraftet_leere_kanaele() {
        let b = wav_stereo(&[2.0], &[-2.0], 8000);
        assert_eq!(i16_at(&b, 0), 32767);
        assert_eq!(i16_at(&b, 1), -32767);
        let leer = wav_stereo(&[], &[], 8000);
        assert_eq!(leer.len(), 44, "nur der Kopf");
        assert_eq!(u32_le(&leer, 40), 0, "leere Daten");
        // Unterschiedlich lange Kanaele: der kuerzere bestimmt.
        let b = wav_stereo(&[0.0, 0.0, 0.0], &[0.0], 8000);
        assert_eq!(b.len(), 44 + 2 * 2);
    }

    #[test]
    fn beat_tracker_meldet_phrasengrenzen() {
        // Wie der Desktop-Test `beat_tracker_returns_phrase_boundaries`.
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(0, 4), None);
        assert_eq!(bt.observe(1, 4), None);
        assert_eq!(bt.observe(4, 4), None);
        assert_eq!(bt.observe(8, 4), None);
        assert_eq!(bt.observe(12, 4), None);
        assert_eq!(bt.observe(16, 4), Some(1), "vier Beats voll");
        assert_eq!(bt.observe(20, 4), None);
        assert_eq!(bt.observe(24, 4), None);
        assert_eq!(bt.observe(28, 4), None);
        assert_eq!(bt.observe(32, 4), Some(2));
    }

    #[test]
    fn beat_tracker_ignoriert_wiederholte_zeilen_und_achtelt_rpb() {
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(3, 4), None, "erste Zeile zaehlt nicht");
        assert_eq!(bt.observe(3, 4), None, "dieselbe Zeile");
        assert_eq!(bt.observe(4, 4), None);
        assert_eq!(bt.observe(8, 4), None);
        assert_eq!(bt.observe(12, 4), None);
        assert_eq!(bt.observe(16, 4), Some(1));
        // Anderes rpb: 0, 8, 16, 24 sind die Beats.
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(0, 8), None);
        assert_eq!(bt.observe(8, 8), None);
        assert_eq!(bt.observe(16, 8), None);
        assert_eq!(bt.observe(24, 8), None);
        assert_eq!(bt.observe(32, 8), Some(1));
        // rpb 0 darf nicht durch Null teilen.
        let mut bt = BeatTracker::new();
        assert_eq!(bt.observe(0, 0), None);
        assert_eq!(bt.observe(1, 0), None);
    }

    #[test]
    fn naechste_order_mit_zielpattern() {
        let an = [0, 1, 2, 1, 3, 2];
        assert_eq!(order_mit_pattern(&an, 2, 0), Some(2), "erste 2 nach 0");
        assert_eq!(order_mit_pattern(&an, 2, 2), Some(5), "danach die naechste");
        assert_eq!(order_mit_pattern(&an, 3, 3), Some(4));
        assert_eq!(order_mit_pattern(&an, 7, 0), None, "kommt nicht vor");
        assert_eq!(order_mit_pattern(&[], 0, -1), None, "leer");
        assert_eq!(order_mit_pattern(&an, 0, -1), Some(0), "auch Order 0");
    }
}
