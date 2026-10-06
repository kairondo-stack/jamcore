//! Transformationen für das Pattern-Raster (Desktop-Recorder-Funktionen,
//! die am Handy noch fehlten). Alles hardwarefrei und getestet.

use crate::grid::{Note, Pattern};

/// Rundet jeden Schritt auf das Raster (`raster >= 1`); Noten auf demselben
/// Zielschritt werden zusammengelegt (die letzte gewinnt).
pub fn quantisieren(noten: &[Note], raster: u16) -> Vec<Note> {
    let raster = raster.max(1);
    let mut out: Vec<Note> = Vec::new();
    for note in noten {
        let ziel = ((note.step + raster / 2) / raster) * raster;
        let ziel = ziel.min(crate::grid::STEPS_PER_CYCLE - 1);
        let neu = Note {
            step: ziel,
            ..*note
        };
        out.retain(|n| n.step != ziel);
        out.push(neu);
    }
    out.sort_by_key(|n| (n.step, n.pitch));
    out
}

/// Spiegelt die Schritte im Zyklus (Schritt 0 wird zum letzten).
pub fn umkehren(noten: &[Note]) -> Vec<Note> {
    let mut out: Vec<Note> = noten
        .iter()
        .map(|note| Note {
            step: crate::grid::STEPS_PER_CYCLE - 1 - note.step,
            ..*note
        })
        .collect();
    out.sort_by_key(|n| (n.step, n.pitch));
    out
}

/// Verschiebt alle Töne um `halbtoene` (Ergebnis bleibt in 0..=127).
pub fn transponieren(noten: &[Note], halbtoene: i32) -> Vec<Note> {
    noten
        .iter()
        .map(|note| Note {
            pitch: (note.pitch + halbtoene).clamp(0, 127),
            ..*note
        })
        .collect()
}

/// Bildet den Tonhöhenbereich auf einen Zielbereich ab (linear, gerundet).
/// Gleiche Töne bleiben gleich; ein Vorzeichen dreht die Richtung.
pub fn skalieren(noten: &[Note], quelle: (i32, i32), ziel: (i32, i32)) -> Vec<Note> {
    let (q_min, q_max) = (quelle.0.min(quelle.1), quelle.0.max(quelle.1));
    let spanne = (q_max - q_min).max(1) as f32;
    noten
        .iter()
        .map(|note| {
            let anteil = (note.pitch - q_min).clamp(0, q_max - q_min) as f32 / spanne;
            let ton = ziel.0 as f32 + anteil * (ziel.1 - ziel.0) as f32;
            Note {
                pitch: (ton.round() as i32).clamp(0, 127),
                ..*note
            }
        })
        .collect()
}

/// Setzt Glide für alle Noten (oder löscht es).
pub fn glide_setzen(noten: &[Note], an: bool) -> Vec<Note> {
    noten
        .iter()
        .map(|note| Note { glide: an, ..*note })
        .collect()
}

/// Setzt das Instrument für alle Noten (0..=255).
pub fn instrument_setzen(noten: &[Note], instrument: u8) -> Vec<Note> {
    noten
        .iter()
        .map(|note| Note {
            instrument,
            ..*note
        })
        .collect()
}

/// Tempo aus Tipp-Zeitpunkten (Sekunden): mittleres Intervall als BPM.
/// Weniger als zwei Tipps oder unbrauchbare Abstände ergeben `None`.
pub fn bpm_aus_taps(taps: &[f64]) -> Option<f32> {
    if taps.len() < 2 {
        return None;
    }
    let mut abstaende: Vec<f64> = taps.windows(2).map(|p| p[1] - p[0]).collect();
    abstaende.retain(|d| d.is_finite() && *d > 0.05 && *d < 2.0);
    if abstaende.is_empty() {
        return None;
    }
    let mittel = abstaende.iter().sum::<f64>() / abstaende.len() as f64;
    let bpm = 60.0 / mittel;
    (40.0..=300.0).contains(&bpm).then_some(bpm as f32)
}

/// Wendet eine Transformation auf das ganze Pattern an.
pub fn pattern_transponieren(pattern: &Pattern, halbtoene: i32) -> Pattern {
    aus_noten(transponieren(pattern.notes(), halbtoene))
}

