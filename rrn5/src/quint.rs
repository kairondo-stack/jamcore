//! Interaktiver Quintenzirkel: reine Geometrie- und Auswahlhelfer.

/// Quintenzirkel in Notenwerten (C, G, D, A, E, B, F#, C#, G#, D#, A#, F).
pub const CIRCLE: [i32; 12] = [60, 67, 62, 69, 64, 71, 66, 61, 68, 63, 70, 65];

/// Winkelbereich (Start, Ende in Radiant) eines Segments; 0 rad = 3 Uhr,
/// Winkel wächst im Uhrzeigersinn (Bildschirmkoordinaten).
pub fn segment_angles(index: usize) -> (f32, f32) {
    let sector = 2.0 * std::f32::consts::PI / 12.0;
    let mid = index as f32 * sector;
    (mid - sector / 2.0, mid + sector / 2.0)
}

/// Segment unter einem Punkt (relativ zum Kreismittelpunkt) treffen.
pub fn segment_index_at(x: f32, y: f32, r_inner: f32, r_outer: f32) -> Option<usize> {
    let r = (x * x + y * y).sqrt();
    if r < r_inner || r > r_outer {
        return None;
    }
    let sector = 2.0 * std::f32::consts::PI / 12.0;
    let a = y.atan2(x);
    let idx = ((a + sector / 2.0) / sector).floor() as i32;
    Some(idx.rem_euclid(12) as usize)
}

/// Notenklassen (0..11) der ausgewählten Segmente, aufsteigend, ohne Duplikate.
pub fn note_classes(selected: &[usize]) -> Vec<i32> {
    let mut v: Vec<i32> = selected
        .iter()
        .map(|&i| CIRCLE[i % 12] % 12)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn segment_angles_cover_full_circle() {
        let (s0, e0) = segment_angles(0);
        assert!((s0 + PI / 12.0).abs() < 1e-4, "Start: {s0}");
        assert!((e0 - PI / 12.0).abs() < 1e-4, "Ende: {e0}");
        let (s11, e11) = segment_angles(11);
        assert!(s11 < 2.0 * PI && e11 > s11);
    }

    #[test]
    fn hit_test_finds_segment_by_angle_and_radius() {
        // rechts (3 Uhr) = Segment 0-Mitte
        assert_eq!(segment_index_at(150.0, 0.0, 50.0, 200.0), Some(0));
        // unten (6 Uhr) = Segment 3
        assert_eq!(segment_index_at(0.0, 150.0, 50.0, 200.0), Some(3));
        // links (9 Uhr) = Segment 6
        assert_eq!(segment_index_at(-150.0, 0.0, 50.0, 200.0), Some(6));
        // oben (12 Uhr) = Segment 9
        assert_eq!(segment_index_at(0.0, -150.0, 50.0, 200.0), Some(9));
        // zu nah am Zentrum / außerhalb
        assert_eq!(segment_index_at(10.0, 0.0, 50.0, 200.0), None);
        assert_eq!(segment_index_at(300.0, 0.0, 50.0, 200.0), None);
    }

    #[test]
    fn note_classes_dedup_and_sort() {
        assert_eq!(note_classes(&[0]), vec![0]); // C
        assert_eq!(note_classes(&[1, 0]), vec![0, 7]); // G(7) und C(0)
        assert_eq!(note_classes(&[2, 2]), vec![2]); // D 62%12=2
        assert!(note_classes(&[]).is_empty());
    }
}
