//! Rhythmus-Screen: Noten-Raster, das auf Module-Takt oder eigene Uhr läuft.
//!
//! Semantik: Das Raster ist ein **Step-Editor**, Spalte = Schritt, Zeile = Ton.
//! Ein Tipp setzt genau die angefasste Stelle; der laufende Cursor (Follow oder
//! eigene Uhr) zeigt nur, wo das Modul gerade ist. Ein Aufnahme-Modus, der
//! jeden Tipp auf dem aktuellen Schritt landen lässt, ist bewusst nicht gebaut
//! (offener Planpunkt, siehe docs/plans/2026-09-30-android-port.md, S6).
//!
//! Der Austausch mit Renoise läuft über `jamcore::grid` (CSV wie im Desktop-
//! Recorder, Clipboard-XML für den Import) und über OSC fürs sofortige Hören.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Grid, RichText, Sense};
use jamcore::grid::{note_name, Pattern, STEPS_PER_CYCLE as STEP_COUNT};
use jamcore::osc::{OscOut, Renoise, DEFAULT_BASE_PORT};
use log::info;

const ROWS: [i32; 4] = [36, 48, 60, 72];
const CELL: f32 = 22.0;
const BLOCK: u16 = 8;
const DEFAULT_BPM: f32 = 128.0;

pub struct RhythmPanel {
    pattern: Pattern,
    target: (String, u16),
    velocity: u8,
    bpm: f32,
    follow: bool,
    current_step: u16,
    clock: Instant,
    status: String,
    dir: Option<String>,
    /// "Rendern" (wie Taste o im Desktop-Recorder): Raster abspielen.
    render_laeuft: bool,
    render_stopp: Option<Arc<AtomicBool>>,
    render_schritt: Arc<AtomicU16>,
    render_thread: Option<std::thread::JoinHandle<()>>,
    render_bpm: f32,
    transport_mit: bool,
}

impl Default for RhythmPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RhythmPanel {
    pub fn new() -> Self {
        Self {
            pattern: Pattern::new(),
            target: ("127.0.0.1".to_string(), 7003),
            velocity: 100,
            bpm: DEFAULT_BPM,
            follow: false,
            current_step: 0,
            clock: Instant::now(),
            status: "Raster leer. Pad tippen legt eine Note auf den aktuellen Schritt.".to_string(),
            dir: None,
            render_laeuft: false,
            render_stopp: None,
            render_schritt: Arc::new(AtomicU16::new(0)),
            render_thread: None,
            render_bpm: DEFAULT_BPM,
            transport_mit: false,
        }
    }

    /// Host/Port des OSC-Tabs übernehmen, damit beide dasselbe Ziel senden.
    pub fn set_target(&mut self, host: &str, port: &str) {
        // Ungueltige Eingabe: gleicher Standard wie der OSC-Tab, sonst senden
        // Rhythmus und OSC-Tab an verschiedene Ports.
        self.target = (
            host.trim().to_string(),
            port.trim().parse().unwrap_or(DEFAULT_BASE_PORT),
        );
    }

    pub fn follow(&self) -> bool {
        self.follow
    }

    /// Aktuelles Raster (fuer das Tagebuch und spaetere Exporte).
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// Schritt aus dem laufenden Modul (Follow-Modus).
    pub fn set_follow_step(&mut self, step: u16) {
        self.current_step = step % STEP_COUNT;
    }

