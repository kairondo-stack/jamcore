//! JamCore-Android-App: egui-Oberfläche mit Modul-Player (AudioTrack + libopenmpt).
//! Weitere Module (Rhythmus, Bild/Scan, Session, Tagebuch) folgen in S4+.

mod camera;
mod diary;
mod image;
mod insets;
mod jniutil;
mod klang;
mod layout;
mod link_support;
mod maske;
mod mic;
mod osc;
mod paths;
mod player;
mod quint;
mod rhythm;
mod session_ui;
mod touch;
mod video;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[cfg(target_os = "android")]
static APP: OnceLock<Mutex<Option<android_activity::AndroidApp>>> = OnceLock::new();

/// Globaler Zugriff auf die App, damit der Player sie für JNI braucht.
pub fn current_app() -> Result<android_activity::AndroidApp, String> {
    APP.get()
        .and_then(|cell| cell.lock().ok().and_then(|guard| guard.clone()))
        .ok_or_else(|| "AndroidApp nicht verfuegbar".to_string())
}

use eframe::egui;
use layout::KombiTeil;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Ueberblick,
    Player,
    Osc,
    Rhythmus,
    Tempo,
    Bild,
    Kreis,
    Session,
    Tagebuch,
    Maske,
    Kombi,
}

struct JamApp {
    tab: Tab,
    status: String,
    meldung: String,
    insets_px: Option<insets::InsetsPx>,
    frames: u32,
    #[cfg(target_os = "android")]
    android_app: Option<android_activity::AndroidApp>,
    #[cfg(not(target_os = "android"))]
    android_app: (),
    module_hint: String,
    modules: Vec<PathBuf>,
    selected: Option<usize>,
    looping: bool,
    player: Option<player::Player>,
    osc: osc::OscPanel,
    rhythm: rhythm::RhythmPanel,
    mic: mic::MicPanel,
    bild: image::ImagePanel,
    kreis: quint::QuintPanel,
    session: session_ui::SessionPanel,
    link_aktiv: bool,
    multicast_aktiv: bool,
    kombi_oben: KombiTeil,
    kombi_unten: KombiTeil,
    diary: diary::DiaryPanel,
    maske: maske::MaskePanel,
    /// **Ein** BPM für Tempo-Tab, Takt-Scan, Rendern und Tagebuch. Mit aktivem
    /// Link-Follow gewinnt das Session-Tempo (siehe `tempo_aktuell`).
    tempo_bpm: f32,
    /// Tempo-Teiler fuer Renoise-Port 7001 (wie rrn5 send_bpm; 1.0 = aus).
    tempo_teiler: f32,
    last_peak: f32,
    last_report: Instant,
    /// S2b-Spike: Link-Session halten und regelmaessig ihren Zustand loggen.
    link: jamcore::rusty_link::AblLink,
    link_report: Instant,
    kamera_laeuft: bool,
}

impl Default for JamApp {
    fn default() -> Self {
        JamApp {
            tab: Tab::Ueberblick,
            status: "Bereit".to_string(),
            meldung: String::new(),
            insets_px: None,
            frames: 0,
            android_app: None,
            module_hint: String::new(),
            modules: Vec::new(),
            selected: None,
            looping: true,
            player: None,
            osc: osc::OscPanel::default(),
            rhythm: rhythm::RhythmPanel::default(),
            mic: mic::MicPanel::default(),
            bild: image::ImagePanel::default(),
            kreis: quint::QuintPanel::default(),
            session: session_ui::SessionPanel::default(),
            link_aktiv: true,
            multicast_aktiv: false,
            kombi_oben: KombiTeil::Bild,
            kombi_unten: KombiTeil::Rhythmus,
            diary: diary::DiaryPanel::default(),
            maske: maske::MaskePanel::default(),
            tempo_bpm: 128.0,
            tempo_teiler: 1.0,
            last_peak: 0.0,
            last_report: Instant::now(),
            link: {
                let link = jamcore::rusty_link::AblLink::new(120.0);
                link.enable(true);
                log::info!("link session gestartet (tempo=120)");
                link
            },
            link_report: Instant::now(),
            kamera_laeuft: false,
        }
    }
}

impl JamApp {
    fn app(&self) -> Option<&android_activity::AndroidApp> {
        self.android_app.as_ref()
    }

