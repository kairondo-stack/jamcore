//! Live-Scan für Kamera/Video: Frame-Spalten filtern und als Noten-Events
//! abspielen (Filter wie Taste `l`, Abspielverhalten wie Taste `o`).

use crate::is_color_in_range;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct ScanFilter {
    /// Hintergrundfarbe (wird übersprungen).
    pub bg: (u8, u8, u8),
    /// gewählte Auswahlfarbe (Linksklick).
    pub sel: (u8, u8, u8),
    /// Toleranzen um die Auswahlfarbe.
    pub range: (i32, i32, i32),
}

/// Pixel für den Scan brauchbar? (Wie `l`: Hintergrund und (255,0,0) aus,
/// Farbbereich um die Auswahlfarbe.)
pub fn pixel_matches(r: u8, g: u8, b: u8, f: &ScanFilter) -> bool {
    if (r, g, b) == f.bg {
        return false;
    }
    if r == 255 && g == 0 && b == 0 {
        return false;
    }
    is_color_in_range(
        r,
        g,
        b,
        f.sel.0,
        f.sel.1,
        f.sel.2,
        f.range.0,
        f.range.1,
        f.range.2,
    )
}

/// Scannt Spalte `x` im RGBA-Frame; Rückgabe (y, r, g, b) je Treffer.
/// Mit `palette` wird gegen die gemalten Farben (2. Fenster) gematcht,
/// sonst gegen die Einzelfarbe im Filter.
pub fn scan_column(
    frame: &[u8],
    w: i32,
    h: i32,
    x: i32,
    f: &ScanFilter,
    palette: Option<&std::collections::HashSet<u32>>,
) -> Vec<(i32, u8, u8, u8)> {
    let mut out = Vec::new();
    if w <= 0 || h <= 0 || x < 0 || x >= w {
        return out;
    }
    for y in 0..h {
        let idx = ((y * w + x) * 4) as usize;
        if idx + 3 >= frame.len() {
            break;
        }
        let (r, g, b) = (frame[idx], frame[idx + 1], frame[idx + 2]);
        let hit = match palette {
            Some(p) => palette_match(r, g, b, p),
            None => pixel_matches(r, g, b, f),
        };
        if hit {
            out.push((y, r, g, b));
        }
    }
    out
}

/// Farbquantisierung für die Palette (5 Bit je Kanal, Toleranz ±7).
pub fn quantize(r: u8, g: u8, b: u8) -> u32 {
    (((r as u32) >> 3) << 10) | (((g as u32) >> 3) << 5) | ((b as u32) >> 3)
}

/// Sammelt die im Puffer gemalten Farben (ohne Hintergrund/Schwarz-Marker).
pub fn build_palette(
    buf: &[u8],
    pixels: usize,
    bg: (u8, u8, u8),
) -> std::collections::HashSet<u32> {
    let bgq = quantize(bg.0, bg.1, bg.2);
    let mut set = std::collections::HashSet::new();
    for i in 0..pixels {
        let idx = i * 4;
        if idx + 2 >= buf.len() {
            break;
        }
        let (r, g, b) = (buf[idx], buf[idx + 1], buf[idx + 2]);
        if r == 255 && g == 0 && b == 0 {
            continue;
        }
        let q = quantize(r, g, b);
        if q != bgq {
            set.insert(q);
        }
    }
    set
}

/// Trifft eine Farbe die Palette? Geprüft werden die eigene und die
/// Nachbar-Quantisierungsstufen (±1, entspricht ca. ±8 je Kanal), damit
/// Farben an Bucket-Grenzen nicht durchfallen.
pub fn palette_match(r: u8, g: u8, b: u8, palette: &std::collections::HashSet<u32>) -> bool {
    let qr = (r as i32) >> 3;
    let qg = (g as i32) >> 3;
    let qb = (b as i32) >> 3;
    for dr in -1..=1 {
        for dg in -1..=1 {
            for db in -1..=1 {
                let nr = qr + dr;
                let ng = qg + dg;
                let nb = qb + db;
                if !(0..32).contains(&nr) || !(0..32).contains(&ng) || !(0..32).contains(&nb) {
                    continue;
                }
                let q = ((nr as u32) << 10) | ((ng as u32) << 5) | (nb as u32);
                if palette.contains(&q) {
                    return true;
                }
            }
        }
    }
    false
}

/// Nächste Spalte mit Schrittweite und Umlauf in [left, right).
pub fn next_column(x: i32, left: i32, right: i32, step: i32) -> i32 {
    let step = step.max(1);
    if right <= left {
        return left;
    }
    let next = x + step;
    if next >= right {
        left
    } else {
        next
    }
}

/// Notenbremse: erlaubt Senden, wenn genug Zeit seit der letzten Note vergangen
/// ist; `max_per_sec == 0` bedeutet ungebremst.
pub fn note_allowed(elapsed: Duration, max_per_sec: u32) -> bool {
    if max_per_sec == 0 {
        return true;
    }
    elapsed.as_secs_f64() >= 1.0 / max_per_sec as f64
}

/// Taktdauer pro Spalte: eine Spalte entspricht `h` Pixeln; damit dauert ein
/// voller Durchlauf (alle Spalten) genau einen Beat wie bei `o`.
pub fn column_interval_ns(h: i32, time_per_pixel_ns: i64) -> i64 {
    (h.max(1) as i64).saturating_mul(time_per_pixel_ns.max(1))
}

/// Takte-Maske: iialpha der Spalte (0.0 = Spalte wird übersprungen).
pub fn mask_alpha(alphas: &[f32], x: i32) -> f32 {
    alphas.get(x as usize).copied().unwrap_or(0.0)
}

