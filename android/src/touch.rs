//! Adapter zwischen egui-Touchereignissen und der Gestenlogik im Kern.
//!
//! Auf Android liefert egui kein `Event::Zoom`; winit reicht je Finger ein
//! `egui::Event::Touch` durch. `Mehrfinger` sammelt die Finger eines Frames und
//! meldet Zoom (zwei Finger) und Verschiebung (zwei oder drei Finger).

use eframe::egui;
use jamcore::gesten::{Gesten, Phase, Punkt};

/// Was ein Frame an Berührungen ergeben hat.
#[derive(Default)]
pub struct Rahmen {
    /// Zoomfaktor und Weg der Gruppe, falls sich etwas bewegt hat.
    pub geste: Option<(f32, (f32, f32))>,
    /// Finger, die **neu aufgesetzt** wurden (Nummer, Position).
    pub starts: Vec<(u64, egui::Pos2)>,
    /// Finger, die sich bewegt haben.
    pub zuege: Vec<(u64, egui::Pos2)>,
    /// Finger, die wieder weg sind (Position = letzte bekannte).
    pub endes: Vec<(u64, egui::Pos2)>,
}

/// Sammelt die Finger über die Frames hinweg.
#[derive(Default)]
pub struct Mehrfinger {
    gesten: Gesten,
}

impl Mehrfinger {
    pub fn neu() -> Self {
        Self::default()
    }

    /// Anzahl der gerade unten liegenden Finger.
    pub fn finger(&self) -> usize {
        self.gesten.finger()
    }

    /// Alle Berührungen dieses Frames auswerten.
    pub fn schritt(&mut self, ui: &egui::Ui) -> Rahmen {
        let ereignisse = ui.input(|eingabe| eingabe.events.clone());
        let mut raus = Rahmen::default();
        for ereignis in ereignisse {
            if let egui::Event::Touch { id, phase, pos, .. } = ereignis {
                let phase = match phase {
                    egui::TouchPhase::Start => Phase::Start,
                    egui::TouchPhase::Move => Phase::Zug,
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => Phase::Ende,
                };
                match phase {
                    Phase::Start => raus.starts.push((id.0, pos)),
                    Phase::Zug => raus.zuege.push((id.0, pos)),
                    Phase::Ende => raus.endes.push((id.0, pos)),
                }
                if let Some(geste) =
                    self.gesten
                        .aktualisiere(id.0, phase, Punkt { x: pos.x, y: pos.y })
                {
                    // Beim Loslassen aendert sich nichts mehr — sonst wuerde der
                    // letzte Abstand noch einmal zoomen.
                    if phase != Phase::Ende {
                        raus.geste = Some((geste.zoom, geste.pan));
                    }
                }
            }
        }
        raus
    }
}
