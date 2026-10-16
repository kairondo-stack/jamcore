//! Stroboskop-Ansicht der Bildquelle.
//!
//! Das Bild wird als Streifen **mehrfach** gezeigt und mit der Phase der
//! Lissajous-Figur synchron zum Takt in eine Richtung bewegt (endlose Drehung,
//! Richtung wählbar). Der Bild-Scan liest diese bewegte Ansicht: eine Spalte
//! der Ansicht gehört zur Quellspalte `(ansicht - versatz) mod Breite`.
//!
//! Formel wie `jamcore::lissa` (Max-Patch `dr22`/`lissa-2020optic`):
//! `φ = 2π · Umdrehungen · Takte / Takte_pro_Zyklus`, Versatz = φ/2π (Anteil
//! der Bildbreite, 0…1).

use crate::lissa::{self, Modus};
use std::f64::consts::TAU;

/// Versatz der Ansicht als Anteil der Bildbreite (0…1).
/// `rechts = true` schiebt das Bild nach rechts, sonst nach links.
pub fn versatz_anteil(takte: f64, takte_pro_zyklus: f64, umdrehungen: f64, rechts: bool) -> f64 {
    let modus = if rechts {
        Modus::UnendlichRechts
    } else {
        Modus::UnendlichLinks
    };
    let phase = lissa::phase(takte, takte_pro_zyklus, umdrehungen, modus);
    (phase / TAU).rem_euclid(1.0)
}

/// Versatz in Pixeln für eine Bildbreite.
pub fn versatz_pixel(versatz_anteil: f64, breite: i32) -> i32 {
    if breite <= 0 || !versatz_anteil.is_finite() {
        return 0;
    }
    ((versatz_anteil.rem_euclid(1.0)) * breite as f64).round() as i32 % breite
}

/// Quellspalte für eine Spalte der bewegten Ansicht (`w` = Bildbreite).
pub fn quell_spalte(ansicht_x: i32, versatz_px: i32, w: i32) -> i32 {
    if w <= 0 {
        return 0;
    }
    (ansicht_x - versatz_px).rem_euclid(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versatz_beginnt_bei_null() {
        assert!(versatz_anteil(0.0, 2.0, 1.0, true).abs() < 1e-12);
        assert!(versatz_anteil(0.0, 2.0, 1.0, false).abs() < 1e-12);
    }

    #[test]
    fn versatz_laeuft_in_eine_richtung() {
        // 2 Takte je Zyklus, 1 Umdrehung: nach einem Takt ist es halb herum.
        let rechts = versatz_anteil(1.0, 2.0, 1.0, true);
        assert!((rechts - 0.5).abs() < 1e-12, "rechts: {rechts}");
        // Bei einem Viertelzyklus unterscheiden sich die Richtungen — genau
        // der Fall, der ohne Richtungslogik fehlschlägt.
        let rechts = versatz_anteil(0.5, 2.0, 1.0, true);
        let links = versatz_anteil(0.5, 2.0, 1.0, false);
        assert!((rechts - 0.25).abs() < 1e-12, "rechts: {rechts}");
        assert!((links - 0.75).abs() < 1e-12, "links ist gespiegelt: {links}");
        // Nach einem vollen Zyklus ist der Versatz wieder bei null (mod 1).
        let voll = versatz_anteil(2.0, 2.0, 1.0, true);
        assert!(voll.abs() < 1e-12, "voller Zyklus: {voll}");
        // Dazwischen monoton (bis zum Umschlag).
        let mut letzter = versatz_anteil(0.0, 2.0, 1.0, true);
        let mut t = 0.1;
        while t < 1.9 {
            let v = versatz_anteil(t, 2.0, 1.0, true);
            assert!(v > letzter, "monoton bis zum Umschlag: {v} > {letzter}");
            letzter = v;
            t += 0.1;
        }
        // Mehr Umdrehungen = schneller.
        let schnell = versatz_anteil(0.5, 2.0, 4.0, true);
        assert!((schnell - 0.0).abs() < 1e-9 || schnell > 0.9, "4 Umdrehungen: {schnell}");
    }

    #[test]
    fn versatz_in_pixeln() {
        assert_eq!(versatz_pixel(0.0, 100), 0);
        assert_eq!(versatz_pixel(0.5, 100), 50);
        assert_eq!(versatz_pixel(1.0, 100), 0, "volle Breite ist wieder null");
        assert_eq!(versatz_pixel(0.999, 100), 0, "rundet auf volle Breite = wieder null");
        assert_eq!(versatz_pixel(f64::NAN, 100), 0);
        assert_eq!(versatz_pixel(0.5, 0), 0);
    }

    #[test]
    fn quell_spalte_wickelt_um() {
        assert_eq!(quell_spalte(50, 0, 100), 50, "ohne Versatz unveraendert");
        assert_eq!(quell_spalte(0, 1, 100), 99, "links heraus kommt rechts rein");
        assert_eq!(quell_spalte(0, 100, 100), 0, "volle Breite ist wieder null");
        assert_eq!(quell_spalte(-1, 0, 100), 99, "negative Ansichtsspalte wickelt um");
        assert_eq!(quell_spalte(-250, 0, 100), 50, "auch mehrere Breiten");
        assert_eq!(quell_spalte(10, 0, 0), 0, "unbrauchbare Breite");
    }
}
