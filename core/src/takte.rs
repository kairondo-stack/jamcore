//! Taktdauer des Bild-Sweeps: ein Sweep über die Bildbreite ist die Einheit,
//! wie im Desktop-`rrn5` (`pixelsPerBeat = xw * yh`, ein Durchlauf = ein Beat).
//! Der Fader sagt, **wie viele Takte** ein Sweep dauert, und wird mit ÷2 ×2 ÷3 ×3
//! bedient — genau die Pfeiltasten des pattern_recorder bei „Dauer in Beats?"
//! (`l` `p`, dann `←` ÷2, `→` ×2, `↑` ÷3, `↓` ×3).
//!
//! Vorgabe ist 1/4 Takt, also ein Beat im 4/4-Takt; der Bereich reicht von
//! 1/36 Takt bis 36 Takte.

/// Vorgabe: ein Sweep dauert ein Vierteltakt (ein Beat im 4/4).
pub const VORGABE: f64 = 0.25;
/// Kleinster Wert des Faders: 1/36 Takt (ein Sechsunddreißigstel).
pub const MIN: f64 = 1.0 / 36.0;
/// Größter Wert des Faders: 36 Takte.
pub const MAX: f64 = 36.0;
/// Wie viele Schläge ein Takt hat (4/4); davon hängt die Umrechnung in Sekunden ab.
pub const SCHLAEGE_PRO_TAKT: f64 = 4.0;

/// Durch `n` teilen (der Faderknopf „÷2" bzw. „÷3"), geklemmt.
pub fn teile(takte: f64, n: f64) -> f64 {
    klemmen(takte / n.max(1.0))
}

/// Mit `n` malnehmen (der Faderknopf „×2" bzw. „×3"), geklemmt.
pub fn vielfache(takte: f64, n: f64) -> f64 {
    klemmen(takte * n.max(1.0))
}

/// In den Bereich 1/36 … 36 Takte klemmen; unbrauchbare Werte werden zur Vorgabe.
pub fn klemmen(takte: f64) -> f64 {
    if takte.is_nan() {
        return VORGABE;
    }
    takte.clamp(MIN, MAX)
}

/// Anzeige für den Fader: ganze Takte als „n Takte", Brüche als Bruchteil
/// eines Taktes („1/36 Takt"), gemischte Werte als Bruch („3/2 Takte").
pub fn text(takte: f64) -> String {
    let t = klemmen(takte);
    if t >= 1.0 {
        let gerundet = t.round();
        if (t - gerundet).abs() < 1e-6 {
            return if gerundet == 1.0 {
                "1 Takt".to_string()
            } else {
                format!("{gerundet:.0} Takte")
            };
        }
    }
    // Bruch suchen: kleinste Nennerzahl bis 36, bei der der Wert ganzzahlig wird.
    for nenner in 1..=36usize {
        let zaehler = t * nenner as f64;
        if (zaehler - zaehler.round()).abs() < 1e-6 && zaehler >= 1.0 {
            return if zaehler.round() == 1.0 {
                format!("{zaehler:.0}/{nenner} Takt")
            } else {
                format!("{zaehler:.0}/{nenner} Takte")
            };
        }
    }
    format!("{t:.2} Takte")
}

/// Dauer eines Sweeps über die Bildbreite in Sekunden: so viele Takte mal
/// Schläge je Takt mal Sekunden je Schlag. Tempo 0 oder kleiner liefert eine
/// sehr lange, aber endliche Dauer (nie 0, nie unendlich).
pub fn sweep_sekunden(takte: f64, bpm: f64) -> f64 {
    let takte = klemmen(takte);
    let tempo = if bpm.is_nan() { VORGABE_BPM } else { bpm };
    let tempo = tempo.clamp(1.0, 1000.0);
    takte * SCHLAEGE_PRO_TAKT * 60.0 / tempo
}

