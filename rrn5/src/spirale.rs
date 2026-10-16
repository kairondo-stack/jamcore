//! Spiral-Scan des Bildes: zweiter Abtastpfad neben dem Spalten-Sweep.
//!
//! Die Rechnung (Familie, `a`, φ, Punkt, Radius → Note) liegt in
//! `jamcore::spirale`; hier steht, was nur den Desktop betrifft: Modus und
//! Statuszeile, Schrittweite ÷/×, die Zelle am Spiralpunkt und die dünne Spur
//! im Sweep-Fenster (drittes Fenster, wie die Treffer des Linien-Sweeps).

use jamcore::livescan::{palette_match, pixel_matches, ScanFilter};
use jamcore::spirale::Spirale;
use std::collections::HashSet;

/// Scan-Modus des Bildes: Linien-Sweep oder Spirale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modus {
    Linie,
    Spirale,
}

/// Modus aus dem Zustand: ohne Spirale läuft der Linien-Sweep.
pub fn modus(spirale: &Option<Spirale>) -> Modus {
    if spirale.is_some() {
        Modus::Spirale
    } else {
        Modus::Linie
    }
}

/// Linie ⇄ Spirale umschalten und den neuen Modus liefern. Beim Anschalten
/// beginnt die Spirale im Zentrum mit Vorgabewerten (Familie Archimedes).
pub fn umschalten(spirale: &mut Option<Spirale>) -> Modus {
    *spirale = match spirale {
        Some(_) => None,
        None => Some(Spirale::neu(jamcore::spirale::Familie::Archimedes)),
    };
    modus(spirale)
}

/// Schrittweite halbieren (Alt+−).
pub fn schritt_teile(s: &mut Spirale) {
    s.setze_schritt(s.schritt / 2.0);
}

/// Schrittweite verdoppeln (Alt++).
pub fn schritt_vielfache(s: &mut Spirale) {
    s.setze_schritt(s.schritt * 2.0);
}

/// Statuszeile: „Scan: Linie" bzw. „Scan: Spirale (Archimedes, Schritt 2,0)".
pub fn text(spirale: &Option<Spirale>) -> String {
    match spirale {
        None => "Scan: Linie".to_string(),
        Some(s) => format!(
            "Scan: Spirale ({}, Schritt {})",
            familie_name(s.familie),
            zahl(s.schritt)
        ),
    }
}

/// Name der Familie wie im Patch („arch ln" für die Logarithmus-Familie).
pub fn familie_name(familie: jamcore::spirale::Familie) -> &'static str {
    match familie {
        jamcore::spirale::Familie::Archimedes => "Archimedes",
        jamcore::spirale::Familie::Log => "arch ln",
    }
}

