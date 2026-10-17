//! Oszillator-Scan (Plan 2026-10-17, Step 1): wie ein Durchlauf über die
//! x-Achse läuft (Sweep oder alles gleichzeitig), auf welcher Basis die
//! Harmonie-Bänder liegen (ganze x-Achse oder Scanbereich), welche Harmonie an
//! einem x-Anteil gilt und welche Note aus einer Bildzeile kommt.
//!
//! Alles reine Funktionen ohne Hardware — die App (Bild/B2) und der Desktop
//! (rrn5) bauen darauf ihre Linien- und Bereichs-Zeit auf.

/// Wie läuft ein Durchlauf über die x-Achse?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durchlauf {
    /// Von links nach rechts; nach der Dauer beginnt er neu.
    Sweep,
    /// Alles gleichzeitig — kein Durchlauf über die Zeit.
    Alles,
}

/// x-Anteil (0…1) zum Zeitpunkt `takte` bei gegebener Dauer in Takten
/// (1/64…4). `None` = alles gleichzeitig ([`Durchlauf::Alles`]).
///
/// `takte` läuft modulo der Dauer: nach einem Durchlauf beginnt er neu.
/// `Sweep` liefert 0 am Anfang und →1 am Ende (geklemmt). Die Dauer wird auf
/// 1/64…4 Takt geklemmt; `NaN` ergibt die Vorgabe 1/4 Takt.
pub fn anteil_je_zeit(takte: f64, dauer_takte: f64, durchlauf: Durchlauf) -> Option<f64> {
    let dauer = if dauer_takte.is_nan() {
        0.25
    } else {
        dauer_takte.clamp(1.0 / 64.0, 4.0)
    };
    match durchlauf {
        Durchlauf::Alles => None,
        Durchlauf::Sweep => {
            let t = if takte.is_finite() {
                takte.rem_euclid(dauer)
            } else {
                0.0
            };
            Some((t / dauer).clamp(0.0, 1.0))
        }
    }
}

/// Basis für die Harmonie-Bänder: ganze x-Achse (z. B. 128 Schritte) oder der
/// Scanbereich (z. B. 36).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XBasis {
    /// Die ganze x-Achse, Raster = `gesamt_schritte`.
    Gesamt,
    /// Nur der Scanbereich, Raster = die Schritte im Bereich.
    Bereich,
}

/// Anteil (0…1) der Bänder auf der gewählten Basis + die Anzahl Schritte, die
/// das Band-Raster hat.
///
/// - [`XBasis::Gesamt`]: `(0, 1)` — die Bänder liegen auf der ganzen Achse mit
///   `gesamt_schritte` Schritten.
/// - [`XBasis::Bereich`]: der Scanbereich in Anteilen (geklemmt auf 0…1,
///   `von ≤ bis`), mindestens einen Rasterschritt breit.
pub fn harm_basis(basis: XBasis, gesamt_schritte: u16, bereich: (f32, f32)) -> (f32, f32) {
    // Ein Raster ohne Schritte ist kein Raster: dann gilt ein Schritt.
    let raster = gesamt_schritte.max(1) as f32;
    match basis {
        XBasis::Gesamt => (0.0, 1.0),
        XBasis::Bereich => {
            let von = bereich.0.clamp(0.0, 1.0);
            let bis = bereich.1.clamp(0.0, 1.0);
            let (von, bis) = (von.min(bis), von.max(bis));
            // Mindestens einen Rasterschritt breit, sonst träfe nie ein Band.
            (von, bis.max((von + 1.0 / raster).min(1.0)))
        }
    }
}

/// Welche Harmonie (Index) gilt an diesem x-Anteil? Bänder als
/// `(von, bis, index)` in Anteilen; Treffer = Index, sonst `None`.
/// Überlappende Bänder: das erste.
pub fn harmonie_an(x_anteil: f32, baender: &[(f32, f32, usize)]) -> Option<usize> {
    baender
        .iter()
        .find(|(von, bis, _)| x_anteil >= *von && x_anteil < *bis)
        .map(|(_, _, index)| *index)
}