    /// Eigene Uhr, wenn der Follow-Modus aus ist.
    /// "Rendern" wie die Taste o im Desktop-Recorder: spielt das Raster in
    /// Schleife im 16tel-Takt, sendet je Note `note_on` per OSC und lässt sie
    /// lokal klingen. Optional startet/stoppt es den Renoise-Transport.
    fn rendern_starten(&mut self) {
        if self.render_laeuft {
            return;
        }
        let noten: Vec<(u16, i32)> = self
            .pattern
            .notes()
            .iter()
            .map(|n| (n.step, n.pitch))
            .collect();
        if noten.is_empty() {
            self.status = "Raster ist leer - nichts zu rendern.".to_string();
            return;
        }
        let anzahl = noten.len();
        let (host, port) = self.target.clone();
        let bpm = self.render_bpm.max(20.0);
        let transport = self.transport_mit;
        let stopp = Arc::new(AtomicBool::new(false));
        let schritt = Arc::new(AtomicU16::new(0));
        let stopp_c = Arc::clone(&stopp);
        let schritt_c = Arc::clone(&schritt);
        let thread = std::thread::Builder::new()
            .name("jam-render".to_string())
            .spawn(move || {
                let out = OscOut::new(&host, port).ok();
                if transport {
                    if let Some(o) = out.as_ref() {
                        let _ = Renoise::new(o).transport_start(7000);
                    }
                }
                let mut i: u16 = 0;
                while !stopp_c.load(Ordering::Relaxed) {
                    let start = Instant::now();
                    for (s, pitch) in &noten {
                        if *s == i {
                            crate::klang::spielen(*pitch);
                            if let Some(o) = out.as_ref() {
                                let _ = Renoise::new(o).note_on(7000, *pitch, 100);
                            }
                        }
                    }
                    schritt_c.store(i, Ordering::Relaxed);
                    i = (i + 1) % STEP_COUNT;
                    // 16tel bei bpm: 60/bpm/4 Sekunden je Schritt.
                    let ziel = (60_000.0 / bpm / 4.0) as u64;
                    let rest = ziel.saturating_sub(start.elapsed().as_millis() as u64);
                    std::thread::sleep(Duration::from_millis(rest));
                }
                if transport {
                    if let Some(o) = out.as_ref() {
                        let _ = Renoise::new(o).transport_stop(7000);
                    }
                }
            });
        match thread {
            Ok(thread) => {
                self.render_thread = Some(thread);
                self.render_stopp = Some(stopp);
                self.render_schritt = schritt;
                self.render_laeuft = true;
                self.status = format!("Rendern läuft ({anzahl} Noten, {bpm:.0} BPM)");
                info!("rhythmus: Rendern gestartet ({anzahl} Noten)");
            }
            Err(e) => self.status = format!("Rendern-Thread: {e}"),
        }
    }

    fn rendern_stoppen(&mut self) {
        if let Some(stopp) = self.render_stopp.take() {
            stopp.store(true, Ordering::Relaxed);
        }
        if let Some(thread) = self.render_thread.take() {
            let _ = thread.join();
        }
        self.render_laeuft = false;
        self.status = "Rendern gestoppt.".to_string();
        info!("rhythmus: Rendern gestoppt");
    }

    fn tick_own_clock(&mut self) {
        // Beim Rendern zeigt der Playhead die laufende Abspielposition.
        if self.render_laeuft {
            self.current_step = self.render_schritt.load(Ordering::Relaxed) % STEP_COUNT;
            return;
        }
        if self.follow {
            return;
        }
        let elapsed = self.clock.elapsed().as_secs_f32();
        let beats = elapsed * self.bpm.max(20.0) / 60.0;
        // Modulo vor dem Cast: ein `as u16` wuerde nach zwei Stunden
        // sattigieren und die Uhr einfrieren.
        let steps = (beats * 4.0).max(0.0);
        self.current_step = if steps.is_finite() {
            (steps as i64 % STEP_COUNT as i64) as u16
        } else {
            0
        };
    }

    fn play(&mut self, pitch: i32, velocity: u8) {
        let (host, port) = self.target.clone();
        let result = OscOut::new(&host, port)
            .and_then(|out| Renoise::new(&out).note_on(7000, pitch, velocity as i32));
        match result {
            Ok(used) => info!(
                "Note {} vel {} -> {}:{}",
                note_name(pitch),
                velocity,
                host,
                used
            ),
            Err(e) => {
                self.status = format!("OSC-Fehler: {e}");
                info!("OSC-Fehler: {e}");
            }
        }
    }

