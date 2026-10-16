//! Lissajous-Figur mit Phasen-Drehung im Takt.
//!
//! Quelle ist der Max-Patch `lissa-2020optic.amxd` (USB-Stick, Ordner `max/`):
//!
//! ```text
//! expr (tan($f1*π/$f2))                     87 und F (Blockrate 44100/512)
//! expr ($f1*$f2/$f3)                        → F/104
//! expr ((atan($f1)+$f3*π)*$f2/π)            $f3 = Transport (Takt)
//! ```
//!
//! Die Kette ist eine **Sägezahn-Phase**: der Phasenversatz einer der beiden
//! Schwingungen läuft linear und springt am Zyklusende zurück. Die Figur dreht
//! sich dadurch nicht starr um den Mittelpunkt, sondern „kippt" wie eine auf
//! der x,y-Ebene stehende Figur. Hier wird dieselbe Bewegung refresh-unabhängig
//! gerechnet: `φ = 2π · Umdrehungen · f(Takte / Takte_pro_Zyklus)`.

use std::f64::consts::PI;

/// Kleinster Wert des Umdrehungs-Faders (1/64).
pub const UMDREHUNGEN_MIN: f64 = 1.0 / 64.0;
/// Größter Wert des Umdrehungs-Faders.
pub const UMDREHUNGEN_MAX: f64 = 16.0;
/// Vorgabe, wenn der Wert unbrauchbar ist.
pub const UMDREHUNGEN_VORGABE: f64 = 1.0;

/// Charakter der Phasen-Drehung.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modus {
    /// Wie das Original: linear hoch, harter Rücksprung („Doppel"-Moment).
    Saege,
    /// Verbessert: weiche Umkehr ohne Sprung.
    Weich,
    /// Endlos in eine Richtung (Phase wächst ohne Umkehr) — links/rechts.
    UnendlichLinks,
    UnendlichRechts,
}

impl Modus {
    /// Richtung der endlosen Drehung als Vorzeichen (0 = keine endlose).
    pub fn endlos_vorzeichen(self) -> f64 {
        match self {
            Modus::UnendlichLinks => -1.0,
            Modus::UnendlichRechts => 1.0,
            _ => 0.0,
        }
    }
}

/// Sägezahn wie `atan(tan(πx))·2/π` aus dem Original: Periode 1, Werte −1…1,
/// steigt 0→1 auf [0; ½), springt auf −1 und steigt wieder auf 0.
/// `saege(0,5)` ist der Wert **nach** dem Sprung (−1).
pub fn saege(x: f64) -> f64 {
    let f = x - x.floor();
    if f < 0.5 {
        2.0 * f
    } else {
        2.0 * f - 2.0
    }
}

/// Weiche Umkehr: 0 am Anfang, 1 in der Mitte, 0 am Ende (Periode 1).
pub fn weich(x: f64) -> f64 {
    (1.0 - (2.0 * PI * x).cos()) / 2.0
}

/// Phasenversatz in Radiant nach `takte` Takten: eine volle Umdrehung
/// entspricht `2π·Umdrehungen` über einen Zyklus von `takte_pro_zyklus` Takten.
pub fn phase(takte: f64, takte_pro_zyklus: f64, umdrehungen: f64, modus: Modus) -> f64 {
    let zyklus = if takte_pro_zyklus.is_finite() && takte_pro_zyklus > 0.0 {
        takte_pro_zyklus
    } else {
        1.0
    };
    let u = klemmen_umdrehungen(umdrehungen);
    let x = takte / zyklus;
    let f = match modus {
        Modus::Saege => saege(x),
        Modus::Weich => weich(x),
        // Endlos: die Phase wächst ohne Umkehr (Vorzeichen = Richtung).
        Modus::UnendlichLinks | Modus::UnendlichRechts => {
            modus.endlos_vorzeichen() * x
        }
    };
    2.0 * PI * u * f
}