/// Note aus der Zeile: y-Anteil (0 oben … 1 unten) → MIDI-Note (wie
/// `note_volume` im Bild-Scan). 0…1 ergibt `offset … offset + noten - 1`
/// (oben die höchste), geklemmt auf 0…119.
pub fn note_fuer_zeile(y_anteil: f32, noten: i32, offset: i32) -> i32 {
    let noten = noten.max(1);
    let y = if y_anteil.is_nan() {
        0.0
    } else {
        y_anteil.clamp(0.0, 1.0)
    };
    // y = 0 ist oben und damit die höchste Note des Bereichs.
    let anteil = ((1.0 - y) * (noten - 1) as f32).round() as i32;
    (offset + anteil).clamp(0, 119)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_am_anfang_und_ende() {
        // Sweep: 0 am Anfang, gegen 1 am Ende des Durchlaufs.
        assert_eq!(anteil_je_zeit(0.0, 1.0, Durchlauf::Sweep), Some(0.0));
        assert_eq!(anteil_je_zeit(0.5, 1.0, Durchlauf::Sweep), Some(0.5));
        assert_eq!(anteil_je_zeit(0.75, 1.0, Durchlauf::Sweep), Some(0.75));
        let fast_ende = anteil_je_zeit(0.999, 1.0, Durchlauf::Sweep).unwrap();
        assert!(fast_ende > 0.99 && fast_ende < 1.0, "fast Ende: {fast_ende}");
    }

    #[test]
    fn sweep_laeuft_modulo() {
        // Nach einem Durchlauf beginnt der Sweep neu (takte modulo Dauer).
        assert_eq!(anteil_je_zeit(1.25, 1.0, Durchlauf::Sweep), Some(0.25));
        assert_eq!(anteil_je_zeit(3.25, 1.0, Durchlauf::Sweep), Some(0.25));
        // Genau am Ende des Durchlaufs beginnt der nächste (Anteil 0).
        assert_eq!(anteil_je_zeit(1.0, 1.0, Durchlauf::Sweep), Some(0.0));
        // Negative Takte laufen im selben Raster weiter.
        assert_eq!(anteil_je_zeit(-0.75, 1.0, Durchlauf::Sweep), Some(0.25));
        // Kürzeste Dauer (1/64 Takt): 2,5 Takte sind halb herum.
        let d = 1.0 / 64.0;
        assert_eq!(anteil_je_zeit(3.0 * d, d, Durchlauf::Sweep), Some(0.0));
        assert_eq!(anteil_je_zeit(2.5 * d, d, Durchlauf::Sweep), Some(0.5));
    }

    #[test]
    fn alles_gleichzeitig() {
        // Durchlauf::Alles liefert keinen x-Anteil — alles klingt gleichzeitig.
        assert_eq!(anteil_je_zeit(0.0, 1.0, Durchlauf::Alles), None);
        assert_eq!(anteil_je_zeit(0.5, 1.0, Durchlauf::Alles), None);
        assert_eq!(anteil_je_zeit(99.0, f64::NAN, Durchlauf::Alles), None);
    }

    #[test]
    fn dauer_wird_geklemmt() {
        // Untergrenze 1/64 Takt: 1/128 Takt ist halb herum.
        assert_eq!(anteil_je_zeit(1.0 / 128.0, 0.0, Durchlauf::Sweep), Some(0.5));
        assert_eq!(
            anteil_je_zeit(1.0 / 128.0, -3.0, Durchlauf::Sweep),
            Some(0.5)
        );
        // Obergrenze 4 Takte: 2 Takte bei 100 Takten Dauer sind halb herum.
        assert_eq!(anteil_je_zeit(2.0, 100.0, Durchlauf::Sweep), Some(0.5));
        // NaN-Dauer: Vorgabe 1/4 Takt.
        assert_eq!(anteil_je_zeit(0.125, f64::NAN, Durchlauf::Sweep), Some(0.5));
        // Genau die Grenzen bleiben stehen.
        assert_eq!(
            anteil_je_zeit(1.0 / 128.0, 1.0 / 64.0, Durchlauf::Sweep),
            Some(0.5)
        );
        assert_eq!(anteil_je_zeit(2.0, 4.0, Durchlauf::Sweep), Some(0.5));
    }

    #[test]
    fn gesamt_oder_bereich_basis() {
        // Gesamt: die Bänder liegen auf der ganzen Achse.
        assert_eq!(harm_basis(XBasis::Gesamt, 128, (0.25, 0.75)), (0.0, 1.0));
        assert_eq!(harm_basis(XBasis::Gesamt, 36, (0.0, 0.0)), (0.0, 1.0));
        // Bereich: der (geklemmte, geordnete) Scanbereich in Anteilen.
        assert_eq!(harm_basis(XBasis::Bereich, 128, (0.25, 0.75)), (0.25, 0.75));
        assert_eq!(harm_basis(XBasis::Bereich, 128, (0.75, 0.25)), (0.25, 0.75));
        assert_eq!(harm_basis(XBasis::Bereich, 128, (-0.5, 1.5)), (0.0, 1.0));
        // Ein Bereich schmaler als ein Rasterschritt wird auf einen Schritt
        // gedehnt (sonst träfe ihn nie ein Band).
        assert_eq!(
            harm_basis(XBasis::Bereich, 128, (0.5, 0.5)),
            (0.5, 0.5 + 1.0 / 128.0)
        );
    }

    #[test]
    fn harmonie_wechselt_mit_x() {
        // Bänder in Anteilen; die Grenze gehört zum nächsten Band (halboffen).
        let baender = [(0.0, 0.25, 4), (0.25, 0.5, 0), (0.5, 1.0, 7)];
        assert_eq!(harmonie_an(0.0, &baender), Some(4));
        assert_eq!(harmonie_an(0.1, &baender), Some(4));
        assert_eq!(harmonie_an(0.25, &baender), Some(0));
        assert_eq!(harmonie_an(0.49, &baender), Some(0));
        assert_eq!(harmonie_an(0.5, &baender), Some(7));
        assert_eq!(harmonie_an(0.999, &baender), Some(7));
        // Überlappende Bänder: das erste gewinnt.
        let ueberlappt = [(0.0, 0.6, 2), (0.4, 1.0, 5)];
        assert_eq!(harmonie_an(0.5, &ueberlappt), Some(2));
    }

    #[test]
    fn harmonie_ausserhalb_ist_none() {
        let baender = [(0.2, 0.8, 1)];
        assert_eq!(harmonie_an(0.1, &baender), None);
        assert_eq!(harmonie_an(0.9, &baender), None);
        assert_eq!(harmonie_an(-0.5, &baender), None);
        assert_eq!(harmonie_an(1.5, &baender), None);
        // Genau am `bis` ist das Band zu Ende (halboffen).
        assert_eq!(harmonie_an(0.8, &baender), None);
        // Ohne Bänder gibt es nichts zu treffen.
        assert_eq!(harmonie_an(0.5, &[]), None);
    }

    #[test]
    fn note_fuer_zeile_normiert() {
        // Oben die höchste, unten die tiefste Note des Bereichs.
        assert_eq!(note_fuer_zeile(0.0, 48, 60), 107);
        assert_eq!(note_fuer_zeile(1.0, 48, 60), 60);
        assert_eq!(note_fuer_zeile(0.5, 48, 60), 84);
        // Anteile außerhalb 0…1 werden geklemmt.
        assert_eq!(note_fuer_zeile(-1.0, 48, 60), 107);
        assert_eq!(note_fuer_zeile(2.0, 48, 60), 60);
        // MIDI-Grenzen 0…119 gelten auch bei großen Verschiebungen.
        assert_eq!(note_fuer_zeile(0.0, 48, 100), 119);
        assert_eq!(note_fuer_zeile(1.0, 48, -10), 0);
        // Ein Notenbereich von 1 liefert überall denselben Ton.
        assert_eq!(note_fuer_zeile(0.3, 1, 60), 60);
    }
}
