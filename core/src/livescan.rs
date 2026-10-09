//! Live-Scan für Kamera/Video: Frame-Spalten filtern und als Noten-Events
//! abspielen (Filter wie Taste `l`, Abspielverhalten wie Taste `o`).

use std::time::Duration;

/// Farbbereich-Prüfung (wie `is_color_in_range` in rrn5).
fn is_color_in_range(
    r1: u8,
    g1: u8,
    b1: u8,
    r2: u8,
    g2: u8,
    b2: u8,
    rr: i32,
    gg: i32,
    bb: i32,
) -> bool {
    (r1 as i32 >= r2 as i32 - rr && r1 as i32 <= r2 as i32 + rr)
        && (g1 as i32 >= g2 as i32 - gg && g1 as i32 <= g2 as i32 + gg)
        && (b1 as i32 >= b2 as i32 - bb && b1 as i32 <= b2 as i32 + bb)
}

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
        r, g, b, f.sel.0, f.sel.1, f.sel.2, f.range.0, f.range.1, f.range.2,
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

/// Bildspalte für einen Sweep-Schritt in einem **Bereich**: `links` und
/// `rechts` sind Anteile (0.0 … 1.0) der Bildbreite — wie die Split-Grenzen
/// Alt+D/Alt+C im Desktop-rrn5. Schritt 0 liegt auf `links`, der letzte auf
/// `rechts`; unbrauchbare Grenzen bedeuten die volle Breite.
pub fn spalte_im_bereich(schritt: i32, schritte: i32, links: f32, rechts: f32, breite: i32) -> i32 {
    let breite = breite.max(1);
    // Beide Grenzen müssen brauchbar sein, sonst gilt die volle Breite.
    let (links, rechts) = if links.is_finite() && rechts.is_finite() && rechts > links {
        (links.clamp(0.0, 1.0), rechts.clamp(0.0, 1.0))
    } else {
        (0.0, 1.0)
    };
    let (links, rechts) = if rechts > links {
        (links, rechts)
    } else {
        (0.0, 1.0)
    };
    let schritte = schritte.max(1);
    let anteil = if schritte <= 1 {
        0.0
    } else {
        schritt.clamp(0, schritte - 1) as f32 / (schritte - 1) as f32
    };
    let x = links * breite as f32 + anteil * (rechts - links) * breite as f32;
    (x as i32).clamp(0, breite - 1)
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
            (200, 40, 120, 255),  // y=0 Treffer
            (0, 0, 0, 255),       // y=0 x=1 Hintergrund
            (0, 0, 0, 255),       // y=1 x=0 Hintergrund
            (205, 45, 115, 255),  // y=1 x=1 Treffer
            (255, 0, 0, 255),     // y=2 x=0 Marker
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
    fn bereich_bildet_schritte_auf_spalten_ab() {
        // 640 breit, Bereich 25 % … 75 % (Spalten 160 … 480).
        assert_eq!(spalte_im_bereich(0, 2, 0.25, 0.75, 640), 160);
        assert_eq!(spalte_im_bereich(1, 2, 0.25, 0.75, 640), 480);
        assert_eq!(
            spalte_im_bereich(0, 1, 0.25, 0.75, 640),
            160,
            "ein einziger Schritt liegt auf dem Anfang"
        );
        // Ohne Bereich die volle Breite, letzter Schritt bleibt im Bild.
        assert_eq!(spalte_im_bereich(0, 2, 0.0, 1.0, 640), 0);
        assert_eq!(spalte_im_bereich(1, 2, 0.0, 1.0, 640), 639);
        // Unsinnige Grenzen: volle Breite.
        assert_eq!(
            spalte_im_bereich(1, 2, 0.8, 0.2, 640),
            639,
            "rechts < links"
        );
        assert_eq!(spalte_im_bereich(1, 2, f32::NAN, 0.5, 640), 639, "NaN");
        // Schrittnummern werden geklemmt.
        assert_eq!(spalte_im_bereich(99, 2, 0.25, 0.75, 640), 480);
        assert_eq!(spalte_im_bereich(-3, 2, 0.25, 0.75, 640), 160);
    }

    #[test]
    fn bereich_verkraftet_randfaelle() {
        // Grenzen ausserhalb 0..1 werden geklemmt.
        assert_eq!(spalte_im_bereich(0, 2, -1.0, 1.5, 640), 0);
        assert_eq!(spalte_im_bereich(1, 2, -1.0, 1.5, 640), 639);
        // links == rechts ist kein Bereich -> volle Breite.
        assert_eq!(spalte_im_bereich(1, 2, 0.5, 0.5, 640), 639);
        // Keine Breite und keine Schritte duerfen nicht paniken.
        assert_eq!(spalte_im_bereich(0, 0, 0.25, 0.75, 0), 0);
        assert_eq!(spalte_im_bereich(5, -4, 0.25, 0.75, 1), 0);
        assert_eq!(spalte_im_bereich(5, 1, 0.0, 1.0, 100), 0, "ein Schritt");
        // Unendlich zaehlt wie NaN: volle Breite.
        assert_eq!(spalte_im_bereich(1, 2, f32::INFINITY, 0.5, 640), 639);
        assert_eq!(spalte_im_bereich(1, 2, 0.1, f32::NEG_INFINITY, 640), 639);
    }

    #[test]
    fn noten_modi_folgen_dem_desktop() {
        // Formeln wie `event_note_volume` im Desktop-rrn5.
        let (w, h, scale, add, x, y) = (100, 50, 120, 10, 25, 10);
        let y_steigend = (y * scale / h + add).clamp(0, 127); // 34
        let y_fallend = (scale - y * scale / h + add).clamp(0, 127); // 106
        let x_steigend = (x * scale / w + add).clamp(0, 127); // 40
        let x_fallend = (scale - x * scale / w + add).clamp(0, 127); // 100
        let vol_rechts_laut = x * 127 / w; // 31
        let vol_rechts_leise = 127 - x * 127 / w; // 96
        assert_eq!(
            note_volume(x, y, w, h, 1, scale, add),
            (y_fallend, 127),
            "Modus 1: Note aus der Zeile (wie der _-Zweig)"
        );
        assert_eq!(
            note_volume(x, y, w, h, 2, scale, add),
            (y_steigend, vol_rechts_leise)
        );
        assert_eq!(
            note_volume(x, y, w, h, 3, scale, add),
            (y_fallend, vol_rechts_laut)
        );
        assert_eq!(
            note_volume(x, y, w, h, 4, scale, add),
            (y_fallend, vol_rechts_leise)
        );
        assert_eq!(
            note_volume(x, y, w, h, 5, scale, add),
            (x_steigend, vol_rechts_laut)
        );
        assert_eq!(
            note_volume(x, y, w, h, 6, scale, add),
            (x_steigend, vol_rechts_leise),
            "Modus 6 steigt mit x"
        );
        assert_eq!(
            note_volume(x, y, w, h, 7, scale, add),
            (x_fallend, vol_rechts_laut),
            "Modus 7 faellt mit x"
        );
        assert_eq!(
            note_volume(x, y, w, h, 8, scale, add),
            (x_fallend, vol_rechts_leise),
            "Modus 8 faellt mit x"
        );
        // Unbekannte Modi fallen wie am Desktop auf den _-Zweig zurueck.
        assert_eq!(note_volume(x, y, w, h, 9, scale, add), (y_fallend, 127));
        assert_eq!(
            note_volume(x, y, w, h, 0, scale, add),
            (y_fallend, 127),
            "Modus 0 wird im Bild-Tab farbbasiert gerechnet"
        );
    }

    #[test]
    fn noten_modus_farbe_nimmt_den_farbanteil() {
        // `color_to_note_value_and_volume`: Rot/Gruen/Blau bestimmen Note und
        // Lautstaerke (die Testwerte liegen im gueltigen Bereich).
        assert_eq!(note_volume_farbe(200, 50, 30, 60, 0), (47, 46), "Rot");
        assert_eq!(note_volume_farbe(10, 200, 30, 60, 0), (68, 40), "Gruen");
        assert_eq!(note_volume_farbe(10, 20, 200, 60, 0), (89, 38), "Blau");
        assert_eq!(
            note_volume_farbe(200, 50, 30, 60, 12),
            (59, 46),
            "add wirkt als Grundton"
        );
        // Ergebnis bleibt im MIDI-Bereich.
        assert_eq!(note_volume_farbe(255, 255, 255, 120, 60), (127, 127));
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

/// Ton und Lautstärke eines Farbtreffers je Zuordnungsmodus
/// (Portierung von `event_note_volume` aus dem Desktop-rrn5):
///
/// - 0: nicht hier — im Bild-Tab farbbasiert über [`note_volume_farbe`]
/// - 1: Note aus der Zeile, fallend mit y; Lautstärke 127 (der `_`-Zweig)
/// - 2: Note steigt mit y, Lautstärke fällt nach rechts
/// - 3: Note fällt mit y, Lautstärke steigt nach rechts
/// - 4: Note fällt mit y, Lautstärke fällt nach rechts
/// - 5: Note steigt mit x, Lautstärke steigt nach rechts
/// - 6: Note steigt mit x, Lautstärke fällt nach rechts
/// - 7: Note fällt mit x, Lautstärke steigt nach rechts
/// - 8: Note fällt mit x, Lautstärke fällt nach rechts
///
/// `note_scale` ist der Tonumfang, `add` der Grundton. Anders als am Desktop
/// wird auf 0..=127 geklemmt (gültige MIDI-Werte).
pub fn note_volume(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    modus: u8,
    note_scale: i32,
    add: i32,
) -> (i32, i32) {
    let w = w.max(1);
    let h = h.max(1);
    let scale = note_scale.max(1);
    let aus_y = |steigend: bool| {
        let anteil = if steigend {
            y * scale / h
        } else {
            scale - y * scale / h
        };
        (anteil + add).clamp(0, 127)
    };
    let aus_x = |fallend: bool| {
        let anteil = if fallend {
            scale - x * scale / w
        } else {
            x * scale / w
        };
        (anteil + add).clamp(0, 127)
    };
    match modus {
        2 => (aus_y(true), (127 - x * 127 / w).clamp(0, 127)),
        3 => (aus_y(false), (x * 127 / w).clamp(0, 127)),
        4 => (aus_y(false), (127 - x * 127 / w).clamp(0, 127)),
        5 => (aus_x(false), (x * 127 / w).clamp(0, 127)),
        6 => (aus_x(false), (127 - x * 127 / w).clamp(0, 127)),
        7 => (aus_x(true), (x * 127 / w).clamp(0, 127)),
        8 => (aus_x(true), (127 - x * 127 / w).clamp(0, 127)),
        _ => (aus_y(false), 127),
    }
}

/// Modus 0 wie am Desktop (`color_to_note_value_and_volume`): die Farbe des
/// Treffers bestimmt Note und Lautstärke.
pub fn note_volume_farbe(r: u8, g: u8, b: u8, note_scale: i32, add: i32) -> (i32, i32) {
    let scale = note_scale.max(1);
    let lautstaerke = ((r as i32 + g as i32 + b as i32) / 6).clamp(0, 127);
    let note = if r > g && r > b {
        r as i32 * scale / 255 + add
    } else if g > r && g > b {
        g as i32 * scale / 255 + add + 43 * scale / 120
    } else {
        b as i32 * scale / 255 + add + 85 * scale / 120
    };
    (note.clamp(0, 127), lautstaerke)
}

#[cfg(test)]
mod tests_note_volume {
    use super::*;

    #[test]
    fn modus_null_ist_im_kern_der_zeilen_fallback() {
        // Modus 0 rechnet der Bild-Tab farbbasiert; der Kern faellt fuer 0 auf
        // den _-Zweig zurueck (Note aus der Zeile, Lautstaerke voll).
        assert_eq!(note_volume(0, 0, 100, 100, 0, 36, 36), (72, 127));
        assert_eq!(note_volume(50, 50, 100, 100, 0, 36, 36), (54, 127));
        assert_eq!(note_volume(0, 100, 100, 100, 0, 36, 36), (36, 127));
    }

    #[test]
    fn modus_zwei_und_drei_drehen_die_richtung() {
        // Modus 2: Note steigt mit y (unten hoch), Lautstaerke faellt nach rechts.
        assert_eq!(note_volume(0, 0, 100, 100, 2, 36, 36).0, 36);
        assert_eq!(note_volume(0, 100, 100, 100, 2, 36, 36).0, 72);
        assert_eq!(note_volume(0, 0, 100, 100, 2, 36, 36).1, 127);
        assert!(note_volume(100, 0, 100, 100, 2, 36, 36).1 < 10);
        // Modus 3: umgekehrt.
        assert_eq!(note_volume(0, 0, 100, 100, 3, 36, 36).0, 72);
        assert_eq!(note_volume(0, 100, 100, 100, 3, 36, 36).0, 36);
        assert_eq!(note_volume(100, 0, 100, 100, 3, 36, 36).1, 127);
    }

    #[test]
    fn modus_fuenf_und_sechs_nehmen_die_spalte() {
        // Beide steigen mit x; Modus 5 ist rechts laut, Modus 6 rechts leise.
        assert_eq!(note_volume(0, 0, 100, 100, 5, 36, 36).0, 36);
        assert_eq!(note_volume(100, 0, 100, 100, 5, 36, 36).0, 72);
        assert_eq!(note_volume(0, 0, 100, 100, 6, 36, 36).0, 36);
        assert_eq!(note_volume(100, 0, 100, 100, 6, 36, 36).0, 72);
        assert_eq!(note_volume(100, 0, 100, 100, 6, 36, 36).1, 0);
    }

    #[test]
    fn werte_bleiben_im_bereich() {
        for modus in 0..9u8 {
            let (n, v) = note_volume(999, 999, 1, 1, modus, 120, 100);
            assert!((0..=127).contains(&n), "Note {n}");
            assert!((0..=127).contains(&v), "Volume {v}");
        }
    }
}