/// Tempo, wenn keins angegeben ist (120 BPM wie im rrn5-Vorgabewert).
const VORGABE_BPM: f64 = 120.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teiler_halbiert_und_verdoppelt() {
        assert_eq!(teile(0.25, 2.0), 0.125);
        assert_eq!(teile(0.25, 3.0), 0.25 / 3.0);
        assert_eq!(vielfache(0.25, 2.0), 0.5);
        assert_eq!(vielfache(0.25, 3.0), 0.75);
    }

    #[test]
    fn klemmen_haelt_den_bereich_ein_und_dreiunddreissig_bis_vierunddreissig() {
        assert_eq!(klemmen(0.25), 0.25, "Vorgabe bleibt");
        assert_eq!(klemmen(1e9), MAX);
        assert_eq!(klemmen(f64::INFINITY), MAX);
        assert_eq!(klemmen(0.0), MIN, "Null würde den Sweep anhalten");
        assert_eq!(klemmen(1.0 / 108.0), MIN, "1/108 ist zu klein");
        assert_eq!(klemmen(f64::NAN), VORGABE, "Müll wird Vorgabe");
        assert_eq!(teile(MIN, 3.0), MIN, "am Anschlag bleibt der Anschlag");
        assert_eq!(vielfache(MAX, 3.0), MAX);
        assert_eq!(teile(0.0, 2.0), MIN, "÷2 von Null klemmt");
        assert_eq!(teile(0.25, 0.0), 0.25, "÷0 ist wirkungslos");
        assert_eq!(
            vielfache(0.25, -2.0),
            0.25,
            "negativer Faktor ist wirkungslos"
        );
    }

    #[test]
    fn vorgabe_ist_ein_vierteltakt() {
        // rrn5: ein Durchlauf ueber das Bild ist ein Beat, im 4/4 ein Vierteltakt.
        assert_eq!(VORGABE, 0.25);
        assert_eq!(MIN, 1.0 / 36.0);
        assert_eq!(MAX, 36.0);
    }

    #[test]
    fn text_schreibt_frac_und_ganzzahlig() {
        assert_eq!(text(0.25), "1/4 Takt");
        assert_eq!(text(0.5), "1/2 Takt");
        assert_eq!(text(1.0 / 36.0), "1/36 Takt");
        assert_eq!(text(1.0), "1 Takt");
        assert_eq!(text(2.0), "2 Takte");
        assert_eq!(text(36.0), "36 Takte");
        assert_eq!(text(1.5), "3/2 Takte", "drittel und halb mischen sich");
    }

    #[test]
    fn sweep_dauert_einen_beat() {
        // rrn5: ein Durchlauf ueber das Bild ist ein Beat. 1/4 Takt bei
        // 120 BPM sind 0,5 Sekunden.
        let sekunden = sweep_sekunden(VORGABE, 120.0);
        assert!((sekunden - 0.5).abs() < 1e-9, "{sekunden}");
    }

    #[test]
    fn sweep_kann_kurz_und_lang_werden() {
        assert!(
            (sweep_sekunden(4.0, 120.0) - 8.0).abs() < 1e-9,
            "4 Takte bei 120 BPM sind 8 s"
        );
        assert!(
            (sweep_sekunden(MIN, 120.0) - 1.0 / 18.0).abs() < 1e-9,
            "1/36 Takt ist ein 36tel Schlag: {:.4} s",
            sweep_sekunden(MIN, 120.0)
        );
        assert!(
            (sweep_sekunden(MAX, 120.0) - 72.0).abs() < 1e-9,
            "36 Takte bei 120 BPM sind 36*4*0,5 s"
        );
    }

    #[test]
    fn unbrauchbare_werte_werden_vorgabe() {
        assert!(
            (sweep_sekunden(f64::NAN, 120.0) - 0.5).abs() < 1e-9,
            "NaN bei den Takten"
        );
        assert!(
            (sweep_sekunden(1.0, 0.0) - 240.0).abs() < 1e-9,
            "Tempo 0 bedeutet unendlich lang, nicht unendlich kurz: {:.1} s",
            sweep_sekunden(1.0, 0.0)
        );
        assert!(
            sweep_sekunden(1.0, -120.0) > 0.0,
            "negatives Tempo ergibt keine Zeit"
        );
    }
}
