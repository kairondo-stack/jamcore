//! Gemeinsame Transport-Uhr für Rhythmus, Bild und Laub.
//!
//! Warum eine eigene Rechnung im Kern: Jeder Tab hatte bisher seine eigene
//! `Instant`-Uhr (Rhythmus-Render, Bild-Sweep, Laub-Anhören). Startete man
//! zwei Tabs, liefen sie mit einem zufälligen Versatz zueinander, weil jeder
//! Thread seinen eigenen Startpunkt mitbrachte. Hier steht nur die reine
//! Mathematik; die Android-Seite hält einen einzigen Startzeitpunkt und lässt
//! alle drei Erzeuger daraus ihre Phase ableiten.
//!
//! Ein „Takt“ ist ein 4/4-Takt, `feinheit` ist die Zahl der Rasterschritte je
//! Takt (16 = 1/16, 32 = 1/32, 64 = 1/64). Die Schrittnummer läuft innerhalb
//! eines Takts von `0` bis `feinheit - 1`; `takt` zählt die vollen Takte seit
//! dem Start und wächst unbegrenzt weiter.

/// Kleinstes Tempo, das noch sinnvoll gerechnet werden kann.
const BPM_MIN: f64 = 1.0;
/// Größtes Tempo: schneller als 1000 BPM ist kein Musikraster mehr.
const BPM_MAX: f64 = 1000.0;
/// Tempo, wenn der Aufrufer etwas Unbrauchbares (NaN/∞) liefert.
const BPM_VORGABE: f64 = 120.0;

/// Klemmt ein Tempo auf einen endlichen, positiven Bereich.
fn bpm_klemmen(bpm: f32) -> f64 {
    if bpm.is_nan() {
        // NaN kann man nicht klemmen; 120 BPM ist der Vorgabewert der App.
        BPM_VORGABE
    } else {
        // ±∞ klemmt `clamp` auf den jeweiligen Anschlag.
        f64::from(bpm).clamp(BPM_MIN, BPM_MAX)
    }
}

/// Dauer eines Rasterschritts in Sekunden.
///
/// Ein 4/4-Takt dauert `240 / bpm` Sekunden; geteilt durch `feinheit` ergibt
/// das den Abstand zweier Zeilen. Bei 120 BPM und 1/16 sind das 0,125 s —
/// derselbe Wert, den die bisherigen Einzel-Uhren benutzt haben.
pub fn step_dauer(bpm: f32, feinheit: u16) -> f64 {
    let feinheit = f64::from(feinheit.max(1));
    240.0 / (bpm_klemmen(bpm) * feinheit)
}

/// Dauer eines vollen 4/4-Takts in Sekunden.
pub fn takt_dauer(bpm: f32) -> f64 {
    240.0 / bpm_klemmen(bpm)
}

/// Rasterposition seit dem Start, in Schritten (gebrochen).
///
/// `0.0` ist der Start; `1.5` heißt „eineinhalb Schritte nach dem Start“.
/// Unbrauchbare Zeiten (NaN/∞, Zeit vor dem Start) ergeben `0.0`, damit
/// keiner der Erzeuger mit einer NaN-Phase weiterrechnet.
pub fn position_schritte(start_s: f64, bpm: f32, feinheit: u16, jetzt_s: f64) -> f64 {
    if !(start_s.is_finite() && jetzt_s.is_finite()) {
        return 0.0;
    }
    // Eine Uhr, die vor dem Start gefragt wird, steht still (0), statt
    // rückwärts zu laufen.
    let verstrichen = (jetzt_s - start_s).max(0.0);
    let schritte = verstrichen / step_dauer(bpm, feinheit);
    if schritte.is_finite() {
        schritte
    } else {
        0.0
    }
}

/// Phase der gemeinsamen Uhr: `(step, takt, anteil)`.
///
/// - `step`: Rasterschritt im aktuellen Takt (`0..feinheit`),
/// - `takt`: volle Takte seit dem Start (wächst unbegrenzt),
/// - `anteil`: Bruchteil des aktuellen Schritts (`0.0..1.0`).
///
/// Der Bruchteil ist der Feinsync: Bild-Sweep und Anhören können daraus ihre
/// Position zwischen zwei Rasterzeilen ableiten, statt auf den nächsten
/// Schritt zu warten.
pub fn phase(start_s: f64, bpm: f32, feinheit: u16, jetzt_s: f64) -> (u16, u64, f32) {
    let feinheit = feinheit.max(1);
    let position = position_schritte(start_s, bpm, feinheit, jetzt_s);
    let schritte_je_takt = f64::from(feinheit);
    // `as` sättigt in Rust: nach Jahrzehnten Laufzeit bleibt `takt` endlich,
    // statt überzulaufen.
    let takt = (position / schritte_je_takt).floor() as u64;
    let step = position.rem_euclid(schritte_je_takt).floor() as u16;
    let anteil = position.fract() as f32;
    (step, takt, anteil.clamp(0.0, 1.0))
}