/// Farbe mit der Masken-Amplitude skalieren.
pub fn scale_rgb(r: u8, g: u8, b: u8, alpha: f32) -> (u8, u8, u8) {
    let a = alpha.clamp(0.0, 1.0);
    (
        (r as f32 * a) as u8,
        (g as f32 * a) as u8,
        (b as f32 * a) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: i32, h: i32, pixels: &[(u8, u8, u8, u8)]) -> Vec<u8> {
        assert_eq!(pixels.len(), (w * h) as usize);
        let mut out = Vec::new();
        for &(r, g, b, a) in pixels {
            out.extend_from_slice(&[r, g, b, a]);
        }
        for v in out.len()..(w * h * 4) as usize {
            let _ = v;
        }
        out
    }

    fn filter() -> ScanFilter {
        ScanFilter {
            bg: (0, 0, 0),
            sel: (200, 40, 120),
            range: (10, 10, 10),
        }
    }

    #[test]
    fn pixel_filter_skips_background_black_and_red() {
        let f = filter();
        assert!(!pixel_matches(0, 0, 0, &f), "Hintergrund");
        assert!(!pixel_matches(255, 0, 0, &f), "Schwarz-Marker");
        assert!(pixel_matches(200, 40, 120, &f), "exakte Auswahl");
        assert!(pixel_matches(205, 45, 115, &f), "innerhalb Toleranz");
        assert!(!pixel_matches(230, 40, 120, &f), "außerhalb Toleranz");
    }

    #[test]
    fn scan_column_returns_matching_rows() {
        let w = 2;
        let h = 3;
        let px = [
            (200, 40, 120, 255), // y=0 Treffer
            (0, 0, 0, 255),      // y=0 x=1 Hintergrund
            (0, 0, 0, 255),      // y=1 x=0 Hintergrund
            (205, 45, 115, 255), // y=1 x=1 Treffer
            (255, 0, 0, 255),    // y=2 x=0 Marker
            (100, 100, 100, 255), // y=2 x=1 außerhalb
        ];
        let data = frame(w, h, &px);
        assert_eq!(
            scan_column(&data, w, h, 0, &filter(), None),
            vec![(0, 200, 40, 120)]
        );
        assert_eq!(
            scan_column(&data, w, h, 1, &filter(), None),
            vec![(1, 205, 45, 115)]
        );
        assert!(scan_column(&data, w, h, 2, &filter(), None).is_empty());
        assert!(scan_column(&[], w, h, 0, &filter(), None).is_empty());
    }

    #[test]
    fn palette_matches_painted_colors_with_tolerance() {
        // "Gemaltes" Bild: Hintergrund (0,0,0), gemalt (200,40,120)
        let buf = frame(2, 1, &[(0, 0, 0, 255), (200, 40, 120, 255)]);
        let pal = build_palette(&buf, 2, (0, 0, 0));
        assert_eq!(pal.len(), 1, "nur die gemalte Farbe");
        assert!(palette_match(200, 40, 120, &pal));
        assert!(palette_match(205, 45, 115, &pal), "innerhalb ±7");
        assert!(!palette_match(220, 40, 120, &pal), "außerhalb");
        assert!(!palette_match(0, 0, 0, &pal), "Hintergrund");

        let data = frame(2, 1, &[(205, 45, 115, 255), (10, 10, 10, 255)]);
        let hits = scan_column(&data, 2, 1, 1, &filter(), None);
        assert!(hits.is_empty(), "Einzelfarbe-Filter trifft hier nicht");
        let hits = scan_column(&data, 2, 1, 1, &filter(), Some(&pal));
        assert!(hits.is_empty(), "x=1 ist dunkel");
        let hits = scan_column(&data, 2, 1, 0, &filter(), Some(&pal));
        assert_eq!(hits, vec![(0, 205, 45, 115)], "Palette trifft ±7");
    }

    #[test]
    fn column_advance_wraps_and_respects_step() {
        assert_eq!(next_column(0, 0, 5, 1), 1);
        assert_eq!(next_column(4, 0, 5, 1), 0);
        assert_eq!(next_column(3, 0, 5, 2), 0);
        assert_eq!(next_column(0, 0, 5, 2), 2);
        assert_eq!(next_column(5, 0, 5, 1), 0);
        assert_eq!(next_column(0, 0, 0, 1), 0);
    }

    #[test]
    fn column_interval_matches_pixel_distance() {
        assert_eq!(column_interval_ns(240, 4_000), 960_000);
        assert_eq!(column_interval_ns(0, 0), 1);
    }

    #[test]
    fn mask_alpha_gates_columns() {
        let alphas = [0.0f32, 0.5, 1.0];
        assert_eq!(mask_alpha(&alphas, 0), 0.0);
        assert_eq!(mask_alpha(&alphas, 1), 0.5);
        assert_eq!(mask_alpha(&alphas, 2), 1.0);
        assert_eq!(mask_alpha(&alphas, 9), 0.0);
    }

    #[test]
    fn scale_rgb_applies_amplitude() {
        assert_eq!(scale_rgb(200, 40, 100, 0.0), (0, 0, 0));
        assert_eq!(scale_rgb(200, 40, 100, 0.5), (100, 20, 50));
        assert_eq!(scale_rgb(200, 40, 100, 1.0), (200, 40, 100));
        assert_eq!(scale_rgb(200, 40, 100, 2.0), (200, 40, 100));
    }

    #[test]
    fn note_limit_is_optional() {
        assert!(note_allowed(Duration::from_millis(1), 0));
        assert!(!note_allowed(Duration::from_millis(10), 20));
        assert!(note_allowed(Duration::from_millis(50), 20));
    }
}