/// Winkelgeschwindigkeit der Phase in rad/s — der Ton der X-Stimme läuft mit
/// diesem Versatz (wie im Max-Patch, wo die Drehung ein Frequenzversatz ist).
pub fn phasen_geschwindigkeit(
    takte: f64,
    takte_pro_zyklus: f64,
    umdrehungen: f64,
    modus: Modus,
    bpm: f64,
) -> f64 {
    let zyklus = if takte_pro_zyklus.is_finite() && takte_pro_zyklus > 0.0 {
        takte_pro_zyklus
    } else {
        1.0
    };
    let u = klemmen_umdrehungen(umdrehungen);
    let takte_pro_s = if bpm.is_finite() { bpm.clamp(0.0, 1000.0) / 60.0 } else { 0.0 };
    let x = takte / zyklus;
    let steigung = match modus {
        // saege: Steigung +2 überall (der Sprung ändert die Steigung nicht).
        Modus::Saege => 2.0,
        // weich: (1-cos)/2 hat die Steigung π·sin(2πx).
        Modus::Weich => PI * (2.0 * PI * x).sin(),
        Modus::UnendlichLinks | Modus::UnendlichRechts => modus.endlos_vorzeichen(),
    };
    2.0 * PI * u * steigung * takte_pro_s / zyklus
}

/// Frequenzen der beiden Stimmen: X = Grundton·x, Y = Grundton·y (das
/// Verhältnis der Figur). Beide werden auf hörbare, ungefährliche Werte
/// geklemmt (20 Hz … 20 kHz).
pub fn stimmen_frequenzen(grundton_hz: f32, a: u32, b: u32) -> [f32; 2] {
    let grund = if grundton_hz.is_finite() { grundton_hz } else { 220.0 };
    let a = a.max(1) as f32;
    let b = b.max(1) as f32;
    [
        (grund * a).clamp(20.0, 20_000.0),
        (grund * b).clamp(20.0, 20_000.0),
    ]
}

/// Zwei Dauertöne (x/y) auf einen Block **addieren** — die Phasen laufen über
/// Blockgrenzen weiter. `freqs[i] == 0` lässt die Stimme still.
pub fn dauerton_mischen(
    puffer: &mut [i16],
    phasen: &mut [f64; 2],
    freqs: [f32; 2],
    rate: u32,
    gain: f32,
) {
    if rate == 0 {
        return;
    }
    let schritt = [
        freqs[0] as f64 / rate as f64,
        freqs[1] as f64 / rate as f64,
    ];
    let g = if gain.is_finite() { gain.clamp(0.0, 1.0) as f64 } else { 0.0 };
    for wert in puffer.iter_mut() {
        let w = (phasen[0] * std::f64::consts::TAU).sin()
            + (phasen[1] * std::f64::consts::TAU).sin();
        let add = (w * g * 32767.0) as i32;
        let neu = (*wert as i32 + add).clamp(-32768, 32767);
        *wert = neu as i16;
        phasen[0] = (phasen[0] + schritt[0]).fract();
        phasen[1] = (phasen[1] + schritt[1]).fract();
    }
}

/// Figurenpunkte: `x = sin(a·θ + φ)`, `y = sin(b·θ)`, θ über den vollen Umlauf.
pub fn punkte(a: f64, b: f64, phi: f64, n: usize) -> Vec<(f64, f64)> {
    let n = n.max(2);
    (0..n)
        .map(|i| {
            let th = 2.0 * PI * i as f64 / n as f64;
            ((a * th + phi).sin(), (b * th).sin())
        })
        .collect()
}

/// Umdrehungen klemmen (1/64 … 16); unbrauchbare Werte werden zur Vorgabe.
pub fn klemmen_umdrehungen(u: f64) -> f64 {
    if u.is_nan() {
        return UMDREHUNGEN_VORGABE;
    }
    u.clamp(UMDREHUNGEN_MIN, UMDREHUNGEN_MAX)
}

/// Durch `n` teilen (Faderknopf „÷2"/„÷3"), geklemmt.
pub fn teile_umdrehungen(u: f64, n: f64) -> f64 {
    klemmen_umdrehungen(u / n.max(1.0))
}

