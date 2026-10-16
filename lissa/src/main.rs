// lissa – Desktop-Programm: Lissajous-Figur mit Phasen-Drehung im Takt, dazu
// die zwei Stimmen der Figur als Dauerton.
//
// Die Figur kippt wie im Max-Patch (`lissa-2020optic.amxd`): der Phasenversatz
// läuft mit dem Takt — als Sägezahn (Original, harter Rücksprung = Doppel-
// Moment), weich, oder endlos in eine Richtung (links/rechts). Die Mathematik
// liegt im Kern (`jamcore::lissa`, `jamcore::takte`, `jamcore::klang`); hier
// stehen Fenster (SDL2), Tastatur, Zeitbasis und Tongenerator.

use jamcore::lissa::{self, Modus};
use sdl2::audio::{AudioCallback, AudioSpecDesired};
use sdl2::event::{Event, EventType};
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Point;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use std::f64::consts::TAU;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Vorgaben und Grenzen
// ---------------------------------------------------------------------------
/// Abtastrate des Tongenerators (wie pattern_recorder).
const RATE: u32 = 44_100;
/// Lautstärke der zwei Stimmen (beide zusammen höchstens ~0,44 Vollausschlag).
const PEGEL: f32 = 0.22;
/// Vorgabe-Tempo (Taste `b`/`n` ändert es).
const VORGABE_BPM: f64 = 128.0;
const BPM_MIN: f64 = 30.0;
const BPM_MAX: f64 = 300.0;
/// Grundton-Vorgabe: A-3 (220 Hz) — mit 2:3 klingen 440/660 Hz.
const VORGABE_GRUNDTON: i32 = 57;
const GRUNDTON_MIN: i32 = 24;
const GRUNDTON_MAX: i32 = 96;
/// Takte pro Zyklus, wenn nichts anderes eingestellt ist (ein Schwung = 2 Takte).
const VORGABE_TAKTE: f64 = 2.0;
/// Verhältnis-Grenzen (1 … 16 wie im Android-Panel).
const VERHAELTNIS_MIN: u32 = 1;
const VERHAELTNIS_MAX: u32 = 16;
/// So viele Punkte hat die Figur (die Kurve wird geschlossen).
const FIGUR_PUNKTE: usize = 512;
/// Fenstergröße, wenn kein `--size WxH` angegeben ist.
const VORGABE_FENSTER: (i32, i32) = (640, 480);
/// Dunkler Hintergrund, Figur als helle Linie.
const HINTERGRUND: Color = Color::RGB(14, 14, 18);
const FIGUR_FARBE: Color = Color::RGB(232, 232, 242);

/// Verhältnis-Presets der Tasten `1`–`6`.
const PRESETS: [(u32, u32); 6] = [
    (2, 2), // Kreis
    (2, 3), // Unendlich
    (1, 2),
    (3, 2),
    (3, 4),
    (4, 5),
];

/// Tastenbelegung (Startmeldung und `--help`).
const TASTEN: &str = "Tasten: x/y Achse wählen · −/+ Verhältnis · ←/→ X, ↑/↓ Y · 1–6 Presets \
(2:2, 2:3, 1:2, 3:2, 3:4, 4:5) · m Sägezahn/Weich · l/r endlos links/rechts · \
,/. Umdrehungen ÷2/×2 · [/] Takte ÷2/×2 · t Ton an/aus · z/h Grundton · b/n BPM · Esc Ende";

// ---------------------------------------------------------------------------
// Zustand (Tastatur, Zeitbasis, Anzeige)
// ---------------------------------------------------------------------------
/// Welche Achse die Tasten `−`/`+` ändern.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Achse {
    X,
    Y,
}

struct Zustand {
    a: u32,
    b: u32,
    achse: Achse,
    umdrehungen: f64,
    /// Endlos drehen (Reglerstufe „∞") statt Faltung/weicher Umkehr?
    endlos: bool,
    /// Richtung der endlosen Drehung: false = links, true = rechts.
    richtung_rechts: bool,
    /// Faltung oder weiche Umkehr (gilt, solange nicht endlos).
    modus: Modus,
    /// Takte pro Dreh-Zyklus (`jamcore::takte`-Bereich).
    takte: f64,
    /// Ton an/aus (die zwei Stimmen der Figur).
    ton_an: bool,
    /// Grundton als MIDI-Note.
    grundton: i32,
    /// Tempo in Schlägen je Minute.
    bpm: f64,
    /// Taktzähler wird fortgeschrieben (Tempoänderungen machen keinen Sprung).
    takte_seit_start: f64,
    letzte_zeit: Instant,
}

impl Zustand {
    fn new() -> Self {
        Self {
            a: 2,
            b: 3,
            achse: Achse::X,
            umdrehungen: 1.0,
            endlos: false,
            richtung_rechts: true,
            modus: Modus::Weich,
            takte: VORGABE_TAKTE,
            ton_an: false,
            grundton: VORGABE_GRUNDTON,
            bpm: VORGABE_BPM,
            takte_seit_start: 0.0,
            letzte_zeit: Instant::now(),
        }
    }

    /// Wirksamer Dreh-Modus: endlos (Richtung) oder Faltung/weich.
    fn wirksamer_modus(&self) -> Modus {
        if self.endlos {
            if self.richtung_rechts {
                Modus::UnendlichRechts
            } else {
                Modus::UnendlichLinks
            }
        } else {
            self.modus
        }
    }