/// Zahl mit einer Nachkommastelle und Komma (die Statuszeile ist deutsch).
fn zahl(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

/// Größter Radius im Bild: Abstand von der Mitte zur Ecke (Patch: `sqrtmax`).
pub fn radius_max(w: i32, h: i32) -> f64 {
    let w = w.max(0) as f64;
    let h = h.max(0) as f64;
    ((w / 2.0).powi(2) + (h / 2.0).powi(2)).sqrt()
}

/// Die Bildzelle am Spiralpunkt, wenn sie den Filter trifft (Palette wie der
/// Linien-Sweep, sonst die Einzelfarbe). `None` = keine Note.
pub fn zelle_trifft(
    frame: &[u8],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    filter: &ScanFilter,
    palette: Option<&HashSet<u32>>,
) -> Option<(u8, u8, u8)> {
    if w <= 0 || h <= 0 || x < 0 || x >= w || y < 0 || y >= h {
        return None;
    }
    let idx = ((y * w + x) * 4) as usize;
    if idx + 3 >= frame.len() {
        return None;
    }
    let (r, g, b) = (frame[idx], frame[idx + 1], frame[idx + 2]);
    let treffer = match palette {
        Some(p) => palette_match(r, g, b, p),
        None => pixel_matches(r, g, b, filter),
    };
    treffer.then_some((r, g, b))
}

/// Dünne Linie (Bresenham) zwischen zwei Bildpunkten — die Spur des
/// Spiral-Scans im Sweep-Fenster. Enthält beide Endpunkte.
pub fn linie(von: (i32, i32), bis: (i32, i32)) -> Vec<(i32, i32)> {
    let (x0, y0) = von;
    let (x1, y1) = bis;
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut punkte = Vec::with_capacity((dx.max(dy) + 1) as usize);
    let (mut x, mut y) = (x0, y0);
    let mut fehler = dx - dy;
    loop {
        punkte.push((x, y));
        if (x, y) == (x1, y1) {
            break;
        }
        let e2 = 2 * fehler;
        if e2 > -dy {
            fehler -= dy;
            x += sx;
        }
        if e2 < dx {
            fehler += dx;
            y += sy;
        }
    }
    punkte
}

#[cfg(test)]
mod tests {
    use super::*;
    use jamcore::spirale::{Familie, SCHRITT_MIN};

    fn filter() -> ScanFilter {
        ScanFilter {
            bg: (0, 0, 0),
            sel: (200, 40, 120),
            range: (10, 10, 10),
        }
    }

    /// RGBA-Puffer aus (r,g,b)-Pixeln.
    fn frame(pixel: &[(u8, u8, u8)]) -> Vec<u8> {
        let mut out = Vec::new();
        for &(r, g, b) in pixel {
            out.extend_from_slice(&[r, g, b, 255]);
        }
        out
    }

    #[test]
    fn umschalten_wechselt_den_modus() {
        let mut spirale: Option<Spirale> = None;
        assert_eq!(modus(&spirale), Modus::Linie);
        assert_eq!(umschalten(&mut spirale), Modus::Spirale, "an");
        assert!(spirale.is_some(), "Zustand traegt die Spirale");
        assert_eq!(modus(&spirale), Modus::Spirale);
        assert_eq!(umschalten(&mut spirale), Modus::Linie, "wieder aus");
        assert!(spirale.is_none(), "Zustand leer = Linien-Sweep");
    }

    #[test]
    fn statuszeile_zeigt_familie_und_schritt() {
        assert_eq!(text(&None), "Scan: Linie");
        let mut spirale = Some(Spirale::neu(Familie::Archimedes));
        assert_eq!(text(&spirale), "Scan: Spirale (Archimedes, Schritt 2,0)");
        {
            let s = spirale.as_mut().unwrap();
            schritt_teile(s);
        }
        assert_eq!(text(&spirale), "Scan: Spirale (Archimedes, Schritt 1,0)");
        {
            let s = spirale.as_mut().unwrap();
            schritt_vielfache(s);
            schritt_vielfache(s);
        }
        assert_eq!(text(&spirale), "Scan: Spirale (Archimedes, Schritt 4,0)");
        // Die zweite Familie heisst wie im Patch „arch ln".
        let log = Some(Spirale::neu(Familie::Log));
        assert_eq!(text(&log), "Scan: Spirale (arch ln, Schritt 2,0)");
    }

    #[test]
    fn schritt_tasten_bleiben_im_geklemmten_band() {
        let mut s = Spirale::neu(Familie::Archimedes);
        for _ in 0..40 {
            schritt_teile(&mut s);
        }
        assert_eq!(s.schritt, SCHRITT_MIN, "viele Male halbieren klemmt unten");
        for _ in 0..40 {
            schritt_vielfache(&mut s);
        }
        assert_eq!(s.schritt, jamcore::spirale::SCHRITT_MAX, "oben ebenso");
    }

    #[test]
    fn radius_max_ist_die_halbe_diagonale() {
        assert_eq!(radius_max(640, 480), 400.0, "3-4-5: sqrt(320²+240²)");
        assert_eq!(radius_max(320, 240), 200.0);
        assert_eq!(radius_max(0, 0), 0.0);
        assert_eq!(radius_max(-4, 10), 5.0, "negative Breite zaehlt als 0");
    }

    #[test]
    fn zelle_trifft_liest_genau_den_punkt() {
        // 2x2: Hintergrund, Treffer, Marker, Treffer.
        let f = filter();
        let bild = frame(&[
            (0, 0, 0),
            (200, 40, 120),
            (255, 0, 0),
            (205, 45, 115),
        ]);
        assert_eq!(
            zelle_trifft(&bild, 2, 2, 1, 0, &f, None),
            Some((200, 40, 120)),
            "Treffer in der Auswahlfarbe"
        );
        assert_eq!(
            zelle_trifft(&bild, 2, 2, 1, 1, &f, None),
            Some((205, 45, 115)),
            "innerhalb der Toleranz"
        );
        assert_eq!(zelle_trifft(&bild, 2, 2, 0, 0, &f, None), None, "Hintergrund");
        assert_eq!(zelle_trifft(&bild, 2, 2, 0, 1, &f, None), None, "Marker");
        assert_eq!(zelle_trifft(&bild, 2, 2, 2, 0, &f, None), None, "ausserhalb");
        assert_eq!(zelle_trifft(&bild, 2, 2, 1, -1, &f, None), None, "ausserhalb");
        assert_eq!(zelle_trifft(&[], 2, 2, 1, 0, &f, None), None, "leerer Puffer");
        assert_eq!(zelle_trifft(&bild, 0, 0, 0, 0, &f, None), None, "kein Bild");
        // Mit Palette zaehlt die gemerkte Farbe statt der Einzelauswahl.
        let mut pal = HashSet::new();
        pal.insert(jamcore::livescan::quantize(10, 20, 30));
        let gemalt = frame(&[(10, 20, 30), (0, 0, 0), (0, 0, 0), (0, 0, 0)]);
        assert_eq!(
            zelle_trifft(&gemalt, 2, 2, 0, 0, &f, Some(&pal)),
            Some((10, 20, 30)),
            "Palette trifft"
        );
        assert_eq!(
            zelle_trifft(&gemalt, 2, 2, 1, 0, &f, Some(&pal)),
            None,
            "Hintergrund trifft auch mit Palette nicht"
        );
    }

    #[test]
    fn linie_verbindet_die_punkte() {
        assert_eq!(linie((0, 0), (3, 0)), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
        assert_eq!(linie((5, 5), (5, 5)), vec![(5, 5)], "ein Punkt bleibt einer");
        let d = linie((0, 0), (2, 2));
        assert_eq!(d.first(), Some(&(0, 0)));
        assert_eq!(d.last(), Some(&(2, 2)));
        assert_eq!(d, vec![(0, 0), (1, 1), (2, 2)]);
        // Auch rueckwaerts und steil: Endpunkte bleiben, keine Luecke groesser 1.
        let s = linie((10, 0), (0, 3));
        assert_eq!(s.first(), Some(&(10, 0)));
        assert_eq!(s.last(), Some(&(0, 3)));
        assert!(
            s.windows(2).all(|w| {
                (w[0].0 - w[1].0).abs() <= 1 && (w[0].1 - w[1].1).abs() <= 1
            }),
            "zusammenhaengende Spur: {s:?}"
        );
    }
}
