//! JamCore-Android-App: egui-Oberfläche mit Modul-Player (AudioTrack + libopenmpt).
//! Weitere Module (Rhythmus, Bild/Scan, Session, Tagebuch) folgen in S4+.

mod akkord;
mod browser;
mod camera;
mod diary;
mod image;
mod insets;
mod instr;
mod jniutil;
mod klang;
mod kreis_presets;
mod laub;
mod layout;
mod link_support;
mod lissa;
mod maske;
mod maske2;
mod mic;
mod ordner;
mod osc;
mod paths;
mod pipeline;
mod player;
mod quint;
mod rhythm;
mod session_ui;
mod slicer;
mod tasten;
mod touch;
mod transport;
mod video;
mod wavrec;

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
    Laub,
    Kreis,
    Tagebuch,
    Maske,
    Maske2,
    Bild2,
    Lissa,
    Instr,
    Slicer,
    Kombi,
}

struct JamApp {
    tab: Tab,
    /// Vollbild: Kopfzeile/Tab-Leiste ausblenden (Bild/B2) — Ausstieg über
    /// den schwebenden Knopf oben rechts.
    vollbild: bool,
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
    /// Datei-Browser (Player-Tab „Öffnen"), offen solange `Some`.
    browser: Option<browser::Browser>,
    /// Datei-Browser des Slicer-Tabs („Audio öffnen"), offen solange `Some`.
    browser_audio: Option<browser::Browser>,
    /// Datei-Browser des Laub-Tabs („Bild laden …"), offen solange `Some`.
    browser_bild: Option<browser::Browser>,
    /// Datei-Browser des Laub-Tabs („Video laden …"), offen solange `Some`.
    browser_video: Option<browser::Browser>,
    /// Datei-Browser des Instr-Tabs („Sample…"): offen solange `Some`.
    browser_instr: Option<browser::Browser>,
    /// Welches Instrument (0-basiert) gerade ein Sample bekommen soll, während
    /// `browser_instr` offen ist.
    instr_tausch: Option<usize>,
    looping: bool,
    player: Option<player::Player>,
    instr: instr::InstrumentPanel,
    osc: osc::OscPanel,
    rhythm: rhythm::RhythmPanel,
    slicer: slicer::SlicerPanel,
    mic: mic::MicPanel,
    bild: image::ImagePanel,
    laub: laub::LaubPanel,
    kreis: quint::QuintPanel,
    lissa: lissa::LissaPanel,
    session: session_ui::SessionPanel,
    link_aktiv: bool,
    multicast_aktiv: bool,
    kombi_oben: KombiTeil,
    kombi_unten: KombiTeil,
    diary: diary::DiaryPanel,
    maske: maske::MaskePanel,
    maske2: maske2::Maske2Panel,
    /// Wie stark die Wischgeschwindigkeit die Schrittweite im Maske-Tab kippt.
    maske_staerke: f32,
    /// **Ein** BPM für Tempo-Tab, Takt-Scan, Rendern und Tagebuch. Mit aktivem
    /// Link-Follow gewinnt das Session-Tempo (siehe `tempo_aktuell`).
    tempo_bpm: f32,
    /// Das eine BPM der App zuletzt — ein Reglerzug irgendwo schreibt es ins
    /// Link-Netz (Nutzerentscheidung 2026-10-14).
    tempo_zuletzt: f32,
    /// Zuletzt an Bild bzw. Rhythmus gereichter Wert (Rueckmeldung der Regler).
    bild_tempo_sync: f32,
    rhythm_tempo_sync: f32,
    /// Tempo-Teiler fuer Renoise-Port 7001 (wie rrn5 send_bpm; 1.0 = aus).
    tempo_teiler: f32,
    /// Laenge des WAV-Renders im Player-Tab (Sekunden).
    wav_sekunden: f64,
    last_peak: f32,
    last_report: Instant,
    /// S2b-Spike: Link-Session halten und regelmaessig ihren Zustand loggen.
    link: jamcore::rusty_link::AblLink,
    link_report: Instant,
    kamera_laeuft: bool,
    /// Kameraframes beim letzten Anzeigeframe und deren Zeitpunkt: die
    /// Pipeline markiert „Kamera läuft" daran (die Kamera kann auch im
    /// Bild-Tab starten).
    kamera_frames: u64,
    kamera_frame_zeit: Instant,
    /// Export-Bereich im Überblick (Datum/Quelle/Notiz, Order, Sichern).
    export: pipeline::ExportPanel,
    /// „Alle Dateien" (`MANAGE_EXTERNAL_STORAGE`) erteilt? Wird einmal pro
    /// Sekunde neu geprüft, damit die Anzeige nach dem Besuch der Systemseite
    /// umspringt (Nutzerentscheidung D2, 2026-10-16).
    ordner_erlaubt: bool,
    /// Nächste Prüfung von [`JamApp::ordner_erlaubt`].
    ordner_pruefung: Instant,
}

