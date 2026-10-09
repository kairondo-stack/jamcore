//! Mehrfinger-Gesten für Bild und Maske.
//!
//! Auf Android kommt von egui **kein** `Event::Zoom`; winit reicht je Finger ein
//! `egui::Event::Touch` durch (`egui-winit/src/lib.rs:878`). Pinch und Schieben
//! rechnet deshalb dieses Modul selbst:
//!
//! - **zwei Finger**: Abstandsverhältnis zoomt, die Mitte verschiebt.
//! - **drei Finger**: nur verschieben (kein Zoom).
//! - **ein Finger**: nichts — der bleibt der Farbauswahl bzw. der Maske vorbehalten.
//!
//! Die Structs kommen ohne egui aus, damit `cargo test` sie ohne Fenster prüfen kann.

/// Punkt in egui-Koordinaten (Punkte, nicht Pixel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Punkt {
    pub x: f32,
    pub y: f32,
}

impl Punkt {
    pub fn mitte(a: Punkt, b: Punkt) -> Punkt {
        Punkt {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        }
    }

    pub fn abstand(a: Punkt, b: Punkt) -> f32 {
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }
}

/// Phase einer Berührung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    Zug,
    Ende,
}

/// Was sich durch eine Berührung geändert hat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geste {
    /// Zoomfaktor für **diese** Berührung (1.0 = nichts).
    pub zoom: f32,
    /// Weg der Fingergruppe in Punkten.
    pub pan: (f32, f32),
}

/// Zustand der Fingergruppe; merkt sich Abstand und Mitte des letzten Schritts.
#[derive(Debug, Default)]
pub struct Gesten {
    punkte: Vec<(u64, Punkt)>,
    abstand: f32,
    mitte: Option<Punkt>,
}

impl Gesten {
    pub fn new() -> Self {
        Self::default()
    }

    /// Anzahl der aktiven Finger.
    pub fn finger(&self) -> usize {
        self.punkte.len()
    }

    /// Berührung eines Fingers melden; liefert die Geste, falls sich daraus eine
    /// ergibt (Zoom nur ab zwei Fingern).
    pub fn aktualisiere(&mut self, id: u64, phase: Phase, pos: Punkt) -> Option<Geste> {
        match phase {
            Phase::Ende => {
                self.punkte.retain(|(finger, _)| *finger != id);
            }
            Phase::Start | Phase::Zug => {
                match self.punkte.iter_mut().find(|(finger, _)| *finger == id) {
                    Some((_, alt)) => *alt = pos,
                    None => self.punkte.push((id, pos)),
                }
            }
        }
        self.auswerten()
    }

    /// Schwerpunkt aller Finger — damit verschiebt sich die Gruppe auch, wenn
    /// sich nur der dritte Finger bewegt.
    fn schwerpunkt(&self) -> Punkt {
        let n = self.punkte.len().max(1) as f32;
        let (mut x, mut y) = (0.0, 0.0);
        for (_, p) in &self.punkte {
            x += p.x;
            y += p.y;
        }
        Punkt { x: x / n, y: y / n }
    }

    /// Weite der Gruppe: mittlerer Abstand zum Schwerpunkt.
    fn weite(&self, mitte: Punkt) -> f32 {
        if self.punkte.is_empty() {
            return 0.0;
        }
        self.punkte
            .iter()
            .map(|(_, p)| Punkt::abstand(*p, mitte))
            .sum::<f32>()
            / self.punkte.len() as f32
    }