    fn insets_px(&mut self) -> Option<insets::InsetsPx> {
        #[cfg(target_os = "android")]
        {
            if self.insets_px.is_some() {
                return self.insets_px;
            }
            let app = self.android_app.clone()?;
            self.frames += 1;
            let found = insets::root_insets_px(&app).or_else(|| {
                if self.frames > 180 {
                    insets::status_bar_px(&app)
                } else {
                    None
                }
            });
            if let Some(px) = found {
                log::info!("systemleisten {px:?}px");
                self.insets_px = Some(px);
            }
            self.insets_px
        }
        #[cfg(not(target_os = "android"))]
        {
            self.insets_px
        }
    }

    fn scan_modules(&mut self) {
        let dirs = match self.app() {
            Some(app) => paths::search_dirs(app),
            None => Vec::new(),
        };
        self.modules = paths::scan(&dirs);
        if self.module_hint.is_empty() {
            if let Some(app) = self.app() {
                if let Some(external) = insets::external_files_dir(app) {
                    self.module_hint = format!("Eigene Module: {external}/module");
                }
            }
        }
        if self.selected.is_none() {
            self.selected = (!self.modules.is_empty()).then_some(0);
        }
        if self.selected.is_some_and(|i| i >= self.modules.len()) {
            self.selected = None;
        }
        log::info!(
            "{} Modul(e) in {} Verzeichnis(s)",
            self.modules.len(),
            dirs.len()
        );
    }

    fn play_selected(&mut self) {
        let Some(index) = self.selected else {
            self.meldung = "Kein Modul ausgewählt".to_string();
            return;
        };
        let Some(path) = self.modules.get(index).cloned() else {
            return;
        };
        if let Some(mut old) = self.player.take() {
            old.stop();
        }
        match player::Player::play(&path, self.looping, 44100) {
            Ok(p) => {
                self.status = format!("spielt: {}", file_name(&path));
                self.player = Some(p);
                self.meldung.clear();
                self.last_report = Instant::now();
            }
            Err(e) => {
                self.status = "Wiedergabe gestartet".to_string();
                self.meldung = format!("Fehler: {e}");
            }
        }
    }

    fn stop(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
            self.status = "gestoppt".to_string();
        }
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl eframe::App for JamApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // MulticastLock fuer Link holen, sobald die App laeuft (ohne ihn
        // kommen im WLAN keine Peer-Pakete an).
        if self.link_aktiv && !self.multicast_aktiv {
            if let Ok(app) = crate::current_app() {
                if link_support::aktivieren(&app).is_ok() {
                    self.multicast_aktiv = true;
                }
            }
        }
        // S2b-Spike: einmal pro Sekunde den Link-Zustand ins Log, damit der
        // Gerätetest ohne UI-Arbeit belegt, dass die Session lebt.
        if self.link_report.elapsed() >= std::time::Duration::from_secs(1) {
            let mut session = jamcore::rusty_link::SessionState::new();
            self.link.capture_app_session_state(&mut session);
            log::info!(
                "link tempo={:.2} peers={} beat={:.3}",
                session.tempo(),
                self.link.num_peers(),
                session.beat_at_time(self.link.clock_micros(), 4.0)
            );
            self.link_report = Instant::now();
            if self.kamera_laeuft {
                let (frames, breite, hoehe) = camera::stand();
                if frames > 0 {
                    log::info!("bild frames={frames} size={breite}x{hoehe}");
                }
                // Java meldet asynchrone Fehler (Kamera getrennt, onError)
                // über status(); ohne diese Abfrage bliebe kamera_laeuft true.
                let meldung = match self.app() {
                    Some(app) => camera::java_fehlermeldung(app),
                    None => String::new(),
                };
                if !meldung.is_empty() {
                    self.status = format!("Kamera: {meldung}");
                    self.kamera_laeuft = false;
                }
            }
        }
        // Ohne Ereignisse zeichnet egui nicht neu; fuer den Spike-Takt (und
        // spaeter fuer Live-Anzeigen) mindestens einmal pro Sekunde.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));

        let px = self.insets_px();
        if px.is_none() {
            ui.ctx().request_repaint();
        }
        let ppp = ui.ctx().input(|i| i.pixels_per_point()).max(0.1);
        let top = px.map(|p| p.0 / ppp).unwrap_or(0.0);
        let bottom = px.map(|p| p.1 / ppp).unwrap_or(0.0);