    /// Wie die Phase läuft (Anzeige).
    fn drehung_text(&self) -> &'static str {
        if self.endlos {
            if self.richtung_rechts {
                "∞ rechts"
            } else {
                "∞ links"
            }
        } else {
            match self.modus {
                Modus::Saege => "Sägezahn",
                Modus::Weich => "weich",
                _ => "endlos",
            }
        }
    }

    /// Eine Taste verarbeiten; `true` heißt: Programm beenden.
    fn taste(&mut self, kc: Keycode) -> bool {
        match kc {
            Keycode::Escape => return true,
            // Achse für `−`/`+` wählen.
            Keycode::X => self.achse = Achse::X,
            Keycode::Y => self.achse = Achse::Y,
            // Verhältnis: `−`/`+` auf der gewählten Achse, Pfeile direkt.
            Keycode::Minus | Keycode::KpMinus => self.setze_verhaeltnis(self.achse, -1),
            Keycode::Plus | Keycode::Equals | Keycode::KpPlus => {
                self.setze_verhaeltnis(self.achse, 1)
            }
            Keycode::Left => self.setze_verhaeltnis(Achse::X, -1),
            Keycode::Right => self.setze_verhaeltnis(Achse::X, 1),
            Keycode::Down => self.setze_verhaeltnis(Achse::Y, -1),
            Keycode::Up => self.setze_verhaeltnis(Achse::Y, 1),
            // Presets 1–6.
            Keycode::Num1 => self.preset(0),
            Keycode::Num2 => self.preset(1),
            Keycode::Num3 => self.preset(2),
            Keycode::Num4 => self.preset(3),
            Keycode::Num5 => self.preset(4),
            Keycode::Num6 => self.preset(5),
            // Charakter: Sägezahn (Original) / weich; beendet den ∞-Modus.
            Keycode::M => {
                self.endlos = false;
                self.modus = if self.modus == Modus::Saege {
                    Modus::Weich
                } else {
                    Modus::Saege
                };
            }
            // Endlos drehen; erneut gedrückt hebt den ∞-Modus wieder auf.
            Keycode::L => {
                if self.endlos && !self.richtung_rechts {
                    self.endlos = false;
                } else {
                    self.endlos = true;
                    self.richtung_rechts = false;
                }
            }
            Keycode::R => {
                if self.endlos && self.richtung_rechts {
                    self.endlos = false;
                } else {
                    self.endlos = true;
                    self.richtung_rechts = true;
                }
            }
            // Umdrehungen und Takte je Zyklus wie die Faderknöpfe (÷2/×2).
            Keycode::Comma => self.umdrehungen = lissa::teile_umdrehungen(self.umdrehungen, 2.0),
            Keycode::Period => {
                self.umdrehungen = lissa::vielfache_umdrehungen(self.umdrehungen, 2.0)
            }
            Keycode::LeftBracket => self.takte = jamcore::takte::teile(self.takte, 2.0),
            Keycode::RightBracket => self.takte = jamcore::takte::vielfache(self.takte, 2.0),
            // Ton und Grundton.
            Keycode::T => self.ton_an = !self.ton_an,
            Keycode::Z => {
                self.grundton = (self.grundton - 1).clamp(GRUNDTON_MIN, GRUNDTON_MAX);
            }
            Keycode::H => {
                self.grundton = (self.grundton + 1).clamp(GRUNDTON_MIN, GRUNDTON_MAX);
            }
            // Tempo.
            Keycode::B => self.bpm = (self.bpm - 1.0).clamp(BPM_MIN, BPM_MAX),
            Keycode::N => self.bpm = (self.bpm + 1.0).clamp(BPM_MIN, BPM_MAX),
            _ => {}
        }
        false
    }

    /// Verhältnis einer Achse um `schritt` ändern und auf 1…16 klemmen.
    fn setze_verhaeltnis(&mut self, achse: Achse, schritt: i32) {
        let wert = match achse {
            Achse::X => &mut self.a,
            Achse::Y => &mut self.b,
        };
        let neu = *wert as i32 + schritt;
        *wert = neu.clamp(VERHAELTNIS_MIN as i32, VERHAELTNIS_MAX as i32) as u32;
    }

    /// Ein Preset setzen (Index in `PRESETS`).
    fn preset(&mut self, index: usize) {
        if let Some((a, b)) = PRESETS.get(index) {
            self.a = *a;
            self.b = *b;
        }
    }

    /// Takte seit dem Start fortschreiben (Tempo wirkt nur auf die Rate, nicht
    /// auf die Vergangenheit — sonst springt die Figur bei jedem Tempo-Wechsel).
    fn takte_fortschreiben(&mut self, jetzt: Instant) {
        let dt = jetzt.duration_since(self.letzte_zeit).as_secs_f64();
        self.letzte_zeit = jetzt;
        if dt.is_finite() && dt > 0.0 {
            self.takte_seit_start += dt * self.bpm / 60.0;
        }
    }

    /// Zustandszeile (ohne Phase) für Fenstertitel und Konsole.
    fn status(&self) -> String {
        format!(
            "{}:{} · {} · Zyklus {} · {} · {:.0} BPM · Ton {} · Grundton {} ({:.0} Hz)",
            self.a,
            self.b,
            self.drehung_text(),
            jamcore::takte::text(self.takte),
            lissa::text_umdrehungen(self.umdrehungen),
            self.bpm,
            if self.ton_an { "an" } else { "aus" },
            jamcore::grid::note_name(self.grundton),
            jamcore::klang::frequenz(self.grundton),
        )
    }

    /// Titelzeile: Status plus aktuelle Phase.
    fn titel(&self, phi: f64) -> String {
        format!("Lissa — {} · φ {:+.0}°", self.status(), phi.to_degrees())
    }
}

// ---------------------------------------------------------------------------
// Tongenerator: die zwei Stimmen der Figur (X mit dem Versatz der Drehung)
// ---------------------------------------------------------------------------
/// Die Werte, die das Hauptprogramm dem Audio-Callback übergibt (Atomics: der
/// Callback läuft in einem eigenen Thread).
#[derive(Clone)]
struct TonRegler {
    an: Arc<AtomicBool>,
    /// Frequenzen der beiden Stimmen in Hz (als f32-Bits).
    freqs: [Arc<AtomicU32>; 2],
}

