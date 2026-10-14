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
/// Welche Toene aus einer Warteschlange ueberhaupt gespielt werden.
///
/// Der Bild-Scan liefert mehr Noten, als der Tongenerator in der Zeit
/// abspielen kann (ein Ton dauert `LAENGE_MS`, die Spalte ist viel kuerzer).
/// Ohne Begrenzung wächst die Warteschlange unbegrenzt: der Klang hoert nie
/// auf und man lauscht auf einen alten Rueckstand statt auf das Bild.
///
/// Deshalb bleiben nur die **`max` neuesten** Toene. Die aeltesten fallen weg —
/// genau die, die man ohnehin nicht mehr hoeren will.
pub fn neueste_woertlich<T>(schlange: &[T], max: usize) -> Vec<&T> {
    if max == 0 || schlange.is_empty() {
        return Vec::new();
    }
    let start = schlange.len().saturating_sub(max);
    schlange[start..].iter().collect()
}

/// Tonlaenge als Notenwert (Nutzerentscheidung 2026-10-14: 1/64 bis 4 Takte,
/// die Millisekunden zeigt die App an).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notenwert {
    Vierundsechzigstel,
    Zweiunddreissigstel,
    Sechzehntel,
    Achtel,
    Viertel,
    Halbe,
    Ganze,
    ZweiTakte,
    VierTakte,
}

impl Notenwert {
    pub const ALLE: [Notenwert; 9] = [
        Notenwert::Vierundsechzigstel,
        Notenwert::Zweiunddreissigstel,
        Notenwert::Sechzehntel,
        Notenwert::Achtel,
        Notenwert::Viertel,
        Notenwert::Halbe,
        Notenwert::Ganze,
        Notenwert::ZweiTakte,
        Notenwert::VierTakte,
    ];

    /// Dauer in Millisekunden bei `bpm` (4/4: ein Takt sind vier Viertel).
    pub fn ms(self, bpm: f32) -> u32 {
        let bpm = bpm.clamp(20.0, 300.0);
        let viertel_ms = 60_000.0 / bpm;
        let faktor = match self {
            Notenwert::Vierundsechzigstel => 1.0 / 16.0,
            Notenwert::Zweiunddreissigstel => 1.0 / 8.0,
            Notenwert::Sechzehntel => 1.0 / 4.0,
            Notenwert::Achtel => 0.5,
            Notenwert::Viertel => 1.0,
            Notenwert::Halbe => 2.0,
            Notenwert::Ganze => 4.0,
            Notenwert::ZweiTakte => 8.0,
            Notenwert::VierTakte => 16.0,
        };
        (viertel_ms * faktor).round() as u32
    }

    pub fn name(self) -> &'static str {
        match self {
            Notenwert::Vierundsechzigstel => "1/64",
            Notenwert::Zweiunddreissigstel => "1/32",
            Notenwert::Sechzehntel => "1/16",
            Notenwert::Achtel => "1/8",
            Notenwert::Viertel => "1/4",
            Notenwert::Halbe => "1/2",
            Notenwert::Ganze => "1",
            Notenwert::ZweiTakte => "2 Takte",
            Notenwert::VierTakte => "4 Takte",
        }
    }
}

/// Eine klingende Stimme des Tongenerators: Frequenz, Position und Laenge in
/// Samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stimme {
    pub freq: f32,
    pub position: u64,
    pub laenge: u64,
}

impl Stimme {
    /// Stimme fuer eine Frequenz und Laenge in Millisekunden.
    pub fn neu(freq: f32, rate: u32, laenge_ms: u32) -> Self {
        let laenge = (rate as u64 * laenge_ms as u64 / 1000).max(1);
        Self {
            freq,
            position: 0,
            laenge,
        }
    }
}

/// Wert der Wellenform bei Phase 0..1; Rauschen ist ueber die Sample-Position
/// deterministisch (kein Zustand noetig).
pub fn wellen_wert(welle: Welle, phase: f32, position: u64) -> f32 {
    match welle {
        Welle::Sinus => (phase * std::f32::consts::TAU).sin(),
        Welle::Dreieck => 4.0 * (phase - 0.5).abs() - 1.0,
        Welle::Saegezahn => 2.0 * phase - 1.0,
        Welle::Rechteck => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Welle::Rauschen => {
            let mut x = position.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            x ^= x >> 33;
            x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
            x ^= x >> 33;
            (x as f32 / u64::MAX as f32) * 2.0 - 1.0
        }
    }
}

/// Einen Block aus allen klingenden Stimmen mischen: die Wellen addieren sich
/// (mit `gain` als Kopfraum), jede Stimme klingt linear aus, das Ergebnis wird
/// auf i16 geklemmt. Danach sind alle Stimmen einen Block weitergerueckt;
/// fertige Stimmen fallen aus der Liste (der Aufrufer entscheidet, wann eine
/// neue hinzukommt und welche bei Ueberlast weicht).
pub fn mischen(puffer: &mut [i16], stimmen: &mut Vec<Stimme>, rate: u32, welle: Welle, gain: f32) {
    let mut roh = vec![0.0f32; puffer.len()];
    for stimme in stimmen.iter_mut() {
        for (i, wert) in roh.iter_mut().enumerate() {
            let pos = stimme.position + i as u64;
            if pos >= stimme.laenge {
                break;
            }
            let phase = (stimme.freq * pos as f32 / rate as f32).fract();
            let huelle = 1.0 - pos as f32 / stimme.laenge as f32;
            *wert += wellen_wert(welle, phase, pos) * huelle * gain;
        }
        stimme.position += puffer.len() as u64;
    }
    stimmen.retain(|s| s.position < s.laenge);
    for (i, wert) in roh.iter().enumerate() {
        puffer[i] = (wert.clamp(-1.0, 1.0) * 32767.0) as i16;
    }
}