/// Mit `n` malnehmen (Faderknopf „×2"/„×3"), geklemmt.
pub fn vielfache_umdrehungen(u: f64, n: f64) -> f64 {
    klemmen_umdrehungen(u * n.max(1.0))
}

/// Anzeige für den Fader: „1 Umdrehung", „3 Umdrehungen", „1/2 Umdrehung",
/// „3/4 Umdrehungen".
pub fn text_umdrehungen(u: f64) -> String {
    let u = klemmen_umdrehungen(u);
    if u >= 1.0 {
        let gerundet = u.round();
        if (u - gerundet).abs() < 1e-6 {
            return if gerundet == 1.0 {
                "1 Umdrehung".to_string()
            } else {
                format!("{gerundet:.0} Umdrehungen")
            };
        }
    }
    for nenner in 1..=64usize {
        let zaehler = u * nenner as f64;
        if (zaehler - zaehler.round()).abs() < 1e-6 && zaehler >= 1.0 {
            return if zaehler.round() == 1.0 {
                format!("{zaehler:.0}/{nenner} Umdrehung")
            } else {
                format!("{zaehler:.0}/{nenner} Umdrehungen")
            };
        }
    }
    format!("{u:.2} Umdrehungen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saege_wie_atan_tan_aus_dem_original() {
        // Dieselbe Kurve wie `atan(tan(πx))·2/π` — an der Sprungstelle 0,5
        // (tan-Pol) wird nicht verglichen.
        let mut x = 0.0f64;
        while x < 1.0 {
            if (x - 0.5).abs() > 1e-6 {
                let original = (PI * x).tan().atan() * 2.0 / PI;
                assert!(
                    (saege(x) - original).abs() < 1e-9,
                    "x={x}: saege={} original={original}",
                    saege(x)
                );
            }
            x += 0.0125;
        }
        assert_eq!(saege(0.0), 0.0, "Start bei null");
        assert_eq!(saege(0.25), 0.5, "Viertel: halbe Höhe");
        assert_eq!(saege(0.75), -0.5, "nach dem Sprung auf halber Höhe");
        assert_eq!(saege(0.5), -1.0, "Sprungstelle: Wert nach dem Sprung");
        assert_eq!(saege(1.0), 0.0, "Periode geschlossen");
        assert!((saege(2.25) - saege(0.25)).abs() < 1e-12, "Periode 1");
        let mut x = 0.0;
        while x < 3.0 {
            assert!(saege(x) >= -1.0 && saege(x) <= 1.0, "Werte in −1…1");
            x += 0.01;
        }
    }

    #[test]
    fn weich_hat_maximum_in_der_mitte() {
        assert!(weich(0.0).abs() < 1e-12, "Start bei null");
        assert!((weich(0.25) - 0.5).abs() < 1e-12, "Viertel: halbe Höhe");
        assert!((weich(0.5) - 1.0).abs() < 1e-12, "Mitte: voll");
        assert!(weich(1.0).abs() < 1e-12, "Periode geschlossen");
        assert!((weich(2.5) - weich(0.5)).abs() < 1e-12, "Periode 1");
    }

    #[test]
    fn phase_beginnt_bei_null_und_spannt_die_umdrehungen() {
        // Weich: Maximum in der Zyklusmitte, zurück am Zyklusende.
        assert!(phase(0.0, 2.0, 1.0, Modus::Weich).abs() < 1e-12);
        assert!((phase(1.0, 2.0, 1.0, Modus::Weich) - 2.0 * PI).abs() < 1e-12);
        assert!(phase(2.0, 2.0, 1.0, Modus::Weich).abs() < 1e-12);
        // Sägezahn: linear bis kurz vor die halbe Periode, dann Sprung auf −1.
        let kurz_vor = phase(0.999, 2.0, 1.0, Modus::Saege);
        assert!(
            (kurz_vor - 2.0 * PI * 0.999).abs() < 1e-9,
            "linearer Lauf vor dem Sprung: {kurz_vor}"
        );
        assert!(
            (phase(1.0, 2.0, 1.0, Modus::Saege) + 2.0 * PI).abs() < 1e-12,
            "Sprungstelle liegt auf −1"
        );
        assert!(phase(2.0, 2.0, 1.0, Modus::Saege).abs() < 1e-12);
        // Umdrehungen skalieren linear.
        assert!((phase(1.0, 2.0, 3.0, Modus::Weich) - 6.0 * PI).abs() < 1e-12);
        // Ein kürzerer Zyklus dreht schneller.
        assert!(
            phase(0.5, 1.0, 1.0, Modus::Weich) > phase(0.5, 2.0, 1.0, Modus::Weich),
            "halber Zyklus ist weiter gedreht"
        );
    }

    #[test]
    fn punkte_kreis_bei_gleichem_verhaeltnis() {
        // a = b und φ = π/2: x = cos(2θ), y = sin(2θ) → Kreis mit r = 1.
        let p = punkte(2.0, 2.0, PI / 2.0, 64);
        assert_eq!(p.len(), 64, "Anzahl der Punkte");
        for (x, y) in &p {
            let r = (x * x + y * y).sqrt();
            assert!((r - 1.0).abs() < 1e-9, "Punkt auf dem Kreis: r={r}");
        }
    }

    #[test]
    fn punkte_bleiben_im_einheitsquadrat() {
        for (x, y) in punkte(3.0, 5.0, 0.7, 128) {
            assert!(x.abs() <= 1.0 && y.abs() <= 1.0, "|x|,|y| ≤ 1");
        }
    }

    #[test]
    fn umdrehungen_werden_geklemmt() {
        assert_eq!(klemmen_umdrehungen(0.0), UMDREHUNGEN_MIN);
        assert_eq!(klemmen_umdrehungen(100.0), UMDREHUNGEN_MAX);
        assert_eq!(klemmen_umdrehungen(f64::NAN), UMDREHUNGEN_VORGABE);
        assert_eq!(teile_umdrehungen(1.0, 2.0), 0.5);
        assert_eq!(vielfache_umdrehungen(0.25, 3.0), 0.75);
        assert_eq!(teile_umdrehungen(UMDREHUNGEN_MIN, 4.0), UMDREHUNGEN_MIN);
        assert_eq!(text_umdrehungen(1.0), "1 Umdrehung");
        assert_eq!(text_umdrehungen(3.0), "3 Umdrehungen");
        assert_eq!(text_umdrehungen(0.5), "1/2 Umdrehung");
        assert_eq!(text_umdrehungen(0.25), "1/4 Umdrehung");
        assert_eq!(text_umdrehungen(1.0 / 64.0), "1/64 Umdrehung");
    }

    #[test]
    fn unendlich_dreht_ohne_umkehr() {
        // Monoton in beide Richtungen, keine Umkehr.
        let mut letzter = f64::NEG_INFINITY;
        let mut t = 0.0;
        while t < 6.0 {
            let p = phase(t, 2.0, 1.0, Modus::UnendlichRechts);
            assert!(p > letzter, "rechts wächst monoton: {p} > {letzter}");
            letzter = p;
            t += 0.25;
        }
        // Nach einem Zyklus ist genau eine Umdrehung mehr gelaufen.
        assert!((phase(2.0, 2.0, 1.0, Modus::UnendlichRechts) - 2.0 * PI).abs() < 1e-12);
        assert!((phase(2.0, 2.0, 1.0, Modus::UnendlichLinks) + 2.0 * PI).abs() < 1e-12);
        // Links und rechts sind Spiegelbilder.
        assert!(
            (phase(1.5, 2.0, 1.0, Modus::UnendlichLinks)
                + phase(1.5, 2.0, 1.0, Modus::UnendlichRechts))
            .abs()
                < 1e-12
        );
        // Umdrehungen skalieren den endlosen Lauf.
        assert!((phase(2.0, 2.0, 4.0, Modus::UnendlichRechts) - 8.0 * PI).abs() < 1e-12);
    }

    #[test]
    fn geschwindigkeit_stimmt_mit_der_ableitung() {
        let bpm = 128.0;
        for modus in [
            Modus::Saege,
            Modus::Weich,
            Modus::UnendlichLinks,
            Modus::UnendlichRechts,
        ] {
            let mut t = 0.0f64;
            let takte_von = |t: f64| t * bpm / 60.0;
            while t < 4.0 {
                // Sprungstelle der Sägezahn (x = 0,5) auslassen.
                let x = takte_von(t) / 2.0;
                let frac = x - x.floor();
                if (frac - 0.5).abs() > 0.02 {
                    let dt = 1e-5;
                    let numerisch = (phase(takte_von(t + dt), 2.0, 1.0, modus)
                        - phase(takte_von(t - dt), 2.0, 1.0, modus))
                        / (2.0 * dt);
                    let formel = phasen_geschwindigkeit(takte_von(t), 2.0, 1.0, modus, bpm);
                    assert!(
                        (numerisch - formel).abs() < 1e-3,
                        "{modus:?} bei t={t}: numerisch {numerisch} vs Formel {formel}"
                    );
                }
                t += 0.05;
            }
        }
        // Endlos: 2 Takte je Zyklus, 1 Umdrehung, 120 BPM → 1 Umdrehung je
        // Sekunde = 2π rad/s (als Ton ein Versatz von 1 Hz).
        let w = phasen_geschwindigkeit(0.0, 2.0, 1.0, Modus::UnendlichRechts, 120.0);
        assert!((w - 2.0 * PI).abs() < 1e-12, "1 Umdrehung je Sekunde: {w}");
        // Sägezahn läuft immer vorwärts (auch nach dem Sprung).
        let w = phasen_geschwindigkeit(1.9, 2.0, 1.0, Modus::Saege, 120.0);
        assert!(w > 0.0, "Sägezahn: {w}");
        // Weich kehrt die Richtung in der zweiten Hälfte um.
        let w = phasen_geschwindigkeit(1.5, 2.0, 1.0, Modus::Weich, 120.0);
        assert!(w < 0.0, "weich nach der Mitte: {w}");
    }

    #[test]
    fn stimmen_frequenzen_folgen_dem_verhaeltnis() {
        let f = stimmen_frequenzen(220.0, 2, 3);
        assert!((f[0] - 440.0).abs() < 1e-3 && (f[1] - 660.0).abs() < 1e-3, "{f:?}");
        // Klemmen nach oben und unten.
        let f = stimmen_frequenzen(1000.0, 16, 1);
        assert!(f[0] <= 20_000.0 && f[1] >= 20.0, "{f:?}");
        // Unbrauchbarer Grundton → Vorgabe 220 Hz.
        let f = stimmen_frequenzen(f32::NAN, 1, 1);
        assert!((f[0] - 220.0).abs() < 1e-3, "{f:?}");
    }

    #[test]
    fn dauerton_laeuft_und_bleibt_still_bei_null() {
        let rate = 44_100;
        let mut phasen = [0.0f64; 2];
        let mut puffer = vec![0i16; 441];
        dauerton_mischen(&mut puffer, &mut phasen, [440.0, 660.0], rate, 0.3);
        assert!(puffer.iter().any(|v| *v != 0), "es klingt");
        // Phase X ist um 440 · 441/44100 Perioden weitergelaufen.
        let erwartet = (440.0 * 441.0 / 44_100.0) % 1.0;
        assert!(
            (phasen[0] - erwartet).abs() < 1e-9,
            "Phase X: {} vs {erwartet}",
            phasen[0]
        );
        // Nullfrequenzen lassen den Puffer unberührt.
        let mut still = vec![123i16; 64];
        let mut ph = [0.0f64; 2];
        dauerton_mischen(&mut still, &mut ph, [0.0, 0.0], rate, 0.3);
        assert!(still.iter().all(|v| *v == 123), "still bleibt still");
        // Additiv: vorhandener Inhalt bleibt erhalten.
        let mut belegt = vec![1000i16; 64];
        let mut ph2 = [0.0f64; 2];
        dauerton_mischen(&mut belegt, &mut ph2, [440.0, 0.0], rate, 0.1);
        assert!(belegt.iter().all(|v| *v != 0), "additiv");
    }
}
