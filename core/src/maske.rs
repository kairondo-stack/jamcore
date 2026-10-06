//! Takt-Maske: Schrittweiten und Verteilungen wie die Recorder-Tastenmaske.
//!
//! Beim Desktop-Recorder bestimmen die Tastendrücke und die Zeit dazwischen,
//! wie viele Schritte zwischen zwei Noten liegen und wie sie verteilt sind
//! (gleichmäßig, beschleunigend oder bremsend – die `kdtanz`-Verteilung).
//! Hier steht die hardwarefreie Rechnung; das Touchpad liefert die Weg- und
//! Zeitproben des Wischens.

/// Wie die Zwischenschritte zwischen zwei Tasten verteilt werden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verteilung {
    /// Gleiche Abstände (annähernd linear gewischt).
    Linear,
    /// Abstände werden größer (Wisch wird schneller – beschleunigend).
    Beschleunigend,
    /// Abstände werden kleiner (Wisch wird langsamer – bremsend).
    Bremsend,
}

/// Positionen der Zwischenschritte zwischen zwei Tastendrücken, als Anteil
/// von 0.0 (erster Druck) bis 1.0 (zweiter Druck), ohne die Endpunkte.
///
/// - `Linear`: gleichmäßig (i/n).
/// - `Beschleunigend`: quadratisch zunehmend (i^2/n^2) - die Schritte
///   werden laenger (die Positionen liegen unter der Geraden), wie beim
///   immer schnelleren Wischen.
/// - `Bremsend`: das Spiegelbild (1 - (1 - i/n)²).
pub fn schritt_positionen(anzahl: u32, profil: Verteilung) -> Vec<f32> {
    let n = anzahl.max(1) as f32;
    (1..anzahl.max(1))
        .map(|i| {
            let t = i as f32 / n;
            match profil {
                Verteilung::Linear => t,
                Verteilung::Beschleunigend => t * t,
                Verteilung::Bremsend => 1.0 - (1.0 - t) * (1.0 - t),
            }
        })
        .collect()
}

/// Erkennt das Verteilungsprofil aus den Geschwindigkeiten der Wischproben
/// (Weg je Zeitfenster). Die zweite Hälfte wird mit der ersten verglichen;
/// ein Viertel Unterschied entscheidet, sonst gilt der Wisch als linear.
pub fn profil_aus_geschwindigkeiten(geschwindigkeiten: &[f32]) -> Verteilung {
    if geschwindigkeiten.len() < 4 {
        return Verteilung::Linear;
    }
    let mitte = geschwindigkeiten.len() / 2;
    let anfang: f32 = geschwindigkeiten[..mitte].iter().sum::<f32>() / mitte as f32;
    let ende: f32 =
        geschwindigkeiten[mitte..].iter().sum::<f32>() / (geschwindigkeiten.len() - mitte) as f32;
    if anfang <= 1e-6 {
        return Verteilung::Linear;
    }
    let verhaeltnis = ende / anfang;
    if verhaeltnis > 1.25 {
        Verteilung::Beschleunigend
    } else if verhaeltnis < 0.8 {
        Verteilung::Bremsend
    } else {
        Verteilung::Linear
    }
}

/// Anzahl der Schritte aus der Wischlänge: eine durchstrichene Taste zählt
/// als ein Schritt, mindestens einer, höchstens `max`.
pub fn schritte_aus_wisch(weg_px: f32, taste_px: f32, max: u32) -> u32 {
    if !weg_px.is_finite() || weg_px <= 0.0 {
        return 1;
    }
    let taste = taste_px.max(1.0);
    ((weg_px / taste).round() as u32).clamp(1, max.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_verteilt_gleichmaessig() {
        let p = schritt_positionen(4, Verteilung::Linear);
        assert_eq!(p.len(), 3, "drei Zwischenschritte bei vier Schritten");
        assert!((p[0] - 0.25).abs() < 1e-6);
        assert!((p[1] - 0.50).abs() < 1e-6);
        assert!((p[2] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn beschleunigend_macht_die_schritte_laenger() {
        let p = schritt_positionen(4, Verteilung::Beschleunigend);
        let linear = schritt_positionen(4, Verteilung::Linear);
        assert!(p[0] < linear[0], "erster Zwischenschritt kommt früher");
        assert!(p.windows(2).all(|w| w[1] > w[0]), "monoton");
        // Abstände zwischen 0, den Zwischenschritten und 1 wachsen.
        let mut punkte = vec![0.0];
        punkte.extend(p);
        punkte.push(1.0);
        let abstaende: Vec<f32> = punkte.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(
            abstaende.windows(2).all(|w| w[1] > w[0]),
            "Schritte werden länger: {abstaende:?}"
        );
    }

    #[test]
    fn bremsend_ist_das_spiegelbild() {
        let p = schritt_positionen(4, Verteilung::Bremsend);
        let b = schritt_positionen(4, Verteilung::Beschleunigend);
        for i in 0..p.len() {
            assert!(
                (p[i] + b[b.len() - 1 - i] - 1.0).abs() < 1e-6,
                "Bremsend spiegelt Beschleunigend: {} {}",
                p[i],
                b[b.len() - 1 - i]
            );
        }
    }

    #[test]
    fn randfaelle_der_positionen() {
        assert!(schritt_positionen(1, Verteilung::Linear).is_empty());
        assert_eq!(schritt_positionen(2, Verteilung::Linear).len(), 1);
        let p = schritt_positionen(3, Verteilung::Linear);
        assert!(p.iter().all(|t| (0.0..1.0).contains(t)));
    }

    #[test]
    fn profil_erkennt_die_wischart() {
        assert_eq!(
            profil_aus_geschwindigkeiten(&[1.0, 1.0, 1.0, 1.0]),
            Verteilung::Linear
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[1.0, 1.0, 2.0, 2.0]),
            Verteilung::Beschleunigend
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[2.0, 2.0, 1.0, 1.0]),
            Verteilung::Bremsend
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[1.0, 1.1, 1.0, 1.05]),
            Verteilung::Linear,
            "kleine Schwankungen bleiben linear"
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[]),
            Verteilung::Linear,
            "zu wenig Proben"
        );
    }

    #[test]
    fn schritte_aus_der_wischlaenge() {
        assert_eq!(schritte_aus_wisch(300.0, 60.0, 32), 5);
        assert_eq!(schritte_aus_wisch(10.0, 60.0, 32), 1, "mindestens einer");
        assert_eq!(schritte_aus_wisch(10_000.0, 60.0, 32), 32, "gedeckelt");
        assert_eq!(schritte_aus_wisch(0.0, 60.0, 32), 1);
        assert_eq!(schritte_aus_wisch(f32::NAN, 60.0, 32), 1);
    }
}
