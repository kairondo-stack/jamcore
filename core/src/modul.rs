//! Modulkopf-Werte auf die App-Uhr umrechnen.

/// App-BPM aus dem Tempo (BPM) und Speed (Ticks je Zeile) eines IT-Moduls.
///
/// Warum die Formel: im IT dauert eine Zeile `speed * 2.5 / bpm_it` Sekunden
/// (ein Tick = `2.5 / bpm_it`, vier Zeilen je Beat). Die App legt eine Zeile
/// auf 1/16, also `60 / (bpm_app * 4)` Sekunden. Gleichsetzen ergibt
/// `bpm_app = 6 * bpm_it / speed`; Speed 6 ist damit identisch, Speed 3
/// doppelt so schnell wie der IT-Kopf.
///
/// `None` bei Werten <= 0 oder NaN — dann bleibt das App-Tempo unveraendert.
pub fn bpm_aus_it(bpm_it: f64, speed: f64) -> Option<f64> {
    if !bpm_it.is_finite() || !speed.is_finite() || bpm_it <= 0.0 || speed <= 0.0 {
        return None;
    }
    Some(6.0 * bpm_it / speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nah(ist: Option<f64>, soll: f64) {
        let ist = ist.expect("Tempo muss berechenbar sein");
        assert!((ist - soll).abs() < 1e-9, "{ist} statt {soll}");
    }

    #[test]
    fn standard_tempo_bleibt() {
        // Initial Speed 6 ist die Vorgabe eines IT: eine Zeile je Sechzehntel.
        nah(bpm_aus_it(125.0, 6.0), 125.0);
    }

    #[test]
    fn halber_speed_verdoppelt_das_tempo() {
        nah(bpm_aus_it(125.0, 3.0), 250.0);
    }

    #[test]
    fn andere_werte_rechnen_mit() {
        nah(bpm_aus_it(150.0, 4.0), 225.0);
    }

    #[test]
    fn null_und_negativ_ergeben_nichts() {
        assert!(bpm_aus_it(0.0, 6.0).is_none(), "BPM 0");
        assert!(bpm_aus_it(125.0, 0.0).is_none(), "Speed 0");
        assert!(bpm_aus_it(0.0, 0.0).is_none(), "beide 0");
        assert!(bpm_aus_it(-125.0, 6.0).is_none(), "BPM negativ");
        assert!(bpm_aus_it(125.0, -6.0).is_none(), "Speed negativ");
        assert!(bpm_aus_it(f64::NAN, 6.0).is_none(), "BPM NaN");
    }
}
