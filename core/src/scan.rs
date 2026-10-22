//! Scan-Linie und Scan-Bereich per Knopf verschieben (Bild-Tab).
//!
//! Reine Rechnung ohne egui, damit sie auf dem Host testbar ist; die
//! Android-Oberfläche (`android/src/image.rs`) hält nur den Zustand.

/// Verschiebt `[von, bis]` um `delta`, die Breite bleibt erhalten, und klemmt
/// an `0..1`. `von > bis` wird normalisiert; ein voller Bereich (Breite 1)
/// lässt sich nicht verschieben. Unbrauchbare Werte (NaN/∞) bleiben
/// unverändert, damit die Oberfläche nichts erfindet.
pub fn verschiebe_bereich(von: f32, bis: f32, delta: f32) -> (f32, f32) {
    // Unbrauchbare Werte nicht „reparieren": die Oberfläche soll bei NaN
    // lieber den alten Zustand zeigen als einen erfundenen Bereich.
    if !(von.is_finite() && bis.is_finite() && delta.is_finite()) {
        return (von, bis);
    }
    let (von, bis) = if von <= bis { (von, bis) } else { (bis, von) };
    let von = von.clamp(0.0, 1.0);
    let bis = bis.clamp(0.0, 1.0);
    let breite = bis - von;
    // Der volle Bereich hat keinen Platz zum Wandern; an den Rändern würde
    // sonst gestaucht (Breite bliebe nicht erhalten).
    if breite >= 1.0 {
        return (0.0, 1.0);
    }
    let neu_von = (von + delta).clamp(0.0, 1.0 - breite);
    (neu_von, neu_von + breite)
}

/// Verschiebt eine Linie/Position um `delta` und klemmt an `0..1`.
/// Unbrauchbare Werte (NaN/∞) bleiben unverändert.
pub fn verschiebe_position(x: f32, delta: f32) -> f32 {
    if !(x.is_finite() && delta.is_finite()) {
        return x;
    }
    (x + delta).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vergleich mit Toleranz: 1-%-Schritte sind als f32 nicht exakt.
    fn nah(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn bereich_mitte_verschiebt_beide_grenzen() {
        let (von, bis) = verschiebe_bereich(0.2, 0.6, 0.01);
        nah(von, 0.21);
        nah(bis, 0.61);
    }

    #[test]
    fn bereich_links_an_der_wand_bleibt_stehen() {
        // Kein Stauchen: an 0.0 stoppt der ganze Bereich, die Breite bleibt.
        let (von, bis) = verschiebe_bereich(0.0, 0.4, -0.1);
        nah(von, 0.0);
        nah(bis, 0.4);
    }

    #[test]
    fn bereich_rechts_an_der_wand_bleibt_stehen() {
        let (von, bis) = verschiebe_bereich(0.6, 1.0, 0.1);
        nah(von, 0.6);
        nah(bis, 1.0);
    }

    #[test]
    fn bereich_delta_null_aendert_nichts() {
        let (von, bis) = verschiebe_bereich(0.2, 0.6, 0.0);
        nah(von, 0.2);
        nah(bis, 0.6);
    }

    #[test]
    fn bereich_von_groesser_bis_wird_normalisiert() {
        let (von, bis) = verschiebe_bereich(0.6, 0.2, 0.1);
        nah(von, 0.3);
        nah(bis, 0.7);
    }

    #[test]
    fn voller_bereich_ist_unverschiebbar() {
        let (von, bis) = verschiebe_bereich(0.0, 1.0, 0.1);
        nah(von, 0.0);
        nah(bis, 1.0);
    }

    #[test]
    fn nan_laesst_bereich_unveraendert() {
        let (von, bis) = verschiebe_bereich(f32::NAN, 0.5, 0.1);
        assert!(von.is_nan());
        nah(bis, 0.5);
        let (von, bis) = verschiebe_bereich(0.2, 0.6, f32::NAN);
        nah(von, 0.2);
        nah(bis, 0.6);
    }

    #[test]
    fn position_mitte_und_waende() {
        nah(verschiebe_position(0.5, 0.01), 0.51);
        nah(verschiebe_position(0.0, -0.1), 0.0);
        nah(verschiebe_position(1.0, 0.1), 1.0);
    }

    #[test]
    fn position_nan_bleibt_unveraendert() {
        assert!(verschiebe_position(f32::NAN, 0.1).is_nan());
        let x = 0.42;
        assert_eq!(verschiebe_position(x, f32::NAN), x);
    }
}
