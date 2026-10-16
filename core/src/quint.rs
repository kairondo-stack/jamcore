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
    let mut v: Vec<i32> = selected.iter().map(|&i| CIRCLE[i % 12] % 12).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Ein gemerkter Akkord aus Kreis-Segmenten.
///
/// `stufen` sind Segmentindizes des Quintenzirkels (0..=11), **sortiert und
/// ohne Doppelte**; `name` ist der erkannte Akkordname oder `"?"`, wenn die
/// Zusammenstellung nicht in der (bewusst kleinen) Tabelle steht.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Akkord {
    pub stufen: Vec<u8>,
    pub name: String,
}

/// Eine Stufe im Akkord an- oder ausschalten (0..=11 geklemmt, sortiert,
/// ohne Doppelte). Grundlage für Akkorde, die aus Einzeltipps entstehen.
pub fn toggle_stufe(akkord: &mut Vec<u8>, stufe: u8) {
    let stufe = stufe.min(11);
    match akkord.binary_search(&stufe) {
        Ok(pos) => {
            akkord.remove(pos);
        }
        Err(pos) => akkord.insert(pos, stufe),
    }
}

/// Stufen auf einen Akkordnamen abbilden, soweit möglich.
///
/// Die Stufen werden zuerst normiert (0..=11 geklemmt, sortiert, ohne
/// Doppelte). Erkannt werden nur die Grundstellungen der kleinen Tabelle
/// (Halbton-Intervalle zur Grundstufe 0):
///
/// | Stufen          | Name  |
/// |-----------------|-------|
/// | `[0, 4, 7]`     | Dur   |
/// | `[0, 3, 7]`     | Moll  |
/// | `[0, 4, 7, 11]` | maj7  |
/// | `[0, 3, 7, 10]` | m7    |
/// | `[0, 4, 7, 10]` | 7     |
///
/// **Entscheidung:** unbekannte, aber nicht leere Zusammenstellungen ergeben
/// `Some(Akkord { name: "?" })` — die UI kann den gemerkten Akkord so immer
/// anzeigen; nur die leere Eingabe ergibt `None`. (Die vollständige Tabelle
/// mit 31 Namen aus `showchord`/`mollchords` folgt im Max-Patch-Block.)
pub fn akkord_aus_stufen(stufen: &[u8]) -> Option<Akkord> {
    let mut normiert: Vec<u8> = stufen.iter().map(|&s| s.min(11)).collect();
    normiert.sort_unstable();
    normiert.dedup();
    if normiert.is_empty() {
        return None;
    }
    let name = match normiert.as_slice() {
        [0, 4, 7] => "Dur",
        [0, 3, 7] => "Moll",
        [0, 4, 7, 11] => "maj7",
        [0, 3, 7, 10] => "m7",
        [0, 4, 7, 10] => "7",
        _ => "?",
    };
    Some(Akkord {
        stufen: normiert,
        name: name.to_string(),
    })
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

    #[test]
    fn toggle_stufe_sortiert() {
        let mut stufen = Vec::new();
        toggle_stufe(&mut stufen, 7);
        toggle_stufe(&mut stufen, 0);
        toggle_stufe(&mut stufen, 4);
        assert_eq!(stufen, vec![0, 4, 7], "sortiert, ohne Doppelte");
        // nochmal dieselbe Stufe schaltet sie wieder aus
        toggle_stufe(&mut stufen, 4);
        assert_eq!(stufen, vec![0, 7]);
        // zweimal dieselbe Stufe ergibt genau einen Eintrag
        toggle_stufe(&mut stufen, 0);
        toggle_stufe(&mut stufen, 0);
        assert_eq!(stufen, vec![0, 7]);
        // Werte über 11 werden auf 11 geklemmt
        toggle_stufe(&mut stufen, 250);
        assert_eq!(stufen, vec![0, 7, 11]);
    }

    #[test]
    fn leerer_akkord_ist_none() {
        assert!(akkord_aus_stufen(&[]).is_none());
    }

    #[test]
    fn dreiklang_dur() {
        let a = akkord_aus_stufen(&[7, 0, 4]).expect("Dur-Dreiklang");
        assert_eq!(a.name, "Dur");
        assert_eq!(a.stufen, vec![0, 4, 7], "normiert: sortiert, ohne Doppelte");
    }

    #[test]
    fn dreiklang_moll() {
        let a = akkord_aus_stufen(&[0, 3, 7]).expect("Moll-Dreiklang");
        assert_eq!(a.name, "Moll");
        assert_eq!(a.stufen, vec![0, 3, 7]);
    }

    #[test]
    fn septakkord() {
        let maj7 = akkord_aus_stufen(&[0, 4, 7, 11]).expect("maj7");
        assert_eq!(maj7.name, "maj7");
        let m7 = akkord_aus_stufen(&[0, 3, 7, 10]).expect("m7");
        assert_eq!(m7.name, "m7");
        let dom = akkord_aus_stufen(&[0, 4, 7, 10]).expect("7");
        assert_eq!(dom.name, "7");
    }

    #[test]
    fn unbekannte_stufen_ohne_namen() {
        // Unbekannte Zusammenstellungen werden trotzdem gemerkt, aber der
        // Name ist "?" (entschieden: Some statt None, damit die UI immer
        // einen gemerkten Akkord zeigen kann).
        let a = akkord_aus_stufen(&[5, 0, 1, 5]).expect("gemerkt");
        assert_eq!(a.name, "?");
        assert_eq!(a.stufen, vec![0, 1, 5], "auch hier normiert");
    }
}