        egui::Panel::top("kopf").show(ui, |ui| {
            ui.add_space(top);
            ui.horizontal(|ui| {
                ui.heading("JamCore");
                // Acht Tabs passen nicht in eine Zeile; sie brechen um. Eine
                // scrollende Leiste war auf dem Touchscreen schwer zu treffen.
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Ueberblick, "Überblick");
                    ui.selectable_value(&mut self.tab, Tab::Player, "Player");
                    ui.selectable_value(&mut self.tab, Tab::Osc, "OSC");
                    ui.selectable_value(&mut self.tab, Tab::Rhythmus, "Rhythmus");
                    ui.selectable_value(&mut self.tab, Tab::Tempo, "Tempo");
                    ui.selectable_value(&mut self.tab, Tab::Bild, "Bild");
                    ui.selectable_value(&mut self.tab, Tab::Kreis, "Kreis");
                    ui.selectable_value(&mut self.tab, Tab::Session, "Session");
                    ui.selectable_value(&mut self.tab, Tab::Tagebuch, "Tagebuch");
                    ui.selectable_value(&mut self.tab, Tab::Maske, "Maske");
                    ui.selectable_value(&mut self.tab, Tab::Kombi, "Kombi");
                });
            });
            ui.add_space(4.0);
        });

        // Das untere Panel muss vor dem CentralPanel kommen, sonst
        // bekommt es keinen Platz mehr.
        egui::Panel::bottom("fuss").show(ui, |ui| {
            ui.add_space(bottom);
            if !self.meldung.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.meldung);
            }
            ui.label(format!("Status: {}", self.status));
            ui.add_space(4.0);
        });

        if let Some(bpm) = self.mic.uebernommenes_tempo() {
            self.tempo_bpm = bpm;
            self.status = format!("Tempo übernommen: {bpm:.1} BPM");
            log::info!("Tempo uebernommen: {bpm:.1} BPM");
        }
        match self.tab {
            Tab::Ueberblick => self.view_ueberblick(ui),
            Tab::Player => self.view_player(ui),
            Tab::Osc => self.view_osc(ui),
            Tab::Rhythmus => self.view_rhythm(ui),
            Tab::Tempo => self.view_mic(ui),
            Tab::Bild => self.view_bild(ui),
            Tab::Kreis => self.view_kreis(ui),
            Tab::Session => self.view_session(ui),
            Tab::Tagebuch => self.view_tagebuch(ui),
            Tab::Maske => self.view_maske(ui),
            Tab::Kombi => self.view_kombi(ui),
        }
    }
}

impl JamApp {
    /// Das **eine** Tempo der App: mit aktivem Link-Follow das Session-Tempo,
    /// sonst der im OSC-Tab eingestellte Wert. Bild-Scan, Rendern und Tagebuch
    /// benutzen alle diese Zahl.
    fn tempo_aktuell(&self) -> f32 {
        jamcore::session::tempo_gesamt(self.session.folgen(), self.session.tempo(), self.tempo_bpm)
    }

