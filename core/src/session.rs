//! Session-Logik für die Ableton-Link-Anbindung (S9).
//!
//! Hardwarefrei: Die App hält die Link-Session (`rusty_link`), hier stehen die
//! Rechenwege, die man ohne Netz prüfen kann — Quantisierung auf den nächsten
//! Beat und das Folgen des Session-Tempos.

/// Zustand, den die Oberfläche anzeigt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionZustand {
    /// Tempo der Session in BPM.
    pub bpm: f64,
    /// Aktuelle Beat-Position (ganzzahlig = Taktlinie bei Quantum 4).
    pub beat: f64,
    /// Anzahl der Peers (ohne uns).
    pub peers: usize,
    /// Läuft die Session gerade (Transport)?
    pub playing: bool,
}

impl Default for SessionZustand {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beat: 0.0,
            peers: 0,
            playing: false,
        }
    }
}

/// Nächster Beat-Rasterpunkt (Vielfaches von `quantum`) ab `beat`.
/// Ist `beat` genau auf dem Raster, bleibt er stehen.
pub fn naechster_beat(beat: f64, quantum: f64) -> f64 {
    let quantum = if quantum.is_finite() && quantum > 0.0 {
        quantum
    } else {
        1.0
    };
    if !beat.is_finite() {
        return 0.0;
    }
    let raster = (beat / quantum).ceil() * quantum;
    if (raster - beat).abs() < 1e-9 {
        beat
    } else {
        raster
    }
}

/// Wartezeit bis zum nächsten Beat in Sekunden (0.0, wenn schon drauf).
pub fn wartezeit_bis_beat(beat: f64, quantum: f64, bpm: f64) -> f64 {
    let bpm = if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        120.0
    };
    let delta = naechster_beat(beat, quantum) - beat;
    (delta.max(0.0)) * 60.0 / bpm
}

/// Tempo, mit dem weitergerechnet wird: Session-Tempo, wenn gefolgt wird.
pub fn tempo_folgen(lokal: f64, session: f64, folgen: bool) -> f64 {
    if folgen && session.is_finite() && session > 0.0 {
        session
    } else {
        lokal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naechster_beat_trifft_das_raster() {
        assert_eq!(naechster_beat(2.3, 4.0), 4.0);
        assert_eq!(naechster_beat(5.1, 4.0), 8.0);
        assert_eq!(naechster_beat(2.3, 1.0), 3.0);
        assert_eq!(naechster_beat(2.3, 0.5), 2.5);
    }

    #[test]
    fn naechster_beat_laesst_das_raster_stehen() {
        assert_eq!(naechster_beat(4.0, 4.0), 4.0);
        assert_eq!(naechster_beat(0.0, 4.0), 0.0);
    }

    #[test]
    fn naechster_beat_verkraftet_unsinn() {
        assert_eq!(naechster_beat(-0.5, 4.0), 0.0);
        assert_eq!(naechster_beat(2.3, 0.0), 3.0, "Quantum 0 wird zu 1");
        assert_eq!(naechster_beat(f64::NAN, 4.0), 0.0);
    }

    #[test]
    fn wartezeit_rechnet_in_sekunden() {
        // Bei 120 BPM dauert ein Beat 0.5 s; von 3.6 auf 4.0 sind es 0.4 Beats.
        let w = wartezeit_bis_beat(3.6, 4.0, 120.0);
        assert!((w - 0.2).abs() < 1e-9, "war {w}");
        assert_eq!(wartezeit_bis_beat(4.0, 4.0, 120.0), 0.0);
    }

    #[test]
    fn tempo_folgen_verhaelt_sich_wie_erwartet() {
        assert_eq!(tempo_folgen(120.0, 100.0, true), 100.0);
        assert_eq!(tempo_folgen(120.0, 100.0, false), 120.0);
        assert_eq!(
            tempo_folgen(120.0, f64::NAN, true),
            120.0,
            "kaputtes Session-Tempo laesst lokal stehen"
        );
        assert_eq!(tempo_folgen(120.0, 0.0, true), 120.0);
    }

    #[test]
    fn zustand_ist_vorbelegt() {
        let z = SessionZustand::default();
        assert_eq!(z.bpm, 120.0);
        assert_eq!(z.peers, 0);
        assert!(!z.playing);
    }
}