impl Default for JamApp {
    fn default() -> Self {
        JamApp {
            tab: Tab::Ueberblick,
            vollbild: false,
            status: "Bereit".to_string(),
            meldung: String::new(),
            insets_px: None,
            frames: 0,
            android_app: None,
            module_hint: String::new(),
            modules: Vec::new(),
            selected: None,
            browser: None,
            browser_audio: None,
            browser_bild: None,
            browser_video: None,
            browser_instr: None,
            instr_tausch: None,
            looping: true,
            player: None,
            instr: instr::InstrumentPanel::default(),
            osc: osc::OscPanel::default(),
            rhythm: rhythm::RhythmPanel::default(),
            slicer: slicer::SlicerPanel::default(),
            mic: mic::MicPanel::default(),
            bild: image::ImagePanel::default(),
            laub: laub::LaubPanel::default(),
            kreis: quint::QuintPanel::default(),
            lissa: lissa::LissaPanel::default(),
            session: session_ui::SessionPanel::default(),
            link_aktiv: true,
            multicast_aktiv: false,
            kombi_oben: KombiTeil::Bild,
            kombi_unten: KombiTeil::Rhythmus,
            diary: diary::DiaryPanel::default(),
            maske: maske::MaskePanel::default(),
            maske2: maske2::Maske2Panel::default(),
            maske_staerke: 0.5,
            tempo_bpm: 128.0,
            tempo_zuletzt: 128.0,
            bild_tempo_sync: 0.0,
            rhythm_tempo_sync: 0.0,
            tempo_teiler: 1.0,
            wav_sekunden: 60.0,
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
            kamera_frames: 0,
            kamera_frame_zeit: Instant::now(),
            export: pipeline::ExportPanel::new(),
            ordner_erlaubt: false,
            ordner_pruefung: Instant::now(),
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
            // Die echten Fenster-Insets zuerst; (0,0) heisst meist „Fenster
            // noch nicht angehaengt“ und wird nicht gecacht. Der Rueckfall auf
            // die Statusleistenhoehe laeuft **sofort** mit: frueher wartete er
            // 180 Frames, auf dem Pixel 4a lag die Tab-Leiste dadurch ~30 s
            // unter der Statusleiste (gemessen 2026-11-14).
            let found = insets::root_insets_px(&app)
                .filter(|px| px.0 > 0.0 || px.1 > 0.0)
                .or_else(|| insets::status_bar_px(&app));
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
                self.tempo_aus_modul();
            }
            Err(e) => {
                self.status = "Wiedergabe gestartet".to_string();
                self.meldung = format!("Fehler: {e}");
                log::info!(
                    "player: kein Modul geladen ({e}) - App-Tempo bleibt {:.0} BPM",
                    self.tempo_bpm
                );
            }
        }
    }

    /// Tempo und Speed aus dem Modulkopf auf die App-Uhr legen.
    ///
    /// Warum beim Laden: die App rechnet Raster, Rendern und Metronom in
    /// 1/16-Zeilen. Ein IT mit Speed 3 laeuft doppelt so schnell wie eines mit
    /// Speed 6; ohne Umrechnung liefen Rhythmus, Bild und Klick gegen das
    /// Modul. Geschrieben wird der zentrale `tempo_bpm` (wie jeder Regler),
    /// damit Link-Folgen und die Synchronisierung unveraendert bleiben.
    fn tempo_aus_modul(&mut self) {
        let Some(spieler) = self.player.as_ref() else {
            log::info!(
                "player: kein Modul geladen - App-Tempo bleibt {:.0} BPM",
                self.tempo_bpm
            );
            return;
        };
        let (tempo, speed) = spieler.it_tempo_werte();
        let Some(bpm) = jamcore::modul::bpm_aus_it(f64::from(tempo), f64::from(speed)) else {
            log::info!(
                "player: IT-Tempo {tempo} BPM, Speed {speed} ungueltig - App-Tempo bleibt {:.0} BPM",
                self.tempo_bpm
            );
            return;
        };
        self.tempo_setzen(bpm as f32);
        // Die Beispiele aus dem Auftrag: Speed 6 = wie im Kopf, Speed 3 doppelt.
        let doppelt = if (bpm - 2.0 * f64::from(tempo)).abs() < 0.5 {
            " (doppelt)"
        } else {
            ""
        };
        log::info!(
            "player: IT-Tempo {tempo} BPM, Speed {speed} -> App {:.0} BPM{doppelt}",
            self.tempo_bpm
        );
    }

    fn stop(&mut self) {
        if let Some(mut p) = self.player.take() {
            p.stop();
            self.status = "gestoppt".to_string();
        }
    }

    /// Ein Modul aus dem Datei-Browser laden und spielen.
    fn modul_laden(&mut self, path: PathBuf) {
        if !self.modules.contains(&path) {
            self.modules.push(path.clone());
            self.modules.sort();
            self.modules.dedup();
        }
        self.selected = self.modules.iter().position(|p| *p == path);
        self.play_selected();
    }
}