    fn auswerten(&mut self) -> Option<Geste> {
        // Zu wenig oder zu viele Finger: keine Geste. Ab drei Fingern wird nur
        // verschoben, nie gezoomt.
        if self.punkte.len() < 2 {
            self.abstand = 0.0;
            self.mitte = None;
            return None;
        }
        let zwei_finger = self.punkte.len() == 2;
        let mitte = self.schwerpunkt();
        let abstand = self.weite(mitte);
        let alt_abstand = std::mem::replace(&mut self.abstand, abstand);
        let alt_mitte = self.mitte.replace(mitte)?;
        let pan = (mitte.x - alt_mitte.x, mitte.y - alt_mitte.y);
        let zoom = if zwei_finger && alt_abstand > 1.0 && abstand > 0.0 {
            abstand / alt_abstand
        } else {
            1.0
        };
        Some(Geste { zoom, pan })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Punkt {
        Punkt { x, y }
    }

    #[test]
    fn zwei_finger_auseinander_zoomt_rein() {
        let mut g = Gesten::new();
        assert_eq!(g.aktualisiere(1, Phase::Start, p(0.0, 0.0)), None);
        assert_eq!(g.aktualisiere(2, Phase::Start, p(10.0, 0.0)), None);
        let g2 = g.aktualisiere(2, Phase::Zug, p(20.0, 0.0)).unwrap();
        assert!(
            (g2.zoom - 2.0).abs() < 1e-6,
            "Abstand 10 -> 20 ist Faktor 2: {}",
            g2.zoom
        );
    }

    #[test]
    fn zwei_finger_zusammen_zoomt_raus() {
        let mut g = Gesten::new();
        g.aktualisiere(1, Phase::Start, p(0.0, 0.0));
        g.aktualisiere(2, Phase::Start, p(40.0, 0.0));
        let g2 = g.aktualisiere(2, Phase::Zug, p(20.0, 0.0)).unwrap();
        assert!((g2.zoom - 0.5).abs() < 1e-6, "40 -> 20: {}", g2.zoom);
    }

    #[test]
    fn drei_finger_verschieben_ohne_zoom() {
        let mut g = Gesten::new();
        g.aktualisiere(1, Phase::Start, p(0.0, 0.0));
        g.aktualisiere(2, Phase::Start, p(10.0, 0.0));
        g.aktualisiere(3, Phase::Start, p(20.0, 0.0));
        let g2 = g.aktualisiere(3, Phase::Zug, p(60.0, 0.0)).unwrap();
        assert_eq!(g2.zoom, 1.0, "drei Finger zoomen nicht");
        assert_eq!(g.finger(), 3);
        assert!(g2.pan.0 > 0.0, "die Gruppe verschiebt sich: {:?}", g2.pan);
    }

    #[test]
    fn ein_finger_zoomen_nicht() {
        let mut g = Gesten::new();
        assert_eq!(g.aktualisiere(1, Phase::Start, p(5.0, 5.0)), None);
        assert_eq!(g.aktualisiere(1, Phase::Zug, p(80.0, 5.0)), None);
        assert_eq!(g.finger(), 1);
    }

    #[test]
    fn zweiter_finger_beginnt_die_geste() {
        let mut g = Gesten::new();
        g.aktualisiere(1, Phase::Start, p(0.0, 0.0));
        // Erst der zweite Finger macht aus der Gruppe eine Geste; solange es
        // keinen Vorgabewert gibt, gibt es auch nichts zu vergleichen.
        assert_eq!(g.aktualisiere(2, Phase::Start, p(10.0, 0.0)), None);
        assert_eq!(g.finger(), 2, "aber zwei Finger sind aktiv");
    }

    #[test]
    fn finger_weg_laesst_keine_geste() {
        let mut g = Gesten::new();
        g.aktualisiere(1, Phase::Start, p(0.0, 0.0));
        g.aktualisiere(2, Phase::Start, p(10.0, 0.0));
        assert_eq!(g.finger(), 2);
        assert_eq!(g.aktualisiere(2, Phase::Ende, p(10.0, 0.0)), None);
        assert_eq!(g.finger(), 1, "ein Finger übrig");
        assert_eq!(g.aktualisiere(1, Phase::Ende, p(0.0, 0.0)), None);
        assert_eq!(g.finger(), 0);
    }
}