impl TonRegler {
    fn neu() -> Self {
        Self {
            an: Arc::new(AtomicBool::new(false)),
            freqs: [
                Arc::new(AtomicU32::new(0.0f32.to_bits())),
                Arc::new(AtomicU32::new(0.0f32.to_bits())),
            ],
        }
    }

    /// Ton an/aus und die zwei Frequenzen setzen (vom Hauptprogramm je Bild).
    fn setzen(&self, an: bool, freqs: [f32; 2]) {
        self.an.store(an, Ordering::Relaxed);
        for (slot, f) in self.freqs.iter().zip(freqs) {
            slot.store(f.to_bits(), Ordering::Relaxed);
        }
    }

    /// Die zuletzt gesetzten Frequenzen.
    fn frequenzen(&self) -> [f32; 2] {
        [
            f32::from_bits(self.freqs[0].load(Ordering::Relaxed)),
            f32::from_bits(self.freqs[1].load(Ordering::Relaxed)),
        ]
    }
}

/// Tongenerator: die zwei Stimmen der Figur laufen als Dauerton; die Phasen
/// laufen über die Blöcke weiter (`jamcore::lissa::dauerton_mischen`).
struct LissaTon {
    regler: TonRegler,
    /// Abtastrate des offenen Geräts (nicht die gewünschte).
    rate: u32,
    phasen: [f64; 2],
    /// Nachgezogene Frequenzen — Sprünge beim Umschalten klingen sonst hart.
    glatt: [f32; 2],
    /// Lautstärkehülle 0…PEGEL: kurze Rampe statt Klicken beim Ein-/Ausschalten.
    huelle: f32,
}

impl LissaTon {
    fn neu(regler: &TonRegler, rate: i32) -> Self {
        Self {
            regler: regler.clone(),
            rate: if rate > 0 { rate as u32 } else { RATE },
            phasen: [0.0; 2],
            glatt: [0.0; 2],
            huelle: 0.0,
        }
    }
}

impl AudioCallback for LissaTon {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        let an = self.regler.an.load(Ordering::Relaxed);
        // Frequenzen weich nachziehen (ein Pol je Block): Sprünge beim
        // Umschalten klingen sonst hart.
        for (i, slot) in self.regler.freqs.iter().enumerate() {
            let ziel = f32::from_bits(slot.load(Ordering::Relaxed));
            if ziel.is_finite() && ziel >= 0.0 {
                self.glatt[i] += (ziel - self.glatt[i]) * 0.25;
            }
        }
        // Die zwei Stimmen addieren; die Phasen laufen über die Blöcke weiter.
        out.fill(0);
        lissa::dauerton_mischen(out, &mut self.phasen, self.glatt, self.rate, PEGEL);
        // Lautstärkehülle über den Block rampen (kein Klicken beim Umschalten).
        let ziel = if an { PEGEL } else { 0.0 };
        let schritt = (ziel - self.huelle) / out.len().max(1) as f32;
        for s in out.iter_mut() {
            self.huelle = (self.huelle + schritt).clamp(0.0, PEGEL);
            *s = (*s as f32 * self.huelle / PEGEL) as i16;
        }
        self.huelle = ziel;
    }
}

// ---------------------------------------------------------------------------
// Zeichnen in einen RGBA-Pixelpuffer (wie rrn5: CPU-Puffer, dann Texture)
// ---------------------------------------------------------------------------
/// Ein Pixel setzen; außerhalb des Bildes passiert nichts.
fn setze_pixel(puffer: &mut [u8], xw: i32, yh: i32, x: i32, y: i32, farbe: Color) {
    if x < 0 || y < 0 || x >= xw || y >= yh {
        return;
    }
    let idx = ((y * xw + x) * 4) as usize;
    if idx + 3 >= puffer.len() {
        return;
    }
    puffer[idx] = farbe.r;
    puffer[idx + 1] = farbe.g;
    puffer[idx + 2] = farbe.b;
    puffer[idx + 3] = farbe.a;
}

/// Den ganzen Puffer mit einer Farbe füllen.
fn fuellen(puffer: &mut [u8], farbe: Color) {
    for px in puffer.chunks_exact_mut(4) {
        px[0] = farbe.r;
        px[1] = farbe.g;
        px[2] = farbe.b;
        px[3] = farbe.a;
    }
}

/// Linie in den Puffer zeichnen: so viele Schritte wie die längere Achse, je
/// Schritt höchstens ein Pixel Versatz — die Linie hat keine Lücken.
fn linie(puffer: &mut [u8], xw: i32, yh: i32, von: Point, nach: Point, farbe: Color) {
    let dx = nach.x() - von.x();
    let dy = nach.y() - von.y();
    let schritte = dx.abs().max(dy.abs());
    if schritte == 0 {
        setze_pixel(puffer, xw, yh, von.x(), von.y(), farbe);
        return;
    }
    for i in 0..=schritte {
        let t = i as f32 / schritte as f32;
        let x = von.x() + (dx as f32 * t).round() as i32;
        let y = von.y() + (dy as f32 * t).round() as i32;
        setze_pixel(puffer, xw, yh, x, y, farbe);
    }
}

/// Bildschirmpunkte der Figur: Mitte des Fensters, Radius = 44 % der kürzeren
/// Seite, y nach oben gespiegelt (wie im Android-Panel).
fn figur_bildpunkte(xw: i32, yh: i32, a: u32, b: u32, phi: f64) -> Vec<Point> {
    let mitte_x = xw as f32 / 2.0;
    let mitte_y = yh as f32 / 2.0;
    let radius = xw.min(yh) as f32 * 0.44;
    lissa::punkte(a as f64, b as f64, phi, FIGUR_PUNKTE)
        .into_iter()
        .map(|(x, y)| {
            Point::new(
                (mitte_x + x as f32 * radius) as i32,
                (mitte_y - y as f32 * radius) as i32,
            )
        })
        .collect()
}