/// Anzeigetext für den Stand der „Alle Dateien"-Berechtigung (D2).
fn zugriff_text(erlaubt: bool) -> &'static str {
    if erlaubt {
        "erlaubt"
    } else {
        "nein"
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
        // Pipeline: neue Kameraframes bemerken, egal in welchem Tab die Kamera
        // gestartet wurde. Die Markierung „Kamera läuft" gilt eine Sekunde
        // nach dem letzten neuen Frame — ein abgestürzter Decoder bleibt so
        // nicht für immer grün.
        let (frames, _, _) = camera::stand();
        if frames != self.kamera_frames {
            let erster = self.kamera_frames == 0;
            self.kamera_frames = frames;
            self.kamera_frame_zeit = Instant::now();
            // Die ersten Bilder nach dem Start machen die Kamera zur aktuellen
            // Quelle (sonst stünde im Export noch das zuletzt geladene Bild).
            if erster {
                pipeline::quelle_setzen("kamera", "livebild");
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
        // Ordner-Zugriff (D2): den Stand einmal pro Sekunde neu holen — nach
        // dem Besuch der Systemseite springt die Anzeige im Überblick um.
        if self.ordner_pruefung.elapsed() >= std::time::Duration::from_secs(1) {
            self.ordner_pruefung = Instant::now();
            let erlaubt = match self.app() {
                Some(app) => ordner::alle_dateien_erlaubt(app),
                None => false,
            };
            if erlaubt != self.ordner_erlaubt {
                log::info!("Alle Dateien: {}", if erlaubt { "erlaubt" } else { "nein" });
            }
            self.ordner_erlaubt = erlaubt;
        }
        // Ohne Ereignisse zeichnet egui nicht neu; fuer den Spike-Takt (und
        // spaeter fuer Live-Anzeigen) mindestens einmal pro Sekunde.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));

        // Das gemerkte OSC-Ziel (Laptop-IP/Port) einmal laden.
        self.osc.laden();

        let px = self.insets_px();
        if px.is_none() {
            ui.ctx().request_repaint();
        }
        let ppp = ui.ctx().input(|i| i.pixels_per_point()).max(0.1);
        let top = px.map(|p| p.0 / ppp).unwrap_or(0.0);
        let bottom = px.map(|p| p.1 / ppp).unwrap_or(0.0);

        // Vollbild (Bild/B2): die Kopfzeile mit den Tabs ausblenden — der
        // Ausstieg steht als schwebender Knopf oben rechts.
        egui::Panel::top("kopf").show(ui, |ui| {
            ui.add_space(top);
            // Vollbild: nur der Statusleisten-Abstand bleibt, die Tabs gehen
            // aus (sonst läge der Inhalt unter Uhr/Cutout).
            if self.vollbild {
                return;
            }
            // Titel **und** Tabs in einer umbrechenden Zeile: die frühere
            // Verschachtelung („horizontal" um „horizontal_wrapped") ließ das
            // Panel nur zwei Zeilen hoch werden — die dritte Tab-Zeile ragte
            // in den Inhalt und fing dessen Klicks ab (auf dem Handy lagen
            // z. B. „Abspielen" und „Lissa" übereinander).
            ui.horizontal_wrapped(|ui| {
                ui.heading("JamCore");
                ui.selectable_value(&mut self.tab, Tab::Ueberblick, "Überblick");
                ui.selectable_value(&mut self.tab, Tab::Player, "Player");
                ui.selectable_value(&mut self.tab, Tab::Instr, "Instr");
                ui.selectable_value(&mut self.tab, Tab::Osc, "Tempo/OSC");
                ui.selectable_value(&mut self.tab, Tab::Rhythmus, "Rhythmus");
                ui.selectable_value(&mut self.tab, Tab::Bild, "Bild");
                ui.selectable_value(&mut self.tab, Tab::Laub, "Laub");
                ui.selectable_value(&mut self.tab, Tab::Kreis, "Kreis");
                ui.selectable_value(&mut self.tab, Tab::Maske, "Maske");
                ui.selectable_value(&mut self.tab, Tab::Bild2, "B2");
                ui.selectable_value(&mut self.tab, Tab::Lissa, "Lissa");
                ui.selectable_value(&mut self.tab, Tab::Slicer, "Slicer");
                ui.selectable_value(&mut self.tab, Tab::Kombi, "Kombi");
            });
            // Zusammengelegte Tabs (Nutzerentscheidung 2026-10-22): ein Eintrag
            // in der Leiste, die Umschaltung steht direkt darunter — so bleibt
            // die Leiste kurz und beide Ansichten sind einen Tipp entfernt.
            let umschalter = match self.tab {
                Tab::Ueberblick | Tab::Tagebuch => {
                    Some((Tab::Ueberblick, "Überblick", Tab::Tagebuch, "Tagebuch"))
                }
                Tab::Osc | Tab::Tempo => Some((Tab::Osc, "OSC", Tab::Tempo, "Tempo")),
                Tab::Maske | Tab::Maske2 => Some((Tab::Maske, "Maske", Tab::Maske2, "M2")),
                _ => None,
            };
            if let Some((links, links_text, rechts, rechts_text)) = umschalter {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.tab == links, links_text).clicked() {
                        self.tab = links;
                    }
                    if ui
                        .selectable_label(self.tab == rechts, rechts_text)
                        .clicked()
                    {
                        self.tab = rechts;
                    }
                });
            }
            // Vollbild für Bild/B2: mehr Platz, Kopfzeile geht aus.
            if matches!(self.tab, Tab::Bild | Tab::Bild2) {
                ui.horizontal(|ui| {
                    if ui
                        .button("⛶ Vollbild")
                        .on_hover_text("Kopfzeile ausblenden — mehr Platz fürs Bild")
                        .clicked()
                    {
                        self.vollbild = true;
                    }
                });
            }
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
        // Eine Uhr fuer alle: App-Tempo und Rhythmus-Feinheit in den
        // Transport spiegeln. Das passiert **jeden** Frame, auch wenn ein
        // anderer Tab sichtbar ist — laufende Erzeuger folgen so einer
        // Tempoaenderung, und der Bild-Scan tickt auch im Hintergrund.
        crate::transport::set_bpm(self.tempo_aktuell(), self.rhythm.feinheit());
        // Der Anhoeren-Thread im Laub-Tab soll beim Tabwechsel weiterklingen;
        // ohne Lebenszeichen beendet er sich nach zwei Sekunden ohne Frame.
        self.laub.lebenszeichen();
        if crate::transport::laeuft() {
            // Die Uhr laeuft: haeufiger zeichnen, damit versteckte Erzeuger
            // (Bild-Scan, Laub-Marker) und der Herzschlag aktuell bleiben.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        // Das Modul des Players an den Rhythmus-Renderer durchreichen
        // (Klangquelle „Modul-Samples").
        self.rhythm
            .set_modul_quelle(self.player.as_ref().map(|p| p.modul_quelle()));
        // Ein laufendes Laub-Video wird hier geprüft — nicht erst im Laub-Tab:
        // so läuft der Dekoder auch nach einem Tab-Wechsel nur seine kurze
        // Bilder-Reihe weiter und wird danach sicher gestoppt.
        self.laub.video_schritt(ui.ctx());
        // Der Bild-Scan klingt in jedem Tab: ist er angemeldet, aber sein Tab
        // nicht sichtbar (auch nicht B2/Kombi), tickt ihn die App hier weiter —
        // ohne Textur-Upload, nur fuer Kamerabild, Noten und Puffer.
        let bild_sichtbar = matches!(self.tab, Tab::Bild | Tab::Bild2)
            || (self.tab == Tab::Kombi
                && (self.kombi_oben == KombiTeil::Bild || self.kombi_unten == KombiTeil::Bild));
        if !bild_sichtbar
            && self.bild.scan_aktiv()
            && crate::transport::teil_aktiv(crate::transport::Teil::Bild)
        {
            self.bild
                .transport_schritt(ui.ctx(), self.rhythm.pattern_mut());
        }
        match self.tab {
            Tab::Ueberblick => self.view_ueberblick(ui),
            Tab::Player => self.view_player(ui),
            Tab::Instr => self.view_instr(ui),
            Tab::Osc => self.view_osc(ui),
            Tab::Rhythmus => self.view_rhythm(ui),
            Tab::Tempo => self.view_mic(ui),
            Tab::Bild => self.view_bild(ui),
            Tab::Laub => self.view_laub(ui),
            Tab::Kreis => self.view_kreis(ui),
            Tab::Tagebuch => self.view_tagebuch(ui),
            Tab::Maske => self.view_maske(ui),
            Tab::Maske2 => self.view_maske2(ui),
            Tab::Bild2 => self.view_bild2(ui),
            Tab::Lissa => self.view_lissa(ui),
            Tab::Slicer => self.view_slicer(ui),
            Tab::Kombi => self.view_kombi(ui),
        }
        // Vollbild-Ausstieg: schwebt oben rechts über dem Bild, damit man auch
        // ohne Kopfzeile jederzeit zurückkommt.
        if self.vollbild {
            egui::Area::new(egui::Id::new("vollbild_aus"))
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, top + 10.0))
                .show(ui.ctx(), |ui| {
                    if ui.button("⛶ aus").clicked() {
                        self.vollbild = false;
                    }
                });
        }
        // Der Dauerton des Lissa-Tabs klingt nur dort (sonst weiter im Hintergrund).
        if self.tab != Tab::Lissa {
            self.lissa.stoppen();
        }
        // Der Linien-/Bereichston des Oszillator-Scans klingt nur, wo das Bild
        // sichtbar ist (Bild, B2 oder Kombi mit Bild) — sonst bliebe er beim
        // Tab-Wechsel für immer stehen.
        if !bild_sichtbar {
            self.bild.linienton_aus();
        }

        // Datei-Browser des Player-Tabs („Öffnen"): Fenster über allem anderen.
        if let Some(mut browser) = self.browser.take() {
            let mut offen = true;
            let gewaehlt = browser.ui(ui.ctx(), &mut offen);
            if let Some(path) = gewaehlt {
                self.modul_laden(path);
                offen = false;
            }
            if offen {
                self.browser = Some(browser);
            }
        }

        // Datei-Browser des Slicer-Tabs („Audio öffnen"): liefert nur den Pfad
        // zurück, abgespielt wird nicht automatisch.
        if let Some(mut browser) = self.browser_audio.take() {
            let mut offen = true;
            let gewaehlt = browser.ui(ui.ctx(), &mut offen);
            if let Some(path) = gewaehlt {
                match self.slicer.laden(&path.display().to_string()) {
                    Ok(beschreibung) => {
                        self.status = format!("Audio: {} ({beschreibung})", file_name(&path));
                    }
                    Err(e) => self.status = format!("Audio-Fehler: {e}"),
                }
                offen = false;
            }
            if offen {
                self.browser_audio = Some(browser);
            }
        }

        // Datei-Browser des Laub-Tabs („Bild laden …"): der Pfad geht ins
        // Laub-Panel; das Bild landet auch im Kamera-Framepuffer (ein
        // Standbild für Bild- und Laub-Tab).
        if let Some(mut browser) = self.browser_bild.take() {
            let mut offen = true;
            let gewaehlt = browser.ui(ui.ctx(), &mut offen);
            if let Some(path) = gewaehlt {
                // Quelle für die .it-Meta-Zeilen merken („zu welchem Bild").
                pipeline::quelle_setzen("bild", &path.display().to_string());
                self.laub.laden(&path.display().to_string());
                offen = false;
            }
            if offen {
                self.browser_bild = Some(browser);
            }
        }

        // Datei-Browser des Laub-Tabs („Video laden …"): das Video wird
        // geprüft (bis zu 30 Bilder, das beste wird zum Standbild), der
        // Laub-Tab rendert danach unverändert weiter.
        if let Some(mut browser) = self.browser_video.take() {
            let mut offen = true;
            let gewaehlt = browser.ui(ui.ctx(), &mut offen);
            if let Some(path) = gewaehlt {
                self.status = format!("Video: {} — Bilder werden geprüft …", file_name(&path));
                pipeline::quelle_setzen("video", &path.display().to_string());
                self.laub.video_laden(&path.display().to_string());
                offen = false;
            }
            if offen {
                self.browser_video = Some(browser);
            }
        }

        // Datei-Browser des Instr-Tabs („Sample…"): die gewählte Audiodatei
        // ersetzt das Sample des Instruments. Die Ablage liegt im Slicer, der
        // Klangweg liest sie beim Spielen dieses Instruments.
        if let Some(mut browser) = self.browser_instr.take() {
            let mut offen = true;
            let gewaehlt = browser.ui(ui.ctx(), &mut offen);
            if let Some(path) = gewaehlt {
                if let Some(index) = self.instr_tausch.take() {
                    match instr::sample_tauschen(index, &path.display().to_string()) {
                        Ok(meldung) => self.status = meldung,
                        Err(e) => self.status = format!("Sample-Tausch: {e}"),
                    }
                }
                offen = false;
            }
            if offen {
                self.browser_instr = Some(browser);
            }
        }

        // Ein zentraler BPM-Wert: hat ihn irgendein Regler (oder die Messung)
        // bewegt, geht er ins Link-Netz (Nutzerentscheidung 2026-10-14).
        if (self.tempo_bpm - self.tempo_zuletzt).abs() > 0.01 {
            self.tempo_zuletzt = self.tempo_bpm;
            self.tempo_ins_link(self.tempo_bpm);
        }
        // Der Tongenerator rechnet Tonlaengen (Notenwert -> ms) mit diesem Tempo.
        crate::klang::bpm_setzen(self.tempo_bpm);
    }
}