    fn view_ueberblick(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                "Fertig: Modul-Player, OSC zu Renoise, Rhythmus-Raster, Tempo-Messung. \
                 Es folgen: Bild/Scan (S7), Session (S9), Tagebuch (S10).",
            );
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Kamera an").clicked() {
                    let ergebnis = match self.app() {
                        Some(app) => camera::start(app),
                        None => Err("keine Activity".to_string()),
                    };
                    match ergebnis {
                        Ok(()) => {
                            self.kamera_laeuft = true;
                            self.status = "Kamera startet …".to_string();
                        }
                        Err(e) => self.status = format!("Kamera: {e}"),
                    }
                }
                if ui.button("Kamera aus").clicked() {
                    if let Some(app) = self.app() {
                        camera::stop(app);
                    }
                    self.kamera_laeuft = false;
                    self.status = "Kamera gestoppt".to_string();
                }
                let (frames, breite, hoehe) = camera::stand();
                ui.label(format!("Kamera: {frames} Frames, {breite}x{hoehe}"));
            });
            ui.label(format!("Module gefunden: {}", self.modules.len()));
            if !self.module_hint.is_empty() {
                ui.label(&self.module_hint);
            }
            ui.separator();
            if ui.button("Module neu suchen").clicked() {
                self.scan_modules();
            }
            if ui.button("Zum Player").clicked() {
                self.tab = Tab::Player;
                if self.modules.is_empty() {
                    self.scan_modules();
                }
            }
        });
    }

    fn view_maske(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .id_salt("maske_inhalt")
                .show(ui, |ui| self.maske.ui(ui, self.rhythm.pattern_mut()));
        });
    }

    /// Kombi-Ansicht: zwei Module gleichzeitig (im Hochformat untereinander,
    /// im Querformat nebeneinander).
    fn view_kombi(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            let (mut oben, mut unten) = (self.kombi_oben, self.kombi_unten);
            layout::wahlzeile(ui, &mut oben, &mut unten);
            self.kombi_oben = oben;
            self.kombi_unten = unten;
            layout::bereiche(ui, oben, unten, |ui, teil| self.teil_ui(ui, teil));
        });
    }

    /// Zeichnet ein Modul ohne eigenen Rahmen (fuer den Kombi-Tab).
    fn teil_ui(&mut self, ui: &mut egui::Ui, teil: KombiTeil) {
        match teil {
            KombiTeil::Leer => {
                ui.colored_label(egui::Color32::GRAY, "leer");
            }
            KombiTeil::Bild => {
                let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
                self.bild.set_target(&host, &port);
                let tempo = self.tempo_aktuell();
                self.bild.set_bpm(tempo);
                self.bild.ui(ui);
            }
            KombiTeil::Rhythmus => {
                let tempo = self.tempo_aktuell();
                self.rhythm.set_tempo(tempo);
                self.rhythm.ui(ui);
            }
            KombiTeil::Maske => self.maske.ui(ui, self.rhythm.pattern_mut()),
            KombiTeil::Tempo => {
                let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
                self.mic.set_target(&host, &port);
                self.mic.ui(ui);
            }
            KombiTeil::Session => self.session_block(ui),
            KombiTeil::Tagebuch => {
                let bpm = Some(self.tempo_aktuell());
                let noten = self.rhythm.pattern().clone();
                self.diary.ui(ui, bpm, &noten);
            }
            KombiTeil::Kreis => {
                let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
                self.kreis.set_target(&host, &port);
                self.kreis.ui(ui);
            }
        }
    }

    fn view_tagebuch(&mut self, ui: &mut egui::Ui) {
        // Das eine Tempo (Follow gewinnt) statt der letzten Mikrofonmessung.
        let bpm = Some(self.tempo_aktuell());
        let noten = self.rhythm.pattern().clone();
        // Der Bild-Export liegt zuklappbar **über** dem Tagebuch: darunter füllt
        // die Einträge-Liste die ganze Fläche, sonst wäre er unsichtbar.
        let abschnitt = self.bild.abschnitt_pattern();
        let abschnitt_schritte = self.bild.schritte();
        egui::Panel::top("tagebuch_bild_pattern").show(ui, |ui| {
            egui::CollapsingHeader::new("Bild-Pattern (Start/Ende aus dem Bild-Tab)")
                .id_salt("bild_pattern_kopf")
                .show(ui, |ui| {
                    ui.label(format!(
                        "X-Auflösung {} Schritte, Ausschnitt ergibt {} Schritte",
                        abschnitt_schritte,
                        abschnitt.steps_per_cycle()
                    ));
                    let xml = abschnitt.to_renoise_xml();
                    let mut xml_text = xml.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut xml_text)
                            .desired_rows(8)
                            .font(egui::TextStyle::Monospace),
                    );
                    if ui.button("Renoise-XML kopieren").clicked() {
                        ui.ctx().copy_text(xml.clone());
                        self.status = "Renoise-XML in die Zwischenablage.".to_string();
                    }
                    ui.label(format!("CSV (normiert 0..1):\n{}", abschnitt.to_csv()));
                });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.diary.ui(ui, bpm, &noten);
        });
    }

    fn view_session(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| self.session_block(ui));
    }

    /// Session-Inhalt ohne eigenen Rahmen (fuer den Kombi-Tab).
    fn session_block(&mut self, ui: &mut egui::Ui) {
        // Link-Zustand holen und dem Panel geben (wie beim Rhythmus-Follow).
        let mut s = jamcore::rusty_link::SessionState::new();
        self.link.capture_app_session_state(&mut s);
        let zustand = jamcore::session::SessionZustand {
            bpm: s.tempo(),
            beat: s.beat_at_time(self.link.clock_micros(), 4.0),
            peers: self.link.num_peers() as usize,
            playing: s.is_playing(),
        };
        self.session.set_zustand(zustand);
        let link_aktiv = self.link_aktiv;
        if let Some(aktion) = self.session.ui(ui, link_aktiv) {
            match aktion {
                session_ui::SessionAktion::LinkNeustart => {
                    self.link.enable(false);
                    self.link.enable(true);
                    self.status = "Link neu gestartet (Interfaces neu gesucht)".to_string();
                    log::info!("session: Link neu gestartet");
                }
                session_ui::SessionAktion::LinkUmschalten(an) => {
                    self.link.enable(an);
                    self.link_aktiv = an;
                    if let Ok(app) = crate::current_app() {
                        if an {
                            let _ = link_support::aktivieren(&app);
                        } else {
                            link_support::freigeben(&app);
                        }
                    }
                    self.status = format!("Link {}", if an { "an" } else { "aus" });
                    log::info!("session: Link {}", if an { "an" } else { "aus" });
                }
            }
        }
    }

    fn view_kreis(&mut self, ui: &mut egui::Ui) {
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.kreis.set_target(&host, &port);
        egui::CentralPanel::default().show(ui, |ui| self.kreis.ui(ui));
    }

    fn view_bild(&mut self, ui: &mut egui::Ui) {
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.bild.set_target(&host, &port);
        // Das eine Tempo (Follow gewinnt) — der Takt-Scan rechnet daraus die
        // Sweep-Dauer.
        let tempo = self.tempo_aktuell();
        self.bild.set_bpm(tempo);
        egui::CentralPanel::default().show(ui, |ui| self.bild.ui(ui));
    }

    fn view_mic(&mut self, ui: &mut egui::Ui) {
        // Das Ziel steht einmal im OSC-Tab; der Tempo-Tab bekommt es hier
        // durchgereicht (sonst müsste die IP zweimal eingetippt werden).
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.mic.set_target(&host, &port);
        egui::CentralPanel::default().show(ui, |ui| self.mic.ui(ui));
    }

    fn view_rhythm(&mut self, ui: &mut egui::Ui) {
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.rhythm.set_target(&host, &port);
        // Rendern benutzt dasselbe Tempo wie der Takt-Scan.
        let tempo = self.tempo_aktuell();
        self.rhythm.set_tempo(tempo);
        if self.rhythm.follow() {
            if let Some(player) = self.player.as_ref() {
                self.rhythm.set_follow_step(player.grid_step(4));
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .id_salt("rhythmus_inhalt")
                .show(ui, |ui| self.rhythm.ui(ui));
        });
    }

    /// Das eine Tempo an Renoise schicken (`/renoise/song/bpm`).
    fn tempo_senden(&mut self) {
        let bpm = self.tempo_aktuell();
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        let port = port.parse().unwrap_or(jamcore::osc::DEFAULT_BASE_PORT);
        // Wie im Desktop-rrn5: 7000 bekommt das Tempo, 7001 das gefaltete
        // (nur wenn der Teiler etwas aendert).
        let (basis, gefaltet) = jamcore::osc::bpm_und_teiler(bpm, self.tempo_teiler);
        let ergebnis = jamcore::osc::OscOut::new(&host, port).and_then(|out| {
            let renoise = jamcore::osc::Renoise::new(&out);
            let verwendet = renoise.bpm(7000, basis)?;
            if let Some(gefaltet) = gefaltet {
                if let Err(e) = renoise.bpm(7001, gefaltet) {
                    log::warn!("Tempo-Teiler: {e}");
                }
            }
            Ok(verwendet)
        });
        match ergebnis {
            Ok(used) => {
                self.status = match gefaltet {
                    Some(gefaltet) => {
                        format!(
                            "Tempo {basis:.1} BPM -> {host}:{used}, {gefaltet:.1} BPM -> Port+1"
                        )
                    }
                    None => format!("Tempo {basis:.1} BPM -> {host}:{used}"),
                };
                log::info!("Tempo {basis:.1} BPM -> {host}:{used}");
            }
            Err(e) => self.status = format!("OSC-Fehler: {e}"),
        }
    }

    fn view_osc(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Ziel (Renoise oder bundle_udp auf dem Laptop) — einmal für OSC und Tempo:");
            ui.horizontal(|ui| {
                ui.label("IP");
                ui.text_edit_singleline(&mut self.osc.host);
                ui.label("Port");
                ui.text_edit_singleline(&mut self.osc.port);
            });
            ui.separator();
            ui.label("Tempo (gilt für Tempo-Tab, Takt-Scan, Rendern und Tagebuch):");
            let mut bpm = self.tempo_bpm;
            if ui.add(egui::Slider::new(&mut bpm, 30.0..=300.0)).changed() {
                self.tempo_bpm = bpm;
            }
            ui.horizontal(|ui| {
                ui.label("Teiler (Port 7001)");
                ui.add(egui::Slider::new(&mut self.tempo_teiler, 0.25..=4.0).logarithmic(true));
                if ui.button("1:1").clicked() {
                    self.tempo_teiler = 1.0;
                }
            });
            if ui.button("Tempo senden").clicked() {
                self.tempo_senden();
            }
            ui.horizontal(|ui| {
                if ui.button("Verbinden").clicked() {
                    self.osc.connect();
                }
                if ui.button("Trennen").clicked() {
                    self.osc.disconnect();
                }
            });
            ui.separator();

            ui.label("Noten (Renoise /renoise/trigger/note_on):");
            ui.horizontal(|ui| {
                for note in [48, 60, 64, 67, 72] {
                    let label = format!("{note}");
                    if ui.button(&label).clicked() {
                        self.osc.note(note);
                    }
                }
            });

            ui.label("Transport und Tempo:");
            ui.horizontal(|ui| {
                if ui.button("Start").clicked() {
                    self.osc.start();
                }
                if ui.button("Stop").clicked() {
                    self.osc.stop();
                }
                if ui.button("BPM 128").clicked() {
                    self.osc.bpm(128.0);
                }
                if ui.button("Follow an").clicked() {
                    self.osc.pattern_follow(true);
                }
                if ui.button("Follow aus").clicked() {
                    self.osc.pattern_follow(false);
                }
            });

            ui.separator();
            if !self.osc.last.is_empty() {
                ui.label(format!("Letztes: {}", self.osc.last));
            }
            if !self.osc.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.osc.error);
            }
            ui.label("Handy und Laptop im selben Netz: hier die Laptop-IP eintagen.");
            ui.label("adb reverse kann nur TCP, OSC laeuft ueber UDP.");
        });
    }

    fn view_player(&mut self, ui: &mut egui::Ui) {
        // Werte laufen im Audio-Thread, also regelmaessig neu zeichnen.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Neu suchen").clicked() {
                    self.scan_modules();
                }
                if ui.button("Abspielen").clicked() {
                    self.play_selected();
                }
                if ui.button("Stopp").clicked() {
                    self.stop();
                }
                ui.checkbox(&mut self.looping, "Dauerschleife");
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .show(ui, |ui| {
                    let modules = self.modules.clone();
                    for (index, path) in modules.iter().enumerate() {
                        let selected = self.selected == Some(index);
                        if ui.selectable_label(selected, file_name(path)).clicked() {
                            self.selected = Some(index);
                            self.play_selected();
                        }
                    }
                });

            if self.modules.is_empty() {
                ui.separator();
                ui.label("Keine Module gefunden (.it/.xm/.mod/.s3m/.mtm).");
                if !self.module_hint.is_empty() {
                    ui.label(&self.module_hint);
                }
            }

            ui.separator();
            if let Some(player) = self.player.as_mut() {
                let peak = player.take_peak();
                if peak > self.last_peak {
                    self.last_peak = peak;
                }
                let (order, row) = player.order_row();
                let pos = player.position_seconds();
                let frames = player.frames();
                ui.label(format!(
                    "Peak: {:.3}   Position: {:.1} s   Order/Row: {}/{}   Frames: {}",
                    self.last_peak, pos, order, row, frames
                ));
                if self.last_report.elapsed().as_secs_f32() > 2.0 {
                    self.last_report = Instant::now();
                    log::info!(
                        "rms peak={:.4} pos={:.1}s order={} row={} frames={}",
                        self.last_peak,
                        pos,
                        order,
                        row,
                        frames
                    );
                    self.last_peak = 0.0;
                }
                if player.finished() {
                    ui.colored_label(egui::Color32::YELLOW, "Modul am Ende");
                }
                // Modul-Info und Samples wie im Desktop-it_live.
                for (name, wert) in player.info_paare() {
                    ui.label(format!("{name}: {wert}"));
                }
                let samples = player.sample_namen(64);
                if !samples.is_empty() {
                    ui.collapsing(format!("Samples ({})", samples.len()), |ui| {
                        ui.label(samples.join(", "));
                    });
                }
            } else {
                ui.label("Nichts abgespielt.");
            }
        });
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("android_main gestartet");
    let options = eframe::NativeOptions {
        android_app: Some(app.clone()),
        ..Default::default()
    };
    let mut jam_app = JamApp::default();
    jam_app.android_app = Some(app.clone());
    let _ = APP.set(Mutex::new(Some(app)));
    jam_app.scan_modules();
    let _ = eframe::run_native("JamCore", options, Box::new(|_cc| Ok(Box::new(jam_app))));
    log::info!("beendet");
}
