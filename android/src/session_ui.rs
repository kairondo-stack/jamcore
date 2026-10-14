//! Session-Tab (S9): Link-Zustand anzeigen und Tempo/Phase teilen.
//!
//! Die Link-Session selbst hält `JamApp` (rusty_link); dieses Panel zeigt den
//! Zustand und gibt Aktionen zurück. Die Quantisierung rechnet
//! `jamcore::session`, damit dieselben Tests auf Desktop und Handy gelten.

use eframe::egui::{self, Color32};

use jamcore::session::{naechster_beat, wartezeit_bis_beat, SessionZustand};

/// Aktion, die die Oberfläche von der App ausgeführt haben will.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SessionAktion {
    /// Link einschalten (true) oder ausschalten (false).
    LinkUmschalten(bool),
    /// Link neu starten: Link scannt die Netzwerk-Interfaces nur beim
    /// Start; nach dem Einschalten eines Hotspots findet er das neue
    /// Interface sonst nicht (Multicast-Gruppe fehlt auf ap_br_wlan2).
    LinkNeustart,
}

pub struct SessionPanel {
    zustand: SessionZustand,
    folgen: bool,
    quantum: f64,
    status: String,
}

impl Default for SessionPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionPanel {
    pub fn new() -> Self {
        Self {
            zustand: SessionZustand::default(),
            folgen: false,
            quantum: 4.0,
            status: "Link startet beim App-Start; Peers brauchen dasselbe Netz.".to_string(),
        }
    }

    /// Wird beim Zeichnen mit den Live-Werten der Link-Session gefüttert.
    pub fn set_zustand(&mut self, zustand: SessionZustand) {
        self.zustand = zustand;
    }

    /// Folgt die App dem Session-Tempo?
    pub fn folgen(&self) -> bool {
        self.folgen
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, link_aktiv: bool) -> Option<SessionAktion> {
        let mut aktion = None;

        let z = self.zustand;
        ui.horizontal(|ui| {
            ui.heading("Session");
            ui.label(
                egui::RichText::new(format!("{:.1} BPM", z.bpm))
                    .size(24.0)
                    .strong(),
            );
            if link_aktiv {
                ui.colored_label(Color32::LIGHT_GREEN, "Link an");
            } else {
                ui.colored_label(Color32::GRAY, "Link aus");
            }
        });

        let beat_raster = naechster_beat(z.beat, self.quantum);
        let warten = wartezeit_bis_beat(z.beat, self.quantum, z.bpm);
        ui.label(format!(
            "Beat {:.3} -> nächster Rasterpunkt {:.0} (in {:.2} s) · Peers: {} · Transport: {}",
            z.beat,
            beat_raster,
            warten,
            z.peers,
            if z.playing { "läuft" } else { "steht" }
        ));

        ui.horizontal(|ui| {
            if ui
                .button(if link_aktiv { "Link aus" } else { "Link an" })
                .clicked()
            {
                aktion = Some(SessionAktion::LinkUmschalten(!link_aktiv));
            }
            if ui.button("Link neu starten").clicked() {
                aktion = Some(SessionAktion::LinkNeustart);
            }
            let _ = ui.checkbox(&mut self.folgen, "Tempo folgen");
            ui.label("Quantum");
            ui.add(egui::Slider::new(&mut self.quantum, 1.0..=16.0).integer());
        });
        ui.label(
            egui::RichText::new(
                "„Tempo folgen“ übernimmt das Link-Tempo für den Live-Scan im Bild-Tab.",
            )
            .small()
            .color(Color32::DARK_GRAY),
        );
        if self.folgen {
            self.status = format!("Scan-Tempo folgt der Session ({:.1} BPM)", z.bpm);
        }
        ui.label(
            egui::RichText::new(&self.status)
                .small()
                .color(Color32::GRAY),
        );

        aktion
    }
}
