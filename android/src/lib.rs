//! JamCore-Android-App: egui-Oberfläche mit Modul-Player (AudioTrack + libopenmpt).
//! Weitere Module (Rhythmus, Bild/Scan, Session, Tagebuch) folgen in S4+.

mod insets;
mod jniutil;
mod paths;
mod player;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Ueberblick,
    Player,
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
    last_peak: f32,
    last_report: Instant,
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
            last_peak: 0.0,
            last_report: Instant::now(),
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
                log::info!("jamandroid: systemleisten {px:?}px");
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
            "jamandroid: {} Modul(e) in {} Verzeichnis(s)",
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
                ui.selectable_value(&mut self.tab, Tab::Ueberblick, "Überblick");
                ui.selectable_value(&mut self.tab, Tab::Player, "Player");
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

        match self.tab {
            Tab::Ueberblick => self.view_ueberblick(ui),
            Tab::Player => self.view_player(ui),
        }
    }
}

impl JamApp {
    fn view_ueberblick(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Schritte: Rhythmus aufnehmen (S6), Bild/Scan (S7), Session (S9), Tagebuch (S10).");
            ui.separator();
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

            egui::ScrollArea::vertical().show(ui, |ui| {
                let modules = self.modules.clone();
                for (index, path) in modules.iter().enumerate() {
                    let selected = self.selected == Some(index);
                    if ui
                        .selectable_label(selected, file_name(path))
                        .clicked()
                    {
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
                    self.last_peak,
                    pos,
                    order,
                    row,
                    frames
                ));
                if self.last_report.elapsed().as_secs_f32() > 2.0 {
                    self.last_report = Instant::now();
                    log::info!(
                        "jamandroid: rms peak={:.4} pos={:.1}s order={} row={} frames={}",
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
    log::info!("jamandroid: android_main gestartet");
    let options = eframe::NativeOptions {
        android_app: Some(app.clone()),
        ..Default::default()
    };
    let mut jam_app = JamApp::default();
    jam_app.android_app = Some(app.clone());
    let _ = APP.set(Mutex::new(Some(app)));
    jam_app.scan_modules();
    let _ = eframe::run_native("JamCore", options, Box::new(|_cc| Ok(Box::new(jam_app))));
    log::info!("jamandroid: beendet");
}
