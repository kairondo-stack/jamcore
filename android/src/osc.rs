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
    /// Das gemerkte Ziel wurde einmal geladen; danach gilt das Feld.
    geladen: bool,
}

impl Default for OscPanel {
    fn default() -> Self {
        OscPanel {
            host: DEFAULT_HOST.to_string(),
            port: "7003".to_string(),
            out: None,
            last: String::new(),
            error: String::new(),
            geladen: false,
        }
    }
}

impl OscPanel {
    fn base_port(&self) -> u16 {
        self.port.trim().parse().unwrap_or(DEFAULT_BASE_PORT)
    }

    /// Ziel, wie es im Feld steht.
    fn ziel(&self) -> String {
        format!("{}:{}", self.host.trim(), self.base_port())
    }

    /// Passt die bestehende Verbindung noch zum Feld?
    fn verbunden_mit_feld(&self) -> bool {
        self.out
            .as_ref()
            .is_some_and(|o| o.target().to_string() == self.ziel())
    }

    /// Pfad der kleinen Ziel-Datei (Host, Port) im App-Ordner.
    fn datei() -> Option<std::path::PathBuf> {
        let app = crate::current_app().ok()?;
        let basis = crate::insets::external_files_dir(&app)?;
        Some(std::path::Path::new(&basis).join("osc.txt"))
    }

    /// Einmal beim Start das gemerkte Ziel laden. Ohne das stand nach jedem
    /// Neustart wieder 127.0.0.1 im Feld (gemessen 2026-10-12).
    pub fn laden(&mut self) {
        if self.geladen {
            return;
        }
        self.geladen = true;
        let Some(pfad) = Self::datei() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&pfad) else {
            return;
        };
        if let Some((host, port)) = jamcore::osc::settings_aus_text(&text) {
            log::info!("OSC-Ziel geladen: {host}:{port}");
            self.host = host;
            self.port = port;
        }
    }

    /// Das aktuelle Feld-Ziel merken, damit der naechste Start es kennt.
    fn speichern(&self) {
        let Some(pfad) = Self::datei() else {
            return;
        };
        let text = jamcore::osc::settings_als_text(self.host.trim(), self.port.trim());
        if let Err(e) = std::fs::write(&pfad, text) {
            log::info!("OSC-Ziel nicht gespeichert: {e}");
        }
    }

    /// Baut die Verbindung neu auf (Host/Port können im UI geändert werden).
    pub fn connect(&mut self) {
        match OscOut::new(self.host.trim(), self.base_port()) {
            Ok(out) => {
                self.last = format!("verbunden mit {}", out.target());
                self.error.clear();
                self.out = Some(out);
                self.speichern();
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

    /// Sendet über die bestehende Verbindung; hat sich das Ziel im Feld
    /// geaendert, wird vorher neu verbunden. Ohne den Vergleich gingen Noten
    /// nach einer Feldänderung weiter an das **alte** Ziel (gemessen
    /// 2026-10-12: Port 7009 im Feld, gesendet wurde an 7005).
    fn send(&mut self, label: &str, run: impl FnOnce(&Renoise) -> Result<u16, String>) {
        if !self.verbunden_mit_feld() {
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
        if !self.verbunden_mit_feld() {
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