impl JamApp {
    /// Das **eine** Tempo der App: mit aktivem Link-Follow das Session-Tempo,
    /// sonst der im OSC-Tab eingestellte Wert. Bild-Scan, Rendern und Tagebuch
    /// benutzen alle diese Zahl.
    fn tempo_aktuell(&self) -> f32 {
        // Den Link-Zustand **live** holen: der zwischengespeicherte Wert des
        // Session-Panels ist nur so frisch wie sein letzter Tab-Besuch, und
        // Bild/Rhythmus sollen dem Link-Tempo sofort folgen.
        let mut s = jamcore::rusty_link::SessionState::new();
        self.link.capture_app_session_state(&mut s);
        jamcore::session::tempo_gesamt(self.session.folgen(), s.tempo(), self.tempo_bpm)
    }

    /// Das eine BPM setzen (Regler in Rhythmus, Bild, OSC und die Messung im
    /// Tempo-Tab landen alle hier).
    fn tempo_setzen(&mut self, bpm: f32) {
        self.tempo_bpm = bpm.clamp(30.0, 300.0);
    }

    /// Einen Reglerzug ins Link-Netz schreiben: App-Zustand holen, Tempo
    /// setzen, zurueckgeben (rusty_link: capture/set_tempo/commit). Damit
    /// folgen andere Teilnehmer dem Regler (Nutzerentscheidung 2026-10-14);
    /// ohne Link schreibt es nur in die eigene Session (harmlos).
    fn tempo_ins_link(&mut self, bpm: f32) {
        let mut zustand = jamcore::rusty_link::SessionState::new();
        self.link.capture_app_session_state(&mut zustand);
        zustand.set_tempo(bpm as f64, 0);
        self.link.commit_app_session_state(&zustand);
        log::info!("Tempo {bpm:.1} ins Link geschrieben");
    }