/// Die Figur in den Puffer zeichnen: dunkler Hintergrund, Kurve als Linie
/// (geschlossen, wie im Android-Panel).
fn figur_in_puffer(puffer: &mut [u8], xw: i32, yh: i32, a: u32, b: u32, phi: f64) {
    fuellen(puffer, HINTERGRUND);
    let mut vorher: Option<Point> = None;
    let mut erster = Point::new(0, 0);
    for p in figur_bildpunkte(xw, yh, a, b, phi) {
        match vorher {
            Some(v) => linie(puffer, xw, yh, v, p, FIGUR_FARBE),
            None => erster = p,
        }
        vorher = Some(p);
    }
    // Kurve schließen (θ endet einen Schritt vor 2π).
    if let Some(v) = vorher {
        linie(puffer, xw, yh, v, erster, FIGUR_FARBE);
    }
}

/// Fensterbild: CPU-Puffer (RGBA) plus Streaming-Texture; gezeichnet wird im
/// Puffer, die Texture lädt ihn als Ganzes hoch (wie in rrn5).
struct Bild {
    canvas: Canvas<Window>,
    tex: Texture,
    #[allow(dead_code)]
    _creator: TextureCreator<WindowContext>,
    puffer: Vec<u8>,
    xw: i32,
    yh: i32,
}