/// Zeitversatz zweier Startzeitpunkte in Sekunden.
///
/// `0.0` genau dann, wenn beide Erzeuger dieselbe Uhr benutzen (gleicher
/// Start). Ein positiver Wert heißt: `start_b` liegt später als `start_a`.
/// Unbrauchbare Werte ergeben `0.0` — ein NaN-Versatz würde jede Phase
/// vergiften.
pub fn sync_versatz(start_a: f64, start_b: f64) -> f64 {
    if !(start_a.is_finite() && start_b.is_finite()) {
        return 0.0;
    }
    start_b - start_a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nah(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn step_dauer_bei_120_bpm_und_16tel() {
        nah(step_dauer(120.0, 16), 0.125);
        nah(step_dauer(120.0, 32), 0.0625);
        // Ein voller Takt bei 120 BPM ist zwei Sekunden lang.
        nah(takt_dauer(120.0), 2.0);
        nah(step_dauer(60.0, 4), 1.0);
    }

    #[test]
    fn step_dauer_klemmt_unsinn() {
        // 0/negative Tempi werden auf den kleinsten Wert geklemmt, NaN auf 120.
        nah(step_dauer(0.0, 16), step_dauer(1.0, 16));
        nah(step_dauer(-10.0, 16), step_dauer(1.0, 16));
        nah(step_dauer(f32::NAN, 16), step_dauer(120.0, 16));
        nah(step_dauer(f32::INFINITY, 16), step_dauer(1000.0, 16));
        // Feinheit 0 würde durch null teilen; sie zählt als ein Schritt.
        nah(step_dauer(120.0, 0), takt_dauer(120.0));
    }

    #[test]
    fn phase_am_start_ist_null() {
        assert_eq!(phase(10.0, 120.0, 16, 10.0), (0, 0, 0.0));
        // Auch eine Uhr, die „vor“ ihrem Start gefragt wird, steht bei null.
        assert_eq!(phase(10.0, 120.0, 16, 9.0), (0, 0, 0.0));
    }

    #[test]
    fn phase_zaehlt_schritte_und_anteil() {
        // Bei 120 BPM/1/16 ist ein Schritt 0,125 s: nach 0,0625 s halb voll.
        let (step, takt, anteil) = phase(0.0, 120.0, 16, 0.0625);
        assert_eq!((step, takt), (0, 0));
        assert!((anteil - 0.5).abs() < 1e-6, "anteil = {anteil}");

        let (step, takt, anteil) = phase(0.0, 120.0, 16, 0.1875);
        assert_eq!((step, takt), (1, 0));
        assert!((anteil - 0.5).abs() < 1e-6, "anteil = {anteil}");
    }

    #[test]
    fn phase_rollt_nach_einem_takt() {
        // 2 s = ein Takt bei 120 BPM: danach wieder Schritt 0, aber Takt 1.
        let (step, takt, anteil) = phase(0.0, 120.0, 16, 2.0);
        assert_eq!((step, takt), (0, 1));
        assert_eq!(anteil, 0.0);
        // 2 s + 3 Schritte (0,375 s) = Takt 1, Schritt 3.
        let (step, takt, _) = phase(0.0, 120.0, 16, 2.375);
        assert_eq!((step, takt), (3, 1));
    }

    #[test]
    fn phase_mit_feinheit_32_halbiert_die_schritte() {
        // Bei 1/32 ist derselbe Zeitpunkt doppelt so viele Schritte weit:
        // 0,1875 s sind hier genau 3 Schritte (bei 1/16 wären es 1,5).
        let (step, takt, anteil) = phase(0.0, 120.0, 32, 0.1875);
        assert_eq!((step, takt), (3, 0));
        assert_eq!(anteil, 0.0);
        // 0,15625 s = 2,5 Schritte à 0,0625 s: halber Schritt 2.
        let (step, takt, anteil) = phase(0.0, 120.0, 32, 0.15625);
        assert_eq!((step, takt), (2, 0));
        assert!((anteil - 0.5).abs() < 1e-6, "anteil = {anteil}");
        // Nach einem Takt (2 s) wieder Schritt 0, Takt 1.
        assert_eq!(phase(0.0, 120.0, 32, 2.0), (0, 1, 0.0));
    }

    #[test]
    fn phase_unsinnige_werte_panikt_nicht() {
        assert_eq!(phase(f64::NAN, 120.0, 16, 1.0), (0, 0, 0.0));
        assert_eq!(phase(0.0, 120.0, 16, f64::INFINITY), (0, 0, 0.0));
        assert_eq!(phase(0.0, f32::NAN, 16, 1.0), phase(0.0, 120.0, 16, 1.0));
        // Extreme Zeiten saturieren, statt zu überlaufen.
        let (step, takt, anteil) = phase(0.0, 120.0, 16, 1.0e9);
        assert!(step < 16);
        assert!(takt > 0);
        assert!((0.0..=1.0).contains(&anteil));
    }

    #[test]
    fn position_ist_stetig_und_gebrochen() {
        nah(position_schritte(0.0, 120.0, 16, 0.0625), 0.5);
        nah(position_schritte(0.0, 120.0, 16, 0.125), 1.0);
        // Nach einem Takt bei 1/16 sind es genau 16 Schritte.
        nah(position_schritte(0.0, 120.0, 16, 2.0), 16.0);
        // Startversatz wird abgezogen, nicht ignoriert.
        nah(position_schritte(1.0, 120.0, 16, 1.125), 1.0);
    }

    #[test]
    fn sync_versatz_gleiche_uhr_ist_null() {
        assert_eq!(sync_versatz(12.5, 12.5), 0.0);
        assert_eq!(sync_versatz(0.0, 0.0), 0.0);
        assert_eq!(sync_versatz(-3.0, -3.0), 0.0);
    }

    #[test]
    fn sync_versatz_ist_die_zeitdifferenz() {
        nah(sync_versatz(10.0, 10.25), 0.25);
        nah(sync_versatz(10.25, 10.0), -0.25);
    }

    #[test]
    fn sync_versatz_unsinn_ist_null() {
        assert_eq!(sync_versatz(f64::NAN, 1.0), 0.0);
        assert_eq!(sync_versatz(1.0, f64::INFINITY), 0.0);
    }
}
