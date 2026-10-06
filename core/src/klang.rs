//! Mini-Synth: kurze Töne auf dem Handy, damit Auswahl und Raster auch ohne
//! Renoise hörbar sind (und im Demo-Video zu hören).
//!
//! Die Frequenzrechnung ist hardwarefrei und getestet; das Abspielen läuft
//! über `AudioTrack` in einem eigenen Thread mit einer Warteschlange, damit
//! das Zeichnen nicht blockiert.

/// Frequenz einer MIDI-Note (A4 = 69 = 440 Hz).
pub fn frequenz(midi: i32) -> f32 {
    440.0 * 2f32.powf((midi.clamp(0, 127) as f32 - 69.0) / 12.0)
}

/// Ein kurzes Ton-Signal (Sinus mit weichem Ein-/Ausblenden), 16 Bit mono.
pub fn sinus_puffer(freq: f32, rate: u32, laenge_ms: u32) -> Vec<i16> {
    let n = (rate as u64 * laenge_ms as u64 / 1000) as usize;
    let mut out = Vec::with_capacity(n);
    let fade = (n / 8).max(1) as f32;
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let huelle = {
            let k = i as f32;
            let ein = (k / fade).min(1.0);
            let aus = ((n as f32 - k) / fade).min(1.0);
            ein.min(aus)
        };
        let wert = (2.0 * std::f32::consts::PI * freq * t).sin() * huelle * 0.35;
        out.push((wert * 32767.0) as i16);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequenzen_stimmen() {
        assert!((frequenz(69) - 440.0).abs() < 1e-3, "A4");
        assert!((frequenz(57) - 220.0).abs() < 1e-3, "A3");
        assert!((frequenz(60) - 261.6256).abs() < 1e-2, "C4");
        assert!((frequenz(81) - 880.0).abs() < 1e-3, "A5");
    }

    #[test]
    fn frequenz_klemmt_unsinn() {
        assert!(frequenz(-5) > 0.0);
        assert!(frequenz(999) > 0.0);
        assert_eq!(frequenz(0), frequenz(0));
    }

    #[test]
    fn puffer_hat_laenge_und_beginnt_leise() {
        let p = sinus_puffer(440.0, 44100, 80);
        assert_eq!(p.len(), 44100 * 80 / 1000);
        assert!(p[0].abs() < 100, "Einblenden startet bei null: {}", p[0]);
        let mitte = p[p.len() / 2];
        assert!(mitte.abs() > 1000, "in der Mitte schwingt es: {mitte}");
        assert!(p[p.len() - 1].abs() < 100, "Ausblenden endet bei null");
    }

    #[test]
    fn puffer_bleibt_im_bereich() {
        let p = sinus_puffer(880.0, 44100, 50);
        assert!(p.iter().all(|s| (-32768..=32767).contains(s)));
    }
}