    fn toggle_pad(&mut self, step: u16, pitch: i32) {
        let added = self.pattern.toggle(step, pitch);
        if added {
            self.play(pitch, self.velocity);
            // Der Ton klingt auch lokal (im Demo-Video hörbar).
            crate::klang::spielen(pitch);
            self.status = format!(
                "Schritt {}: {} ({} Noten)",
                step + 1,
                note_name(pitch),
                self.pattern.notes().len()
            );
        } else {
            self.status = format!(
                "Schritt {} frei ({} Noten)",
                step + 1,
                self.pattern.notes().len()
            );
        }
    }

    /// Musterordner im externen App-Verzeichnis (ohne Berechtigung, per adb erreichbar).
    fn pattern_dir(&mut self) -> Option<String> {
        if self.dir.is_none() {
            let app = crate::current_app().ok()?;
            let base = crate::insets::external_files_dir(&app)?;
            let dir = std::path::PathBuf::from(base).join("patterns");
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Pattern-Ordner {}: {e}", dir.display()))
                .ok()?;
            self.dir = Some(dir.display().to_string());
        }
        self.dir.clone()
    }

    fn save_csv(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis für Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join("muster.csv");
        let csv = self.pattern.to_csv();
        match self.write_atomic(&path, &csv) {
            Ok(()) => {
                self.status = format!(
                    "{} Noten -> {} ({} Bytes)",
                    self.pattern.notes().len(),
                    path.display(),
                    csv.len()
                );
                info!("CSV gesichert: {}", path.display());
            }
            Err(e) => self.status = format!("CSV-Fehler: {e}"),
        }
    }

    fn save_xml(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis für Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join("muster.xml");
        let xml = self.pattern.to_renoise_xml();
        match self.write_atomic(&path, &xml) {
            Ok(()) => {
                self.status = format!("XML für Renoise -> {}", path.display());
                info!("XML geschrieben: {}", path.display());
            }
            Err(e) => self.status = format!("XML-Fehler: {e}"),
        }
    }

