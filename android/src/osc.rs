//! OSC-Seite der Android-App: schickt Noten/Transport/BPM an Renoise oder an
//! das Relay (`bundle_udp`) auf dem Laptop. Die Logik steckt in `jamcore::osc`.

use jamcore::osc::{OscOut, Renoise, DEFAULT_BASE_PORT, DEFAULT_HOST};
use rosc::OscType;

pub struct OscPanel {
    pub host: String,
    pub port: String,
    out: Option<OscOut>,
    pub last: String,
    pub error: String,
}

impl Default for OscPanel {
    fn default() -> Self {
        OscPanel {
            host: DEFAULT_HOST.to_string(),
            port: "7003".to_string(),
            out: None,
            last: String::new(),
            error: String::new(),
        }
    }
}

impl OscPanel {
    fn base_port(&self) -> u16 {
        self.port.trim().parse().unwrap_or(DEFAULT_BASE_PORT)
    }

    /// Baut die Verbindung neu auf (Host/Port können im UI geändert werden).
    pub fn connect(&mut self) {
        match OscOut::new(self.host.trim(), self.base_port()) {
            Ok(out) => {
                self.last = format!("verbunden mit {}", out.target());
                self.error.clear();
                self.out = Some(out);
            }
            Err(e) => {
                self.error = format!("Verbindung fehlgeschlagen: {e}");
                self.out = None;
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.out = None;
        self.last = "getrennt".to_string();
    }

    /// Sendet über die bestehende Verbindung, baut sie bei Bedarf neu auf.
    fn send(&mut self, label: &str, run: impl FnOnce(&Renoise) -> Result<u16, String>) {
        if self.out.is_none() {
            self.connect();
        }
        let Some(out) = self.out.as_ref() else {
            self.error = "keine Verbindung".to_string();
            return;
        };
        let renoise = Renoise::new(out);
        match run(&renoise) {
            Ok(used) => {
                self.last = format!("{label} -> Port {used}");
                self.error.clear();
                log::info!("jamandroid: OSC {label} -> Port {used}");
            }
            Err(e) => {
                self.error = format!("{label}: {e}");
                log::info!("jamandroid: OSC {label} fehlgeschlagen: {e}");
            }
        }
    }

    pub fn note(&mut self, note: i32) {
        self.send(&format!("Note {note}"), |r| r.note_on(7000, note, 100));
    }

    pub fn start(&mut self) {
        self.send("Start", |r| r.transport_start(7000));
    }

    pub fn stop(&mut self) {
        self.send("Stop", |r| r.transport_stop(7000));
    }

    pub fn bpm(&mut self, value: f32) {
        self.send(&format!("BPM {value}"), |r| r.bpm(7000, value));
    }

    pub fn pattern_follow(&mut self, on: bool) {
        self.send(if on { "Follow an" } else { "Follow aus" }, |r| {
            r.pattern_follow(7000, on)
        });
    }

    #[allow(dead_code)]
    /// Roher Sendepfad für eigene Aufrufe (z. B. aus dem Rhythmus-Screen).
    pub fn raw(&mut self, port: u16, addr: &str, args: Vec<OscType>) -> bool {
        if self.out.is_none() {
            self.connect();
        }
        match self.out.as_ref().map(|o| o.send(port, addr, args)) {
            Some(Ok(used)) => {
                self.last = format!("{addr} -> Port {used}");
                self.error.clear();
                true
            }
            Some(Err(e)) => {
                self.error = e;
                false
            }
            None => false,
        }
    }
}