    fn view_ueberblick(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            // Der Überblick ist die zentrale Seite (Kamera, Module, Pipeline,
            // Export) — auf dem Handy passt das nur mit Scrollen.
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("ueberblick_inhalt")
                .show(ui, |ui| {
                    ui.label(
                        "Fertig: Modul-Player, OSC zu Renoise, Rhythmus-Raster, Tempo-Messung, \
                         Bild/Scan, Laub, Slicer, Tagebuch. Die Pipeline unten zeigt, welche \
                         Stufe gerade läuft.",
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
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Module neu suchen").clicked() {
                            self.scan_modules();
                        }
                        // „Alle Dateien" (D2): Systemseite für dieses Paket öffnen.
                        if ui.button("Ordner-Zugriff erlauben").clicked() {
                            let ergebnis = match self.app() {
                                Some(app) => ordner::einstellungen_oeffnen(app),
                                None => Err("keine Activity".to_string()),
                            };
                            match ergebnis {
                                Ok(()) => {
                                    self.status =
                                        "Systemseite für Alle Dateien geöffnet".to_string()
                                }
                                Err(e) => self.meldung = format!("Ordner-Zugriff: {e}"),
                            }
                        }
                        let stand = zugriff_text(self.ordner_erlaubt);
                        ui.label(format!("Alle Dateien: {stand}"));
                    });
                    if ui.button("Zum Player").clicked() {
                        self.tab = Tab::Player;
                        if self.modules.is_empty() {
                            self.scan_modules();
                        }
                    }

                    // ==== Pipeline: farbige Stufen mit Live-Markierung ====
                    ui.separator();
                    let kamera_aktiv = self.kamera_frame_zeit.elapsed()
                        < std::time::Duration::from_secs(1);
                    let video_aktiv = self.laub.video_laeuft();
                    let live = pipeline::Live {
                        kamera: kamera_aktiv || self.kamera_laeuft,
                        video: video_aktiv,
                        // Erkennung läuft nur, solange der Laub-Tab offen ist
                        // und Bilder ankommen (dort rechnet `ui` je Frame).
                        laub: self.tab == Tab::Laub && (kamera_aktiv || video_aktiv),
                        // „Rendern" gehört zum Rhythmus-Tab; einen öffentlichen
                        // Zustand gibt es dort nicht, der offene Tab ist der
                        // sichtbare Hinweis.
                        rhythmus: self.tab == Tab::Rhythmus,
                        export: self.export.export_live(),
                    };
                    egui::CollapsingHeader::new("Pipeline")
                        .default_open(true)
                        .id_salt("pipeline_kopf")
                        .show(ui, |ui| {
                            pipeline::ui_pipeline(ui, &live);
                        });

                    // ==== Export: Raster + Slices als .it mit Song-Message ====
                    egui::CollapsingHeader::new("Export (.it mit Meta)")
                        .default_open(true)
                        .id_salt("export_kopf")
                        .show(ui, |ui| {
                            let rhythmus = self.rhythm.pattern().clone();
                            let bild = self.bild.abschnitt_pattern(rhythmus.steps_per_cycle());
                            let aktion = self.export.ui(ui, &rhythmus, &bild);
                            if aktion == pipeline::Aktion::Sichern {
                                match self.export.sichern(&rhythmus) {
                                    Ok(pfad) => {
                                        self.status = format!("IT gesichert: {pfad}");
                                    }
                                    Err(e) => {
                                        pipeline::export_fehler(&e);
                                        self.meldung = format!("IT-Export: {e}");
                                        self.status = format!("IT-Export: {e}");
                                    }
                                }
                            }
                        });
                });
        });
    }

    fn view_maske(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            // Scrollen nur ueber die Leiste: jede Geste im Feld ist ein Tipp
            // oder ein Wisch (auch ein langsamer). Frueher entschied die
            // Gestendauer, was ein Wisch ist — langsame Wische fielen weg
            // (Vorgabe des Nutzers 2026-10-11).
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("maske_inhalt")
                .show(ui, |ui| self.maske_bereich(ui));
        });
    }

    /// Maskenfeld mit der Folge-Zeile. Die Aufzeichnung kommt aus dem
    /// Maske-Tab (Zeitabstaende + Feldwerte); das Bild rechnet daraus die
    /// gruene Maske ueber dem Kamerabild.
    fn maske_bereich(&mut self, ui: &mut egui::Ui) {
        let laeuft = self.maske.laeuft();
        let anzahl = self.maske.folge().len();
        ui.horizontal_wrapped(|ui| {
            // „neu" loescht die Folge und nimmt wieder auf. Ohne Ende-Knopf:
            // die Aufnahme laeuft einfach weiter, bis man neu drueckt.
            if ui
                .add(egui::Button::new("neu").min_size(egui::vec2(96.0, 44.0)))
                .clicked()
            {
                self.maske.neu();
            }
            let text = if laeuft {
                format!("{anzahl} Zeitpunkte — nimmt auf")
            } else if anzahl > 0 {
                format!("{anzahl} Zeitpunkte — festgehalten")
            } else {
                "Feld tippen: nimmt auf".to_string()
            };
            ui.label(egui::RichText::new(text).color(if laeuft {
                egui::Color32::from_rgb(120, 230, 160)
            } else {
                egui::Color32::GRAY
            }));
        });
        // Wie stark die Wischgeschwindigkeit die Schrittweite kippt. Bei 0
        // wirkt nur die normale Rechnung.
        ui.horizontal(|ui| {
            ui.label("Wisch-Modulation");
            ui.add(
                egui::Slider::new(&mut self.maske_staerke, 0.0..=1.0)
                    .text("stark")
                    .show_value(true),
            );
        });
        self.maske.ui(ui, self.rhythm.pattern_mut());
        // Der Bild-Tab bekommt die Zeitpunkte der **zuletzt benutzten** Maske
        // (Maske oder Maske 2) — beide schreiben in dieselbe gruene Maske.
        self.bild_maske_aktualisieren();
    }

    /// Die Zeitpunkte der zuletzt benutzten Maske in die Bild-Maske legen.
    fn bild_maske_aktualisieren(&mut self) {
        let eins = self.maske.letzte_aenderung();
        let zwei = self.maske2.letzte_aenderung();
        let (zeiten, tempo) = match (eins, zwei) {
            (Some(a), Some(b)) if b > a => (
                self.maske2.zeiten(),
                self.maske2
                    .folge()
                    .iter()
                    .flat_map(|a| a.tempo.iter().copied())
                    .collect::<Vec<f32>>(),
            ),
            (None, Some(_)) => (
                self.maske2.zeiten(),
                self.maske2
                    .folge()
                    .iter()
                    .flat_map(|a| a.tempo.iter().copied())
                    .collect::<Vec<f32>>(),
            ),
            _ => (
                self.maske.zeiten(),
                self.maske
                    .folge()
                    .iter()
                    .flat_map(|a| a.tempo.iter().copied())
                    .collect::<Vec<f32>>(),
            ),
        };
        if zeiten.is_empty() {
            return;
        }
        let staerke = self.maske_staerke;
        let schritte = self.rhythm.pattern().steps_per_cycle();
        self.bild
            .maske_aus_zeiten(&zeiten, &tempo, staerke, schritte);
    }

    fn view_maske2(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("maske2_inhalt")
                .show(ui, |ui| {
                    self.maske2.ui(ui, self.rhythm.pattern_mut());
                    self.bild_maske_aktualisieren();
                });
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
                let effektiv = self.tempo_aktuell();
                if (effektiv - self.bild_tempo_sync).abs() > 0.01 {
                    self.bild.set_bpm(effektiv);
                    self.bild_tempo_sync = effektiv;
                }
                // Bild → Kreis: derselbe x-Bereich (geteilter Wert).
                let (von, bis) = self.bild.x_bereich();
                self.kreis.setze_x_bereich(von, bis);
                self.bild.ui(ui, self.rhythm.pattern_mut());
                let nach = self.bild.scan_bpm();
                if (nach - effektiv).abs() > 0.01 {
                    self.tempo_setzen(nach);                    self.bild_tempo_sync = self.tempo_bpm;
                }
            }
            KombiTeil::Rhythmus => {
                let tempo = self.tempo_aktuell();
                self.rhythm.set_tempo(tempo);
                if (self.tempo_bpm - self.rhythm_tempo_sync).abs() > 0.01 {
                    self.rhythm.set_eigenuhr(self.tempo_bpm);
                    self.rhythm_tempo_sync = self.tempo_bpm;
                }
                self.rhythm.ui(ui);
                let nach = self.rhythm.eigenuhr_bpm();
                if (nach - self.tempo_bpm).abs() > 0.01 {
                    self.tempo_setzen(nach);
                    self.rhythm_tempo_sync = self.tempo_bpm;
                }
            }
            KombiTeil::Maske => self.maske_bereich(ui),
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
                if let Some(klassen) = self.kreis.ui(ui) {
                    self.bild
                        .harmonie_band(klassen, self.bild.aktuelle_tipp_spalte());
                }
            }
        }
    }

    fn view_bild2(&mut self, ui: &mut egui::Ui) {
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.bild.set_target(&host, &port);
        // Gleiche Klangquelle und gleiches Tempo wie im Bild-Tab — es ist
        // derselbe Panel-Zustand, nur das Layout ist anders.
        let (quelle, welle, modul) = self.rhythm.klang_zustand();
        self.bild.set_klang_zustand(quelle, welle, modul);
        let effektiv = self.tempo_aktuell();
        if (effektiv - self.bild_tempo_sync).abs() > 0.01 {
            self.bild.set_bpm(effektiv);
            self.bild_tempo_sync = effektiv;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            self.bild.ui_vollbild(ui, self.rhythm.pattern_mut());
        });
        // Bild (B2 nutzt denselben Zustand) → Kreis: geteilter x-Bereich.
        let (von, bis) = self.bild.x_bereich();
        self.kreis.setze_x_bereich(von, bis);
        let nach = self.bild.scan_bpm();
        if (nach - effektiv).abs() > 0.01 {
            self.tempo_setzen(nach);
            self.bild_tempo_sync = self.tempo_bpm;
        }
        // „Tempo aus Video" auch in B2 (es ist derselbe Bild-Zustand).
        if let Some(bpm) = self.bild.uebernommenes_tempo() {
            self.tempo_setzen(bpm);
            self.bild_tempo_sync = self.tempo_bpm;
            self.status = format!("Tempo aus Video: {:.1} BPM", self.tempo_bpm);
            log::info!("Tempo aus Video übernommen: {:.1} BPM", self.tempo_bpm);
        }
    }

    fn view_lissa(&mut self, ui: &mut egui::Ui) {
        let bpm = self.tempo_aktuell();
        egui::CentralPanel::default().show(ui, |ui| {
            self.lissa.ui(ui, bpm);
        });
    }

    /// Slicer-Tab: WAV/MP3/… öffnen oder aufnehmen, schneiden, vorhören und
    /// als .it sichern. Der Datei-Browser wird von hier angefordert und von
    /// `ui()` als Fenster gezeigt (die App hat nur einen Ort für Fenster).
    fn view_slicer(&mut self, ui: &mut egui::Ui) {
        let mut aktion = slicer::Aktion::Keine;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("slicer_inhalt")
                .show(ui, |ui| {
                    aktion = self.slicer.ui(ui);
                });
        });
        if aktion == slicer::Aktion::BrowserOeffnen {
            // App-eigene Ordner zuerst (audio/recordings/wav gehen immer),
            // mit „Alle Dateien" zusätzlich Music/Download/Documents.
            let start = match self.app() {
                Some(app) => browser::startordner(app, browser::Filter::Audio),
                None => Vec::new(),
            };
            self.browser_audio = Some(browser::Browser::neu_audio(start));
            self.status = "Audio wählen …".to_string();
        }
    }

    fn view_tagebuch(&mut self, ui: &mut egui::Ui) {
        // Das eine Tempo (Follow gewinnt) statt der letzten Mikrofonmessung.
        let bpm = Some(self.tempo_aktuell());
        let noten = self.rhythm.pattern().clone();
        // Der Bild-Export liegt zuklappbar **über** dem Tagebuch: darunter füllt
        // die Einträge-Liste die ganze Fläche, sonst wäre er unsichtbar.
        let abschnitt = self
            .bild
            .abschnitt_pattern(self.rhythm.pattern().steps_per_cycle());
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
        let mut uebernehmen = None;
        egui::CentralPanel::default().show(ui, |ui| {
            uebernehmen = self.kreis.ui(ui);
            kreis_presets::ui(ui, &mut self.kreis);
        });
        if let Some(klassen) = uebernehmen {
            self.bild
                .harmonie_band(klassen, self.bild.aktuelle_tipp_spalte());
        }
        // Der x-Bereich des Kreises gilt auch für den Bild-Scan (geteilter Wert).
        let (von, bis) = self.kreis.x_bereich();
        self.bild.setze_x_bereich(von, bis);
    }

    fn view_bild(&mut self, ui: &mut egui::Ui) {
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.bild.set_target(&host, &port);
        // Der Bild-Scan klingt mit derselben Quelle wie das Pattern: ein
        // Schalter im Rhythmus-Tab gilt fuer Bild UND Rhythmus. Ohne das war
        // der Scan still, weil er nur OSC an den Laptop schickte.
        let (quelle, welle, modul) = self.rhythm.klang_zustand();
        self.bild.set_klang_zustand(quelle, welle, modul);
        // Das eine Tempo (Follow gewinnt) — der Takt-Scan rechnet daraus die
        // Sweep-Dauer. Der BPM-Regler im Bild-Tab schreibt zurueck in die App.
        let effektiv = self.tempo_aktuell();
        if (effektiv - self.bild_tempo_sync).abs() > 0.01 {
            self.bild.set_bpm(effektiv);
            self.bild_tempo_sync = effektiv;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            // Das Bild darf Scan-Treffer ins Rhythmus-Pattern schreiben
            // (Bild-Follow, Desktop `L`/bpmo3).
            self.bild.ui(ui, self.rhythm.pattern_mut());
        });
        // Bild → Kreis: derselbe x-Bereich (geteilter Wert).
        let (von, bis) = self.bild.x_bereich();
        self.kreis.setze_x_bereich(von, bis);
        let nach = self.bild.scan_bpm();
        if (nach - effektiv).abs() > 0.01 {
            self.tempo_setzen(nach);
            self.bild_tempo_sync = self.tempo_bpm;
        }
        // „Tempo aus Video" (Knopf in der Kamera-Zeile): das erkannte Tempo
        // wird das eine BPM der App. Erst nach dem Scan-Rückkanal, damit es
        // nicht gleich wieder von einem alten Reglerwert überschrieben wird.
        if let Some(bpm) = self.bild.uebernommenes_tempo() {
            self.tempo_setzen(bpm);
            self.bild_tempo_sync = self.tempo_bpm;
            self.status = format!("Tempo aus Video: {:.1} BPM", self.tempo_bpm);
            log::info!("Tempo aus Video übernommen: {:.1} BPM", self.tempo_bpm);
        }
    }

    /// Laub-Tab: Foto/Bild → Blätter → Abstände → Genre-Rhythmus. Das Rendern
    /// schreibt über [`rhythm::RhythmPanel::laub_setzen`] ins Raster, deshalb
    /// bekommt das Panel das Rhythmus-Panel durchgereicht.
    fn view_laub(&mut self, ui: &mut egui::Ui) {
        // Das eine Tempo der App (der absolute Modus rechnet damit).
        let tempo = self.tempo_aktuell();
        self.laub.set_tempo(tempo);
        let mut aktion = laub::Aktion::Keine;
        egui::CentralPanel::default().show(ui, |ui| {
            // Scrollen nur über die Leiste: jede Geste im Bild ist ein Tipp
            // (Hintergrundfarbe wählen), kein Scrollen.
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("laub_inhalt")
                .show(ui, |ui| {
                    aktion = self.laub.ui(ui, &mut self.rhythm);
                });
        });
        if aktion == laub::Aktion::BrowserBild {
            let start = match self.app() {
                Some(app) => browser::startordner(app, browser::Filter::Bild),
                None => Vec::new(),
            };
            self.browser_bild = Some(browser::Browser::neu_bild(start));
            self.status = "Bild wählen …".to_string();
        }
        if aktion == laub::Aktion::BrowserVideo {
            let start = match self.app() {
                Some(app) => browser::startordner(app, browser::Filter::Video),
                None => Vec::new(),
            };
            self.browser_video = Some(browser::Browser::neu_video(start));
            self.status = "Video wählen …".to_string();
        }
    }

    fn view_mic(&mut self, ui: &mut egui::Ui) {
        // Das Ziel steht einmal im OSC-Tab; der Tempo-Tab bekommt es hier
        // durchgereicht (sonst müsste die IP zweimal eingetippt werden).
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.mic.set_target(&host, &port);
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("tempo_inhalt")
                .show(ui, |ui| {
                    self.mic.ui(ui);
                    ui.separator();
                    // Link/Session gehoeren zum selben Tab wie die Messung
                    // (Nutzerentscheidung 2026-10-14).
                    self.session_block(ui);
                });
        });
    }

    fn view_rhythm(&mut self, ui: &mut egui::Ui) {
        let (host, port) = (self.osc.host.clone(), self.osc.port.clone());
        self.rhythm.set_target(&host, &port);
        // Rendern benutzt dasselbe Tempo wie der Takt-Scan. Der Eigenuhr-
        // Regler im Rhythmus-Tab schreibt zurueck in die App (ein BPM-Wert).
        let tempo = self.tempo_aktuell();
        self.rhythm.set_tempo(tempo);
        // Den App-Wert in die Eigenuhr spiegeln, wenn er sich anderswo
        // geaendert hat; die Anzeige folgt damit dem einen Wert.
        if (self.tempo_bpm - self.rhythm_tempo_sync).abs() > 0.01 {
            self.rhythm.set_eigenuhr(self.tempo_bpm);
            self.rhythm_tempo_sync = self.tempo_bpm;
        }
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
        // Der Eigenuhr-Regler schreibt zurueck in das eine BPM der App.
        let nach = self.rhythm.eigenuhr_bpm();
        if (nach - self.tempo_bpm).abs() > 0.01 {
            self.tempo_setzen(nach);
            self.rhythm_tempo_sync = self.tempo_bpm;
            self.status = format!("Tempo: {:.0} BPM", self.tempo_bpm);
            log::info!("Tempo aus dem Rhythmus: {:.1} BPM", self.tempo_bpm);
        }
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

    /// Rendert das geladene Modul als WAV in den App-Ordner `wav/` (Desktop:
    /// `it_live --wav`). Stoppt dafuer die Wiedergabe — der Renderer braucht das
    /// Modul exklusiv.
    fn wav_sichern(&mut self) {
        let Some(app) = self.app() else {
            self.meldung = "Keine Activity".to_string();
            return;
        };
        let Some(basis) = insets::external_files_dir(app) else {
            self.meldung = "Kein App-Ordner".to_string();
            return;
        };
        let name = self
            .player
            .as_ref()
            .map(|p| {
                std::path::Path::new(p.pfad())
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "modul".to_string())
            })
            .unwrap_or_else(|| "modul".to_string());
        let ordner = std::path::PathBuf::from(basis).join("wav");
        let _ = std::fs::create_dir_all(&ordner);
        let pfad = ordner.join(format!("{name}.wav"));
        let sekunden = self.wav_sekunden;
        let Some(player) = self.player.as_mut() else {
            self.meldung = "Kein Modul geladen".to_string();
            return;
        };
        self.status = format!("Rendere {sekunden:.0} s WAV …");
        match player.wav_schreiben(&pfad.display().to_string(), sekunden) {
            Ok(frames) => {
                self.status = format!("WAV: {} ({} Frames)", pfad.display(), frames);
            }
            Err(e) => self.meldung = format!("WAV-Fehler: {e}"),
        }
    }

    fn view_instr(&mut self, ui: &mut egui::Ui) {
        // Das Modul des Players an die Zuordnung reichen (Klon der Quelle).
        let quelle = self.player.as_ref().map(|p| p.modul_quelle());
        let mut aktion = instr::Aktion::Keine;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR)
                .id_salt("instr_inhalt")
                .show(ui, |ui| aktion = self.instr.ui(ui, quelle.as_ref()));
        });
        // „Sample…": Audiodatei wählen; nach der Wahl ersetzt sie das Sample
        // dieses Instruments (Ablage im Slicer, Klangweg liest sie).
        if let instr::Aktion::SampleWaehlen(index) = aktion {
            let start = match self.app() {
                Some(app) => browser::startordner(app, browser::Filter::Audio),
                None => Vec::new(),
            };
            self.instr_tausch = Some(index);
            self.browser_instr = Some(browser::Browser::neu_audio(start));
            self.status = format!(
                "Instrument {}: Audiodatei für den Sample-Tausch wählen …",
                index + 1
            );
            log::info!(
                "instr: Sample-Auswahl fuer Instrument {} geoeffnet",
                index + 1
            );
        }
    }

    fn view_player(&mut self, ui: &mut egui::Ui) {
        // Werte laufen im Audio-Thread, also regelmaessig neu zeichnen.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Neu suchen").clicked() {
                    self.scan_modules();
                }
                if ui.button("Öffnen …").clicked() {
                    // App-eigene Ordner zuerst (module/… gehen immer), mit
                    // „Alle Dateien" zusätzlich Download/Documents.
                    let start = match self.app() {
                        Some(app) => browser::startordner(app, browser::Filter::Module),
                        None => Vec::new(),
                    };
                    self.browser = Some(browser::Browser::neu(start));
                }
                if ui.button("Abspielen").clicked() {
                    self.play_selected();
                }
                if ui.button("Stopp").clicked() {
                    self.stop();
                }
                ui.checkbox(&mut self.looping, "Dauerschleife");
            });
            // WAV-Render wie `it_live --wav` (Stoppt die Wiedergabe).
            ui.horizontal(|ui| {
                ui.label("WAV");
                ui.add(
                    egui::DragValue::new(&mut self.wav_sekunden)
                        .range(5.0..=600.0)
                        .suffix(" s"),
                );
                if ui.button("WAV sichern").clicked() {
                    self.wav_sichern();
                }
            });
            // Akkord-Zeile aus den gerade klingenden Noten des Moduls.
            let noten = self
                .player
                .as_ref()
                .map(|p| p.modul_quelle().klingende_noten())
                .unwrap_or_default();
            crate::akkord::zeile(ui, &noten);
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
                // Struktur-Dump wie `it_live --dump` (Orders mit Pattern und
                // Zeilen; `<BREAK>` = 0xFFFE).
                let struktur = player.struktur(48);
                if !struktur.is_empty() {
                    ui.collapsing("Struktur", |ui| {
                        for zeile in &struktur {
                            ui.label(zeile);
                        }
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
    // App-eigene Ordner anlegen und den Stand von „Alle Dateien" merken
    // (D2, 2026-10-16) — Fehler landen nur im Log.
    ordner::app_ordner_anlegen(&app);
    jam_app.ordner_erlaubt = ordner::alle_dateien_erlaubt(&app);
    log::info!("Alle Dateien: {}", zugriff_text(jam_app.ordner_erlaubt));
    let _ = APP.set(Mutex::new(Some(app)));
    jam_app.scan_modules();
    let _ = eframe::run_native("JamCore", options, Box::new(|_cc| Ok(Box::new(jam_app))));
    log::info!("beendet");
}
