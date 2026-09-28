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
pub fn scan_column(
    frame: &[u8],
    w: i32,
    h: i32,
    x: i32,
    f: &ScanFilter,
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
        if pixel_matches(r, g, b, f) {
            out.push((y, r, g, b));
        }
    }
    out
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
        assert_eq!(scan_column(&data, w, h, 0, &filter()), vec![(0, 200, 40, 120)]);
        assert_eq!(
            scan_column(&data, w, h, 1, &filter()),
            vec![(1, 205, 45, 115)]
        );
        assert!(scan_column(&data, w, h, 2, &filter()).is_empty());
        assert!(scan_column(&[], w, h, 0, &filter()).is_empty());
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
    fn note_limit_is_optional() {
        assert!(note_allowed(Duration::from_millis(1), 0));
        assert!(!note_allowed(Duration::from_millis(10), 20));
        assert!(note_allowed(Duration::from_millis(50), 20));
    }
}
