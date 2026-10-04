//! Android-App-Gerüst (S2): startet eine egui-Oberfläche und loggt nach logcat.
//! Funktionen (Player, Rhythmus, Bild/Scan, Session, Tagebuch) folgen in S3+.

mod insets;

use eframe::egui;

#[derive(Clone, Copy, Default)]
struct Insets {
    top: f32,
    bottom: f32,
}

struct JamApp {
    status: String,
    insets_px: Option<insets::InsetsPx>,
    frames: u32,
    #[cfg(target_os = "android")]
    android_app: Option<android_activity::AndroidApp>,
}

impl Default for JamApp {
    fn default() -> Self {
        JamApp {
            status: "Bereit (S2-Gerüst)".to_string(),
            insets_px: None,
            frames: 0,
            #[cfg(target_os = "android")]
            android_app: None,
        }
    }
}

impl JamApp {
    #[cfg(target_os = "android")]
    fn insets_px(&mut self) -> Option<insets::InsetsPx> {
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
    fn insets_px(&mut self) -> Option<insets::InsetsPx> {
        self.insets_px
    }
}

impl eframe::App for JamApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let px = self.insets_px();
        if px.is_none() {
            ui.ctx().request_repaint();
        }
        let ppp = ui.ctx().input(|i| i.pixels_per_point()).max(0.1);
        let insets = Insets {
            top: px.map(|p| p.0 / ppp).unwrap_or(0.0),
            bottom: px.map(|p| p.1 / ppp).unwrap_or(0.0),
        };

        egui::Panel::top("kopf").show(ui, |ui| {
            ui.add_space(insets.top);
            ui.horizontal(|ui| {
                ui.heading("JamCore");
                ui.label("· Rhythmus · Bild/Scan · Modul · Session · Tagebuch");
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(&self.status);
            ui.separator();
            if ui.button("Link-Session-Status").clicked() {
                self.status = "S2: Gerüst laeuft, Module folgen".to_string();
                log::info!("jamandroid: button gedrueckt");
            }
            ui.add_space(insets.bottom);
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
    jam_app.android_app = Some(app);
    let _ = eframe::run_native("JamCore", options, Box::new(|_cc| Ok(Box::new(jam_app))));
    log::info!("jamandroid: beendet");
}