    /// Neben die Zieldatei schreiben und dann umbenennen: bricht das Schreiben
    /// ab, bleibt das vorherige Muster unangetastet.
    fn write_atomic(&self, path: &std::path::Path, content: &str) -> std::io::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, content)?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn load_csv(&mut self) {
        let Some(dir) = self.pattern_dir() else {
            self.status = "Kein App-Verzeichnis fuer Patterns.".to_string();
            return;
        };
        let path = std::path::Path::new(&dir).join("muster.csv");
        match std::fs::read_to_string(&path) {
            Ok(content) => match Pattern::from_csv(&content, STEP_COUNT) {
                Ok(loaded) => {
                    self.pattern = loaded;
                    self.status = format!(
                        "{} Noten aus {}",
                        self.pattern.notes().len(),
                        path.display()
                    );
                    info!("CSV geladen: {}", path.display());
                }
                Err(e) => self.status = format!("CSV kaputt: {e}"),
            },
            Err(e) => self.status = format!("CSV fehlt: {e}"),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.tick_own_clock();
        ui.ctx().request_repaint_after(Duration::from_millis(50));

        ui.horizontal(|ui| {
            ui.label(format!("Schritt {}/{}", self.current_step + 1, STEP_COUNT));
            ui.label(format!("Noten: {}", self.pattern.notes().len()));
            if self.follow {
                ui.label(RichText::new("Follow: Modul").strong());
            } else {
                ui.label(format!("Eigenuhr: {:.0} BPM", self.bpm));
            }
        });
        ui.add(
            egui::Slider::new(&mut self.bpm, 40.0..=220.0)
                .text("BPM")
                .show_value(true),
        );
        ui.horizontal(|ui| {
            if ui.button("Follow").clicked() {
                self.follow = !self.follow;
                self.clock = Instant::now();
                info!("Follow = {}", self.follow);
            }
            if ui.button("Leeren").clicked() {
                self.pattern.clear();
                self.status = "Raster geleert.".to_string();
            }
            if ui.button("Vel -").clicked() {
                self.velocity = self.velocity.saturating_sub(10).max(1);
            }
            ui.label(format!("Vel {}", self.velocity));
            if ui.button("Vel +").clicked() {
                self.velocity = (self.velocity + 10).min(127);
            }
        });
        ui.horizontal(|ui| {
            // "Rendern" wie Taste o im Desktop-Recorder: Raster abspielen.
            if self.render_laeuft {
                if ui.button("Rendern stoppen").clicked() {
                    self.rendern_stoppen();
                }
                ui.label(
                    RichText::new(format!("läuft, Schritt {}", self.current_step + 1))
                        .strong()
                        .color(Color32::from_rgb(90, 200, 120)),
                );
            } else if ui.button("Rendern").clicked() {
                self.rendern_starten();
            }
            ui.label("BPM");
            ui.add(egui::Slider::new(&mut self.render_bpm, 40.0..=220.0));
            ui.checkbox(&mut self.transport_mit, "Transport");
        });

        // 16 Spalten passen nicht nebeneinander auf die Handy-Breite, deshalb
        // zwei Bloecke a 8 Schritte mit grossen Zellen zum Tippen.
        for block in 0..2 {
            Grid::new(format!("rhythm_block{block}"))
                .spacing([2.0, 2.0])
                .show(ui, |ui| {
                    let titel = if block == 0 { "1-8" } else { "9-16" };
                    ui.label(RichText::new(titel).small().color(Color32::GRAY));
                    for i in 0..BLOCK {
                        let step = block * BLOCK + i;
                        if i % 2 == 0 {
                            ui.label(
                                RichText::new(format!("{}", step + 1))
                                    .small()
                                    .color(Color32::GRAY),
                            );
                        } else {
                            _ = ui.allocate_painter(egui::vec2(CELL, 12.0), Sense::hover());
                        }
                    }
                    ui.end_row();
                    for pitch in ROWS {
                        ui.label(RichText::new(note_name(pitch)).small());
                        for i in 0..BLOCK {
                            let step = block * BLOCK + i;
                            let on = self.pattern.has_at(step, pitch);
                            let played = step == self.current_step;
                            let color = if on {
                                Color32::from_rgb(90, 200, 120)
                            } else if played {
                                Color32::from_rgb(115, 125, 155)
                            } else {
                                Color32::from_rgb(45, 48, 58)
                            };
                            let (cell, painter) =
                                ui.allocate_painter(egui::vec2(CELL, CELL), Sense::click());
                            painter.rect_filled(cell.rect, 3.0, color);
                            if on {
                                painter.rect_stroke(
                                    cell.rect,
                                    egui::CornerRadius::same(3),
                                    egui::Stroke::new(1.0, Color32::from_rgb(150, 240, 170)),
                                    egui::StrokeKind::Middle,
                                );
                            }
                            // Der Taktcursor ist ein heller Rahmen, damit er auch
                            // auf einer belegten Zelle sichtbar bleibt.
                            if played {
                                painter.rect_stroke(
                                    cell.rect,
                                    egui::CornerRadius::same(3),
                                    egui::Stroke::new(2.0, Color32::from_rgb(245, 245, 255)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if cell.clicked() {
                                self.toggle_pad(step, pitch);
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
        }

        ui.horizontal(|ui| {
            if ui.button("CSV sichern").clicked() {
                self.save_csv();
            }
            if ui.button("CSV laden").clicked() {
                self.load_csv();
            }
            if ui.button("XML schreiben").clicked() {
                self.save_xml();
            }
        });
        ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
        ui.label(
            RichText::new("CSV (Desktop-Format) und Clipboard-XML liegen im App-Ordner patterns/.")
                .small()
                .color(Color32::DARK_GRAY),
        );
    }
}