fn aus_noten(noten: Vec<Note>) -> Pattern {
    let mut p = Pattern::new();
    for note in noten {
        p.insert(note);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(step: u16, pitch: i32) -> Note {
        Note {
            step,
            instrument: 0,
            pitch,
            glide: false,
        }
    }

    #[test]
    fn quantisieren_rundet_aufs_raster() {
        let q = quantisieren(&[n(0, 60), n(3, 61), n(5, 62), n(6, 63)], 4);
        let steps: Vec<u16> = q.iter().map(|x| x.step).collect();
        // Schritt 4 bekam zwei Noten und wurde zusammengelegt.
        assert_eq!(steps, vec![0, 4, 8]);
        assert_eq!(q[1].pitch, 62, "die letzte auf dem Rasterschritt gewinnt");
    }

    #[test]
    fn quantisieren_mit_raster_eins_aendert_nichts() {
        let vorher = [n(0, 60), n(7, 62)];
        assert_eq!(quantisieren(&vorher, 1), vorher.to_vec());
    }

    #[test]
    fn umkehren_spiegelt_die_schritte() {
        let u = umkehren(&[n(0, 60), n(3, 62)]);
        // Sortiert nach Schritt: 12 (ehemals 3) kommt vor 15 (ehemals 0).
        assert_eq!(u[0].step, 12);
        assert_eq!(u[1].step, 15);
        assert_eq!(u[1].pitch, 60, "die Note von Schritt 0 liegt jetzt hinten");
    }

    #[test]
    fn transponieren_verschiebt_und_klemmt() {
        let t = transponieren(&[n(0, 60), n(1, 120)], 12);
        assert_eq!(t[0].pitch, 72);
        assert_eq!(t[1].pitch, 127);
        let runter = transponieren(&[n(0, 5)], -12);
        assert_eq!(runter[0].pitch, 0);
    }

    #[test]
    fn skalieren_bildet_den_bereich_ab() {
        let s = skalieren(&[n(0, 36), n(1, 48), n(2, 60)], (36, 60), (48, 72));
        assert_eq!(s[0].pitch, 48);
        assert_eq!(s[1].pitch, 60);
        assert_eq!(s[2].pitch, 72);
    }

    #[test]
    fn skalieren_mit_einem_ton_bleibt_stabil() {
        let s = skalieren(&[n(0, 60)], (60, 60), (48, 72));
        assert_eq!(s[0].pitch, 48, "einzelner Ton landet am Zielanfang");
    }

    #[test]
    fn glide_und_instrument_setzen_alles() {
        let g = glide_setzen(&[n(0, 60), n(1, 61)], true);
        assert!(g.iter().all(|x| x.glide));
        let i = instrument_setzen(&g, 3);
        assert!(i.iter().all(|x| x.instrument == 3));
    }

    #[test]
    fn bpm_aus_taps_rechnet_das_mittel() {
        // 0.5 s Abstand = 120 BPM.
        let bpm = bpm_aus_taps(&[0.0, 0.5, 1.0, 1.5]).unwrap();
        assert!((bpm - 120.0).abs() < 0.5, "war {bpm}");
        // Ein Ausreisser wird verworfen.
        let bpm = bpm_aus_taps(&[0.0, 0.5, 1.0, 1.4]).unwrap();
        assert!((bpm - 120.0).abs() < 15.0, "war {bpm}");
    }

    #[test]
    fn bpm_aus_taps_lehnt_unsinn_ab() {
        assert!(bpm_aus_taps(&[]).is_none());
        assert!(bpm_aus_taps(&[1.0]).is_none());
        assert!(bpm_aus_taps(&[0.0, 0.0, 0.0]).is_none(), "kein Abstand");
        assert!(bpm_aus_taps(&[0.0, 100.0]).is_none(), "zu langsam");
    }

    #[test]
    fn pattern_transponieren_bleibt_ein_pattern() {
        let mut p = Pattern::new();
        p.insert(n(0, 60));
        p.insert(n(4, 64));
        let t = pattern_transponieren(&p, -12);
        assert_eq!(t.notes().len(), 2);
        assert_eq!(t.notes()[0].pitch, 48);
    }
}

/// Übliche Tonleitern als Halbtonstufen (Grundton = 0).
pub const MOLL: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
pub const DUR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
pub const PENTATONIK_MOLL: [i32; 5] = [0, 3, 5, 7, 10];
pub const PENTATONIK_DUR: [i32; 5] = [0, 2, 4, 7, 9];

/// Alle Töne einer Tonleiter über `oktaven` Oktaven ab `grundton`.
pub fn skala_toene(grundton: i32, stufen: &[i32], oktaven: u8) -> Vec<i32> {
    let mut out = Vec::new();
    for o in 0..oktaven.max(1) as i32 {
        for stufe in stufen {
            let ton = grundton + o * 12 + stufe;
            if (0..=127).contains(&ton) {
                out.push(ton);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Nächster erlaubter Ton; bei gleichem Abstand gewinnt der tiefere.
/// Leere Skala lässt den Ton unverändert.
pub fn auf_skala(pitch: i32, erlaubt: &[i32]) -> i32 {
    let Some(erster) = erlaubt.first() else {
        return pitch;
    };
    let mut best = *erster;
    let mut abstand = (pitch - best).abs();
    for ton in &erlaubt[1..] {
        let d = (pitch - ton).abs();
        if d < abstand {
            best = *ton;
            abstand = d;
        }
    }
    best
}

#[cfg(test)]
mod tests_skala {
    use super::*;

    #[test]
    fn skala_ueber_zwei_oktaven() {
        let toene = skala_toene(60, &MOLL, 2);
        assert_eq!(
            toene,
            vec![60, 62, 63, 65, 67, 68, 70, 72, 74, 75, 77, 79, 80, 82]
        );
    }

    #[test]
    fn skala_klemmt_an_den_grenzen() {
        let toene = skala_toene(120, &DUR, 2);
        assert!(toene.iter().all(|t| (0..=127).contains(t)));
        assert!(toene.contains(&120));
        assert!(!toene.contains(&131));
    }

    #[test]
    fn auf_skala_rundet_auf_den_naechsten_ton() {
        let toene = skala_toene(60, &MOLL, 2);
        assert_eq!(
            auf_skala(61, &toene),
            60,
            "Halbton drueber wird zum Grundton"
        );
        assert_eq!(auf_skala(64, &toene), 63, "bei Gleichstand der tiefere");
        assert_eq!(auf_skala(66, &toene), 65, "unter dem Ton");
        assert_eq!(auf_skala(60, &toene), 60, "erlaubter Ton bleibt");
    }

    #[test]
    fn leere_skala_laesst_alles() {
        assert_eq!(auf_skala(61, &[]), 61);
    }

    #[test]
    fn pentatonik_hat_fuenf_stufen() {
        assert_eq!(skala_toene(60, &PENTATONIK_MOLL, 1).len(), 5);
        assert_eq!(skala_toene(60, &PENTATONIK_DUR, 1).len(), 5);
    }
}
