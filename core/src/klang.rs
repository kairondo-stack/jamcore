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

/// Gängige Wellenformen des Tongenerators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Welle {
    Sinus,
    Dreieck,
    Saegezahn,
    Rechteck,
    Rauschen,
}

impl Welle {
    pub const ALLE: [Welle; 5] = [
        Welle::Sinus,
        Welle::Dreieck,
        Welle::Saegezahn,
        Welle::Rechteck,
        Welle::Rauschen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Welle::Sinus => "Sinus",
            Welle::Dreieck => "Dreieck",
            Welle::Saegezahn => "Sägezahn",
            Welle::Rechteck => "Rechteck",
            Welle::Rauschen => "Rauschen",
        }
    }
}

/// Ein kurzes Ton-Signal mit gewählter Wellenform, 16 Bit mono. Ein-/Ausblenden
/// wie bei [`sinus_puffer`]; Rauschen bekommt einen festen Zufallsstrom
/// (xorshift, damit die Tests reproduzierbar sind).
pub fn puffer_welle(freq: f32, rate: u32, laenge_ms: u32, welle: Welle) -> Vec<i16> {
    if welle == Welle::Sinus {
        return sinus_puffer(freq, rate, laenge_ms);
    }
    let n = (rate as u64 * laenge_ms as u64 / 1000) as usize;
    let mut out = Vec::with_capacity(n);
    let fade = (n / 8).max(1) as f32;
    let periode = (rate as f32 / freq.max(1.0)).max(2.0);
    let mut zufall: u32 = 0x1234_5678;
    for i in 0..n {
        let k = i as f32;
        let huelle = (k / fade).min(1.0).min(((n as f32 - k) / fade).min(1.0));
        let phase = (i as f32 % periode) / periode; // 0.0 … 1.0
        let roh = match welle {
            Welle::Dreieck => 4.0 * (phase - 0.5).abs() - 1.0, // -1 … 1
            Welle::Saegezahn => 2.0 * phase - 1.0,             // -1 … 1 steigend
            Welle::Rechteck => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Welle::Rauschen => {
                zufall ^= zufall << 13;
                zufall ^= zufall >> 17;
                zufall ^= zufall << 5;
                (zufall as f32 / u32::MAX as f32) * 2.0 - 1.0
            }
            Welle::Sinus => unreachable!(),
        };
        out.push((roh * huelle * 0.35 * 32767.0) as i16);
    }
    out
}

#[cfg(test)]
mod tests_welle {
    use super::*;

    fn spitzen(p: &[i16]) -> i16 {
        p.iter().map(|s| s.abs()).max().unwrap_or(0)
    }

    #[test]
    fn sinus_entspricht_dem_bisherigen_puffer() {
        assert_eq!(
            puffer_welle(440.0, 44100, 30, Welle::Sinus),
            sinus_puffer(440.0, 44100, 30)
        );
    }

    #[test]
    fn laenge_und_bereich_stimmen() {
        for welle in Welle::ALLE {
            let p = puffer_welle(440.0, 44100, 20, welle);
            assert_eq!(p.len(), 44100 * 20 / 1000, "{}", welle.name());
            assert!(
                p.iter().all(|s| (-32768..=32767).contains(s)),
                "{}",
                welle.name()
            );
            assert!(spitzen(&p) > 1000, "{} klingt", welle.name());
        }
    }

    #[test]
    fn rechteck_springt_nur_zwischen_den_amplituden() {
        let p = puffer_welle(100.0, 8000, 40, Welle::Rechteck);
        let voll = (0.35 * 32767.0) as i16;
        // In der Mitte (nach dem Einblenden) gibt es nur ± voll.
        let mitte = &p[p.len() / 4..p.len() * 3 / 4];
        assert!(
            mitte.iter().all(|s| (s.abs() - voll).abs() <= 2),
            "Rechteck hat nur zwei Werte"
        );
    }

    #[test]
    fn saegezahn_steigt_je_periode() {
        let rate = 8000u32;
        let freq = 100.0;
        let p = puffer_welle(freq, rate, 40, Welle::Saegezahn);
        let periode = (rate as f32 / freq) as usize;
        let a = &p[periode / 2..periode / 2 + 10];
        let b = &p[periode / 2 + 10..periode / 2 + 20];
        let mittel = |x: &[i16]| x.iter().map(|v| *v as i32).sum::<i32>() as f32 / x.len() as f32;
        assert!(mittel(b) > mittel(a), "Sägezahn steigt");
    }

    #[test]
    fn dreieck_ist_symmetrisch_und_rauschen_nicht_konstant() {
        let d = puffer_welle(100.0, 8000, 40, Welle::Dreieck);
        let spitze = d.iter().map(|s| *s as i32).max().unwrap();
        let tiefe = d.iter().map(|s| *s as i32).min().unwrap();
        assert!((spitze + tiefe).abs() < 50, "Dreieck ist mittig");
        let r = puffer_welle(100.0, 8000, 40, Welle::Rauschen);
        let erste: std::collections::HashSet<i16> = r.iter().copied().collect();
        assert!(erste.len() > 10, "Rauschen ist nicht konstant");
    }
}