impl Bild {
    fn neu(canvas: Canvas<Window>, xw: i32, yh: i32) -> Result<Self, String> {
        let creator = canvas.texture_creator();
        let tex = creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, xw as u32, yh as u32)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            canvas,
            tex,
            _creator: creator,
            puffer: vec![0u8; (xw.max(0) * yh.max(0)) as usize * 4],
            xw,
            yh,
        })
    }

    /// Die Figur zeichnen und anzeigen.
    fn figur(&mut self, a: u32, b: u32, phi: f64) -> Result<(), String> {
        figur_in_puffer(&mut self.puffer, self.xw, self.yh, a, b, phi);
        self.present()
    }

    /// Puffer hochladen und anzeigen.
    fn present(&mut self) -> Result<(), String> {
        let Bild {
            canvas,
            tex,
            puffer,
            xw,
            yh,
            ..
        } = self;
        let row = *xw as usize * 4;
        tex.with_lock(None, |dst, pitch| {
            for y in 0..*yh as usize {
                let von = y * row;
                dst[y * pitch..y * pitch + row].copy_from_slice(&puffer[von..von + row]);
            }
        })?;
        canvas.copy(tex, None, None)?;
        canvas.present();
        Ok(())
    }

    /// Puffer als PPM (P6) schreiben — Verifikation über `LISSA_DUMP`.
    fn als_ppm(&self, pfad: &str) -> Result<(), String> {
        let mut daten = format!("P6\n{} {}\n255\n", self.xw, self.yh).into_bytes();
        for px in self.puffer.chunks_exact(4) {
            daten.extend_from_slice(&px[..3]);
        }
        std::fs::write(pfad, daten).map_err(|e| e.to_string())
    }

    /// Fenstertitel setzen (Anzeige des Zustands).
    fn titel_setzen(&mut self, titel: &str) -> Result<(), String> {
        self.canvas
            .window_mut()
            .set_title(titel)
            .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Argumente
// ---------------------------------------------------------------------------
/// "640x480" parsen (min. 1x1).
fn parse_size(spec: &str) -> Option<(i32, i32)> {
    let (w, h) = spec.split_once(['x', 'X'])?;
    let w: i32 = w.trim().parse().ok()?;
    let h: i32 = h.trim().parse().ok()?;
    Some((w.max(1), h.max(1)))
}

/// Größe aus `--size WxH` (Vorgabe 640x480, siehe `VORGABE_FENSTER`).
fn resolve_size(args: &[String]) -> Option<(i32, i32)> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--size" {
            return it.next().and_then(|v| parse_size(v));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Ereignisse (SDL2-compat leakt SDL3-Events; nur relevante Typen weiterreichen,
// sonst panict sdl2 0.37 bei Event::from_ll)
// ---------------------------------------------------------------------------
fn is_kept_event(t: u32) -> bool {
    matches!(
        t,
        x if x == EventType::Quit as u32
            || x == EventType::KeyDown as u32
            || x == EventType::KeyUp as u32
            || x == EventType::MouseButtonDown as u32
            || x == EventType::MouseButtonUp as u32
    )
}

fn poll_event_compat() -> Option<Event> {
    unsafe {
        loop {
            let mut raw = std::mem::MaybeUninit::uninit();
            if sdl2::sys::SDL_PollEvent(raw.as_mut_ptr()) != 1 {
                return None;
            }
            let ev = raw.assume_init();
            if is_kept_event(ev.type_) {
                return Some(Event::from_ll(ev));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test-Hook: E2E_KEYS="t,3,comma,quit" beim Start in die SDL-Queue legen
// (wie rrn5/pattern_recorder, damit E2E-Skripte Tasten drücken können).
// ---------------------------------------------------------------------------
fn inject_test_keys() {
    let Ok(spec) = std::env::var("E2E_KEYS") else {
        return;
    };
    std::thread::spawn(move || push_test_keys(&spec));
}

fn push_test_keys(spec: &str) {
    use sdl2::keyboard::Keycode as K;
    for name in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(ms) = name.strip_prefix("wait:") {
            std::thread::sleep(Duration::from_millis(ms.parse().unwrap_or(0)));
            continue;
        }
        if name.eq_ignore_ascii_case("quit") {
            unsafe {
                let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
                raw.type_ = sdl2::sys::SDL_EventType::SDL_QUIT as u32;
                sdl2::sys::SDL_PushEvent(&mut raw);
            }
            continue;
        }
        let k = match name.to_ascii_lowercase().as_str() {
            "minus" => K::Minus,
            "plus" => K::Plus,
            "comma" => K::Comma,
            "period" => K::Period,
            "leftbracket" => K::LeftBracket,
            "rightbracket" => K::RightBracket,
            "escape" => K::Escape,
            "left" => K::Left,
            "right" => K::Right,
            "up" => K::Up,
            "down" => K::Down,
            "0" => K::Num0,
            "1" => K::Num1,
            "2" => K::Num2,
            "3" => K::Num3,
            "4" => K::Num4,
            "5" => K::Num5,
            "6" => K::Num6,
            "b" => K::B,
            "h" => K::H,
            "l" => K::L,
            "m" => K::M,
            "n" => K::N,
            "r" => K::R,
            "t" => K::T,
            "x" => K::X,
            "y" => K::Y,
            "z" => K::Z,
            _ => continue,
        };
        unsafe {
            let mut raw = std::mem::zeroed::<sdl2::sys::SDL_Event>();
            raw.key.type_ = sdl2::sys::SDL_EventType::SDL_KEYDOWN as u32;
            raw.key.state = 1;
            raw.key.keysym.sym = k.into_i32();
            raw.key.keysym.scancode = sdl2::sys::SDL_GetScancodeFromKey(k.into_i32());
            sdl2::sys::SDL_PushEvent(&mut raw);
        }
    }
}

// ---------------------------------------------------------------------------
// Hauptprogramm
// ---------------------------------------------------------------------------
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("lissa — Lissajous-Figur mit Phasen-Drehung im Takt und zwei Stimmen.");
        println!(
            "Aufruf: lissa [--size WxH]   (Vorgabe {}x{})",
            VORGABE_FENSTER.0, VORGABE_FENSTER.1
        );
        println!("{TASTEN}");
        return;
    }
    let (xw, yh) = resolve_size(&args).unwrap_or(VORGABE_FENSTER);

    let sdl = match sdl2::init() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SDL-Start fehlgeschlagen: {e}");
            std::process::exit(1);
        }
    };
    let video = match sdl.video() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Video-Subsystem fehlt: {e}");
            std::process::exit(1);
        }
    };
    let fenster = match video.window("Lissa", xw as u32, yh as u32).build() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Fenster konnte nicht geöffnet werden: {e}");
            std::process::exit(1);
        }
    };
    let canvas = match fenster.into_canvas().accelerated().build() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Renderer fehlt: {e}");
            std::process::exit(1);
        }
    };
    let mut bild = match Bild::neu(canvas, xw, yh) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Textur fehlt: {e}");
            std::process::exit(1);
        }
    };

    // Ton: die zwei Stimmen der Figur. Ohne Audio läuft die Figur trotzdem.
    let regler = TonRegler::neu();
    let ton_device = match sdl.audio() {
        Ok(audio) => {
            let desired = AudioSpecDesired {
                freq: Some(RATE as i32),
                channels: Some(1),
                samples: Some(1024),
            };
            match audio.open_playback(None, &desired, |spec| LissaTon::neu(&regler, spec.freq)) {
                Ok(dev) => {
                    dev.resume();
                    println!("Tongenerator: {RATE} Hz, mono");
                    Some(dev)
                }
                Err(e) => {
                    eprintln!("Tongenerator nicht verfügbar: {e}");
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("Audio nicht verfügbar: {e}");
            None
        }
    };

    println!("Lissa — Fenster {xw}x{yh}");
    println!("{TASTEN}");

    // Verifikation: `LISSA_DUMP=<pfad>` schreibt das erste Bild als PPM und
    // beendet das Programm (wie die E2E-Hooks von rrn5/pattern_recorder).
    let dump_pfad = std::env::var("LISSA_DUMP").ok();

    inject_test_keys();

    let mut zustand = Zustand::new();
    zustand.letzte_zeit = Instant::now();
    let mut letzter_status = String::new();
    let mut beenden = false;
    while !beenden {
        // Ereignisse abarbeiten, dann zeichnen und den Ton nachziehen.
        loop {
            match poll_event_compat() {
                Some(Event::Quit { .. }) => {
                    beenden = true;
                    break;
                }
                Some(Event::KeyDown {
                    keycode: Some(kc), ..
                }) => {
                    if zustand.taste(kc) {
                        beenden = true;
                        break;
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
        if beenden {
            break;
        }

        let jetzt = Instant::now();
        zustand.takte_fortschreiben(jetzt);
        let modus = zustand.wirksamer_modus();
        let phi = lissa::phase(
            zustand.takte_seit_start,
            zustand.takte,
            zustand.umdrehungen,
            modus,
        );

        // Die zwei Stimmen der Figur (X = Grundton·x, Y = Grundton·y); die
        // X-Stimme bekommt den Frequenzversatz der Drehung (wie im Max-Patch).
        let grund_hz = jamcore::klang::frequenz(zustand.grundton);
        let mut stimmen = lissa::stimmen_frequenzen(grund_hz, zustand.a, zustand.b);
        stimmen[0] += (lissa::phasen_geschwindigkeit(
            zustand.takte_seit_start,
            zustand.takte,
            zustand.umdrehungen,
            modus,
            zustand.bpm,
        ) / TAU) as f32;
        regler.setzen(zustand.ton_an, stimmen);

        if let Err(e) = bild.figur(zustand.a, zustand.b, phi) {
            eprintln!("Zeichnen: {e}");
            break;
        }

        let status = zustand.status();
        if status != letzter_status {
            // Die Werte aus dem Tongenerator zurücklesen: sie sind es, die der
            // Audio-Callback sieht.
            let ist = regler.frequenzen();
            println!(
                "{status} · φ {:+.0}° · X {:.0} Hz / Y {:.0} Hz",
                phi.to_degrees(),
                ist[0],
                ist[1]
            );
            if let Err(e) = bild.titel_setzen(&zustand.titel(phi)) {
                eprintln!("Fenstertitel: {e}");
            }
            letzter_status = status;
        }

        if let Some(pfad) = &dump_pfad {
            match bild.als_ppm(pfad) {
                Ok(()) => println!("Bild geschrieben: {pfad}"),
                Err(e) => eprintln!("Bild nicht geschrieben: {e}"),
            }
            break;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    // Sauber aufräumen: Ton aus, Gerät schließen.
    regler.setzen(false, [0.0, 0.0]);
    drop(ton_device);
    println!("Lissa beendet");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ton_mit(an: bool, freqs: [f32; 2]) -> (TonRegler, LissaTon) {
        let regler = TonRegler::neu();
        regler.setzen(an, freqs);
        let ton = LissaTon::neu(&regler, RATE as i32);
        (regler, ton)
    }

    #[test]
    fn tongenerator_uebernimmt_die_rate_des_geraets() {
        let regler = TonRegler::neu();
        // Unbrauchbare Rate fällt auf die Vorgabe zurück.
        assert_eq!(LissaTon::neu(&regler, 0).rate, RATE);
        assert_eq!(LissaTon::neu(&regler, -1).rate, RATE);
        // Sonst zählt die Rate des offenen Geräts (dort laufen die Phasen).
        assert_eq!(LissaTon::neu(&regler, 48_000).rate, 48_000);
    }

    #[test]
    fn parse_size_akzeptiert_aufloesungen() {
        assert_eq!(parse_size("640x480"), Some((640, 480)));
        assert_eq!(parse_size("320X200"), Some((320, 200)));
        assert_eq!(parse_size("0x10"), Some((1, 10)));
        assert_eq!(parse_size("abc"), None);
        assert_eq!(parse_size("640"), None);
    }

    #[test]
    fn resolve_size_ohne_argument_ist_none() {
        assert_eq!(resolve_size(&[]), None);
        assert_eq!(resolve_size(&["--size".to_string()]), None);
        assert_eq!(
            resolve_size(&["--size".to_string(), "800x600".to_string()]),
            Some((800, 600))
        );
    }

    #[test]
    fn vorgabe_ist_ein_verhaeltnis_mit_ton_aus() {
        let z = Zustand::new();
        assert_eq!((z.a, z.b), (2, 3), "2:3 wie im Android-Panel");
        assert_eq!(z.bpm, VORGABE_BPM);
        assert_eq!(z.grundton, VORGABE_GRUNDTON);
        assert!(!z.ton_an, "Ton ist beim Start aus");
        assert_eq!(z.wirksamer_modus(), Modus::Weich);
        assert_eq!(z.takte, VORGABE_TAKTE);
    }

    #[test]
    fn verhaeltnis_bleibt_zwischen_eins_und_sechzehn() {
        let mut z = Zustand::new();
        z.achse = Achse::X;
        for _ in 0..30 {
            z.taste(Keycode::Minus);
        }
        assert_eq!(z.a, VERHAELTNIS_MIN, "kleiner geht nicht");
        for _ in 0..40 {
            z.taste(Keycode::Plus);
        }
        assert_eq!(z.a, VERHAELTNIS_MAX, "größer geht nicht");
        // Pfeiltasten ändern die Achsen direkt.
        for _ in 0..40 {
            z.taste(Keycode::Up);
        }
        assert_eq!(z.b, VERHAELTNIS_MAX);
        for _ in 0..40 {
            z.taste(Keycode::Down);
        }
        assert_eq!(z.b, VERHAELTNIS_MIN);
    }

    #[test]
    fn plus_und_minus_wirken_auf_die_gewaehlte_achse() {
        let mut z = Zustand::new();
        z.taste(Keycode::Y);
        z.taste(Keycode::Minus);
        assert_eq!((z.a, z.b), (2, 2), "Y wurde geändert");
        z.taste(Keycode::X);
        z.taste(Keycode::Minus);
        assert_eq!((z.a, z.b), (1, 2), "X wurde geändert");
    }

    #[test]
    fn presets_setzen_das_verhaeltnis() {
        let mut z = Zustand::new();
        z.taste(Keycode::Num1);
        assert_eq!((z.a, z.b), (2, 2), "Kreis");
        z.taste(Keycode::Num2);
        assert_eq!((z.a, z.b), (2, 3), "Unendlich");
        z.taste(Keycode::Num3);
        assert_eq!((z.a, z.b), (1, 2));
        z.taste(Keycode::Num4);
        assert_eq!((z.a, z.b), (3, 2));
        z.taste(Keycode::Num5);
        assert_eq!((z.a, z.b), (3, 4));
        z.taste(Keycode::Num6);
        assert_eq!((z.a, z.b), (4, 5));
    }

    #[test]
    fn m_wechselt_saegezahn_und_weich() {
        let mut z = Zustand::new();
        assert_eq!(z.wirksamer_modus(), Modus::Weich);
        z.taste(Keycode::M);
        assert_eq!(z.wirksamer_modus(), Modus::Saege);
        z.taste(Keycode::M);
        assert_eq!(z.wirksamer_modus(), Modus::Weich);
    }

    #[test]
    fn l_und_r_schalten_den_endlosmodus() {
        let mut z = Zustand::new();
        z.taste(Keycode::L);
        assert_eq!(z.wirksamer_modus(), Modus::UnendlichLinks);
        z.taste(Keycode::R);
        assert_eq!(z.wirksamer_modus(), Modus::UnendlichRechts);
        z.taste(Keycode::R);
        assert_eq!(z.wirksamer_modus(), Modus::Weich, "erneut gedrückt hebt ∞ auf");
        z.taste(Keycode::M);
        assert_eq!(z.wirksamer_modus(), Modus::Saege, "m beendet ∞ ebenfalls");
    }

    #[test]
    fn umdrehungen_und_takte_folgen_den_kernfaellen() {
        let mut z = Zustand::new();
        z.taste(Keycode::Comma);
        assert_eq!(z.umdrehungen, lissa::teile_umdrehungen(1.0, 2.0));
        z.taste(Keycode::Period);
        assert_eq!(z.umdrehungen, 1.0);
        z.taste(Keycode::LeftBracket);
        assert_eq!(z.takte, jamcore::takte::teile(VORGABE_TAKTE, 2.0));
        z.taste(Keycode::RightBracket);
        assert_eq!(z.takte, VORGABE_TAKTE);
    }

    #[test]
    fn grundton_und_bpm_bleiben_im_bereich() {
        let mut z = Zustand::new();
        for _ in 0..200 {
            z.taste(Keycode::Z);
        }
        assert_eq!(z.grundton, GRUNDTON_MIN);
        for _ in 0..300 {
            z.taste(Keycode::H);
        }
        assert_eq!(z.grundton, GRUNDTON_MAX);
        for _ in 0..500 {
            z.taste(Keycode::B);
        }
        assert_eq!(z.bpm, BPM_MIN);
        for _ in 0..500 {
            z.taste(Keycode::N);
        }
        assert_eq!(z.bpm, BPM_MAX);
    }

    #[test]
    fn t_schaltet_den_ton_und_escape_beendet() {
        let mut z = Zustand::new();
        assert!(!z.taste(Keycode::T), "t beendet nicht");
        assert!(z.ton_an);
        assert!(!z.taste(Keycode::T));
        assert!(!z.ton_an);
        assert!(z.taste(Keycode::Escape), "Escape beendet");
    }

    #[test]
    fn takte_laufen_mit_dem_tempo() {
        let mut z = Zustand::new();
        z.bpm = 120.0;
        let start = Instant::now();
        z.letzte_zeit = start;
        z.takte_fortschreiben(start + Duration::from_millis(1000));
        assert!(
            (z.takte_seit_start - 2.0).abs() < 1e-9,
            "1 s bei 120 BPM = 2 Takte: {}",
            z.takte_seit_start
        );
    }

    #[test]
    fn status_zeigt_verhaeltnis_modus_und_grundton() {
        let z = Zustand::new();
        let s = z.status();
        assert!(s.contains("2:3"), "{s}");
        assert!(s.contains("weich"), "{s}");
        assert!(s.contains("128 BPM"), "{s}");
        assert!(s.contains("220 Hz"), "{s}");
        assert!(s.contains("Ton aus"), "{s}");
        let t = z.titel(std::f64::consts::PI);
        assert!(t.starts_with("Lissa"), "{t}");
        assert!(t.contains("+180°"), "{t}");
    }

    #[test]
    fn figur_bleibt_im_fenster_und_ist_zentriert() {
        // Radius = 44 % der kürzeren Seite (480) = 211,2.
        let punkte = figur_bildpunkte(640, 480, 2, 3, 0.7);
        assert_eq!(punkte.len(), FIGUR_PUNKTE, "Punkte je Umlauf");
        for p in &punkte {
            assert!(
                p.x() >= 0 && p.x() < 640 && p.y() >= 0 && p.y() < 480,
                "Punkt im Fenster: ({}, {})",
                p.x(),
                p.y()
            );
        }
        let rand = |v: &[i32], mitte: i32| v.iter().map(|x| (x - mitte).abs()).max().unwrap_or(0);
        let xs: Vec<i32> = punkte.iter().map(|p| p.x()).collect();
        let ys: Vec<i32> = punkte.iter().map(|p| p.y()).collect();
        assert!(
            (rand(&xs, 320) - 211).abs() <= 1,
            "halbe Breite = Radius: {}",
            rand(&xs, 320)
        );
        assert!(
            (rand(&ys, 240) - 211).abs() <= 1,
            "halbe Höhe = Radius: {}",
            rand(&ys, 240)
        );
        // Bei φ = 0 beginnt die Figur in der Mitte (sin 0 = 0).
        let mitte = figur_bildpunkte(640, 480, 2, 3, 0.0);
        assert_eq!((mitte[0].x(), mitte[0].y()), (320, 240), "Start in der Mitte");
    }

    #[test]
    fn linie_ist_lueckenlos_und_haelt_sich_im_puffer() {
        let (xw, yh) = (32i32, 32i32);
        let mut puffer = vec![0u8; (xw * yh) as usize * 4];
        linie(
            &mut puffer,
            xw,
            yh,
            Point::new(1, 1),
            Point::new(30, 12),
            FIGUR_FARBE,
        );
        let gesetzt = |x: i32, y: i32| puffer[((y * xw + x) * 4) as usize] == FIGUR_FARBE.r;
        for x in 1..=30 {
            assert!(
                (0..32).any(|y| gesetzt(x, y)),
                "jede Spalte hat ein Pixel (x={x})"
            );
        }
        assert!(gesetzt(1, 1) && gesetzt(30, 12), "beide Enden gesetzt");
        assert!(!gesetzt(31, 31), "sonst bleibt der Puffer leer");
        // Ein einzelner Punkt ist auch eine Linie.
        let mut einzeln = vec![0u8; 4];
        linie(&mut einzeln, 1, 1, Point::new(0, 0), Point::new(0, 0), FIGUR_FARBE);
        assert_eq!(einzeln[0], FIGUR_FARBE.r);
    }

    #[test]
    fn figur_puffer_hat_hintergrund_und_eine_zusammenhaengende_linie() {
        let (xw, yh) = (640i32, 480i32);
        let mut puffer = vec![0u8; (xw * yh) as usize * 4];
        figur_in_puffer(&mut puffer, xw, yh, 2, 3, 0.7);

        let farbe = |x: i32, y: i32| {
            let i = ((y * xw + x) * 4) as usize;
            (puffer[i], puffer[i + 1], puffer[i + 2])
        };
        assert_eq!(
            farbe(0, 0),
            (HINTERGRUND.r, HINTERGRUND.g, HINTERGRUND.b),
            "dunkler Hintergrund"
        );

        let mut spalten: Vec<Vec<i32>> = vec![Vec::new(); xw as usize];
        let mut hell = 0;
        for y in 0..yh {
            for x in 0..xw {
                let p = farbe(x, y);
                if p.0 as u32 + p.1 as u32 + p.2 as u32 > 400 {
                    spalten[x as usize].push(y);
                    hell += 1;
                }
            }
        }
        assert!(hell > 600, "die Figur ist sichtbar: {hell} Pixel");
        let xs: Vec<i32> = (0..xw)
            .filter(|x| !spalten[*x as usize].is_empty())
            .collect();
        let ys: Vec<i32> = spalten
            .iter()
            .flat_map(|s| s.iter().copied())
            .collect();
        assert!(
            xs.first().copied().unwrap_or(999) <= 110 && xs.last().copied().unwrap_or(-1) >= 530,
            "Breite der Figur: {:?}..{:?}",
            xs.first(),
            xs.last()
        );
        assert!(
            ys.iter().min().copied().unwrap_or(999) <= 30
                && ys.iter().max().copied().unwrap_or(-1) >= 450,
            "Höhe der Figur: {:?}..{:?}",
            ys.iter().min(),
            ys.iter().max()
        );
        // Keine Lücke: benachbarte Spalten hängen zusammen (8er-Nachbarschaft).
        let mut luecken = 0;
        for x in 1..xw as usize {
            if spalten[x].is_empty() || spalten[x - 1].is_empty() {
                continue;
            }
            let anschluesse = spalten[x]
                .iter()
                .filter(|y| spalten[x - 1].iter().any(|v| (v - *y).abs() <= 1))
                .count();
            if anschluesse == 0 {
                luecken += 1;
            }
        }
        assert_eq!(luecken, 0, "die Linie ist lückenlos");
    }

    #[test]
    fn figur_kippt_mit_der_phase() {
        let (xw, yh) = (200i32, 200i32);
        let bild = |phi: f64| {
            let mut puffer = vec![0u8; (xw * yh) as usize * 4];
            figur_in_puffer(&mut puffer, xw, yh, 2, 3, phi);
            puffer
        };
        let hell = |p: &[u8]| {
            p.chunks_exact(4)
                .filter(|px| px[0] as u32 + px[1] as u32 + px[2] as u32 > 400)
                .count()
        };
        let unterschied = |a: &[u8], b: &[u8]| {
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(x, y)| x != y)
                .count()
        };
        let bei_null = bild(0.0);
        let spaeter = bild(2.0 * std::f64::consts::PI);
        assert!(
            unterschied(&bei_null, &spaeter) <= hell(&bei_null) / 20,
            "nach einem Umlauf ist die Figur wieder dieselbe ({} Pixel anders)",
            unterschied(&bei_null, &spaeter)
        );
        let gedreht = bild(std::f64::consts::FRAC_PI_2);
        assert_ne!(bei_null, gedreht, "mit anderer Phase kippt die Figur");
        let (h0, h1) = (hell(&bei_null) as i32, hell(&gedreht) as i32);
        assert!(
            (h0 - h1).abs() * 2 < h0.max(h1),
            "die Figur bleibt dabei gleich groß: {h0} vs {h1}"
        );
    }

    #[test]
    fn tonregler_gibt_die_werte_weiter() {
        let regler = TonRegler::neu();
        assert_eq!(regler.frequenzen(), [0.0, 0.0]);
        regler.setzen(true, [440.0, 660.0]);
        assert!(regler.an.load(Ordering::Relaxed));
        assert_eq!(regler.frequenzen(), [440.0, 660.0]);
    }

    #[test]
    fn tongenerator_ist_still_wenn_ton_aus() {
        let (regler, mut ton) = ton_mit(false, [440.0, 660.0]);
        let mut block = vec![0i16; 1024];
        for _ in 0..4 {
            ton.callback(&mut block);
        }
        assert!(block.iter().all(|v| *v == 0), "aus bleibt still");
        // Einschalten: es klingt. Ausschalten: nach der Rampe wieder still.
        regler.setzen(true, [440.0, 660.0]);
        for _ in 0..4 {
            ton.callback(&mut block);
        }
        assert!(block.iter().any(|v| *v != 0), "an klingt");
        regler.setzen(false, [440.0, 660.0]);
        for _ in 0..4 {
            ton.callback(&mut block);
        }
        assert!(block.iter().all(|v| *v == 0), "nach der Rampe still");
    }

    #[test]
    fn tongenerator_klingt_und_blendet_weich_ein() {
        let (_regler, mut ton) = ton_mit(true, [440.0, 660.0]);
        let mut block = vec![0i16; 1024];
        ton.callback(&mut block);
        let spitze = |s: &[i16]| s.iter().map(|v| v.abs() as i32).max().unwrap_or(0);
        assert!(
            (spitze(&block[..64]) as f32) < (spitze(&block[960..]) as f32) / 2.0,
            "Rampe: Anfang leiser als Ende ({} vs {})",
            spitze(&block[..64]),
            spitze(&block[960..])
        );
        let mut laut = 0;
        for _ in 0..8 {
            ton.callback(&mut block);
            laut = laut.max(spitze(&block));
        }
        assert!(laut > 3000, "es klingt: {laut}");
        assert!(laut < 15000, "moderat (kein Vollausschlag): {laut}");
    }

    #[test]
    fn tongenerator_fuehrt_die_phasen_weiter() {
        let (_regler, mut ton) = ton_mit(true, [441.0, 0.0]);
        let mut block = vec![0i16; 1024];
        for _ in 0..80 {
            ton.callback(&mut block); // Glättung der Frequenz einschwingen
        }
        let vorher = ton.phasen[0];
        ton.callback(&mut block);
        let erwartet = (vorher + 441.0 * 1024.0 / RATE as f64).fract();
        assert!(
            (ton.phasen[0] - erwartet).abs() < 1e-5,
            "Phase X läuft weiter: {} vs {erwartet}",
            ton.phasen[0]
        );
        assert!(
            ton.phasen[1].abs() < 1e-12,
            "Stimme ohne Frequenz steht: {}",
            ton.phasen[1]
        );
    }
}