#[cfg(test)]
mod tests_mixer {
    use super::*;

    #[test]
    fn notenwert_in_millisekunden() {
        // 120 BPM: eine Viertel ist 500 ms.
        assert_eq!(Notenwert::Viertel.ms(120.0), 500);
        assert_eq!(Notenwert::Vierundsechzigstel.ms(120.0), 31);
        assert_eq!(Notenwert::Ganze.ms(120.0), 2000);
        assert_eq!(Notenwert::VierTakte.ms(120.0), 8000, "4 Takte = 16 Viertel");
        // 60 BPM: alles doppelt so lang.
        assert_eq!(Notenwert::Viertel.ms(60.0), 1000);
    }

    #[test]
    fn eine_stimme_klingt_und_laeuft_aus() {
        let rate = 44_100;
        // 5 ms sind 220 Samples: nach zwei kurzen Bloecken ist die Stimme durch.
        let mut stimmen = vec![Stimme::neu(440.0, rate, 5)];
        let mut puffer = [0i16; 64];
        mischen(&mut puffer, &mut stimmen, rate, Welle::Sinus, 0.5);
        assert!(puffer.iter().any(|v| *v != 0), "es klingt");
        assert_eq!(stimmen.len(), 1, "noch nicht fertig");
        let mut rest = [0i16; 512];
        mischen(&mut rest, &mut stimmen, rate, Welle::Sinus, 0.5);
        assert!(stimmen.is_empty(), "nach der Laenge ist die Stimme weg");
    }

    #[test]
    fn zwei_stimmen_addieren_sich() {
        let rate = 44_100;
        let mut einzeln = vec![Stimme::neu(440.0, rate, 500)];
        let mut doppelt = vec![Stimme::neu(440.0, rate, 500), Stimme::neu(440.0, rate, 500)];
        let (mut a, mut b) = ([0i16; 256], [0i16; 256]);
        mischen(&mut a, &mut einzeln, rate, Welle::Sinus, 0.25);
        mischen(&mut b, &mut doppelt, rate, Welle::Sinus, 0.25);
        let spitze = |x: &[i16]| x.iter().map(|v| v.abs()).max().unwrap_or(0);
        assert!(
            spitze(&b) > spitze(&a),
            "zwei Stimmen sind lauter: {} vs {}",
            spitze(&b),
            spitze(&a)
        );
    }

    #[test]
    fn uebersteuerung_klemmt_statt_zu_ueberlaufen() {
        let rate = 44_100;
        let mut stimmen: Vec<Stimme> = (0..64).map(|_| Stimme::neu(440.0, rate, 500)).collect();
        let mut puffer = [0i16; 128];
        mischen(&mut puffer, &mut stimmen, rate, Welle::Sinus, 1.0);
        assert!(puffer.iter().all(|v| *v != i16::MIN), "kein Ueberlauf");
        assert!(
            puffer.iter().any(|v| *v == i16::MAX || *v == -i16::MAX),
            "viel Pegel klemmt am Anschlag"
        );
    }
}

#[cfg(test)]
mod tests_warteschlange {
    use super::neueste_woertlich;

    #[test]
    fn nur_die_neuesten_bleiben() {
        let schlange: Vec<i32> = (0..100).collect();
        let raus = neueste_woertlich(&schlange, 8);
        assert_eq!(raus.len(), 8);
        assert_eq!(*raus.last().unwrap(), &99, "die neueste Note bleibt");
        assert_eq!(*raus.first().unwrap(), &92);
    }

    #[test]
    fn kurze_schlange_bleibt_vollstaendig() {
        let schlange: Vec<i32> = (0..3).collect();
        assert_eq!(neueste_woertlich(&schlange, 8).len(), 3);
    }

    #[test]
    fn leere_schlange_und_null() {
        let leer: Vec<i32> = Vec::new();
        assert!(neueste_woertlich(&leer, 8).is_empty());
        let schlange: Vec<i32> = (0..5).collect();
        assert!(
            neueste_woertlich(&schlange, 0).is_empty(),
            "max = 0 spielt nichts"
        );
    }

    #[test]
    fn die_grenze_haelt_und_begrenzt() {
        // So viele Noten, wie der Bild-Scan liefert: die Warteschlange darf
        // nicht groesser werden als das Fenster.
        let schlange: Vec<i32> = (0..10_000).collect();
        let raus = neueste_woertlich(&schlange, 16);
        assert!(
            raus.len() <= 16,
            "die Schlange bleibt begrenzt: {}",
            raus.len()
        );
    }
}

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
