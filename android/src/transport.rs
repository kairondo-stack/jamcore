//! Globale Transport-Uhr der Android-App.
//!
//! Rhythmus, Bild-Scan und Laub-Anhören sollen **einzeln** start- und
//! stoppbar sein, aber synchron laufen. Deshalb gibt es genau **einen**
//! Startzeitpunkt: Jeder Erzeuger fragt hier seine Phase ab, statt eine eigene
//! `Instant`-Uhr zu führen. Startet ein zweiter Tab, während der erste schon
//! läuft, übernimmt er einfach die laufende Phase — es gibt keinen neuen
//! Startpunkt und damit keinen Versatz.
//!
//! Die Uhr stoppt erst, wenn der letzte aktive Teil stoppt. Ein einzelner
//! Stopp-Knopf lässt die anderen also weiterklingen (Nutzerwunsch:
//! „Alle sind sync. Und alle können in jedem Tab hörbar sein.“).

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use jamcore::transport as kern;

/// Die drei Erzeuger, die sich eine Uhr teilen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Teil {
    Rhythmus,
    Bild,
    Laub,
}

impl Teil {
    /// Alle Teile in Anzeige-Reihenfolge (Logs und Status).
    pub const ALLE: [Teil; 3] = [Teil::Rhythmus, Teil::Bild, Teil::Laub];

    fn index(self) -> usize {
        match self {
            Teil::Rhythmus => 0,
            Teil::Bild => 1,
            Teil::Laub => 2,
        }
    }

    /// Deutscher Name für Log und Oberfläche.
    pub fn name(self) -> &'static str {
        match self {
            Teil::Rhythmus => "Rhythmus",
            Teil::Bild => "Bild",
            Teil::Laub => "Laub",
        }
    }
}

/// Zustand der gemeinsamen Uhr.
struct Uhr {
    /// `None` = gestoppt. Sonst der eine Startzeitpunkt aller Teile.
    start: Option<Instant>,
    /// Aktuelles Tempo (vom App-Tempo gesetzt).
    bpm: f32,
    /// Rasterschritte je Takt (16 = 1/16).
    feinheit: u16,
    /// Welche Erzeuger gerade laufen sollen.
    teile: [bool; 3],
}

impl Uhr {
    fn neu() -> Self {
        Self {
            start: None,
            bpm: 120.0,
            feinheit: 16,
            teile: [false; 3],
        }
    }

    fn laeuft(&self) -> bool {
        self.start.is_some()
    }

    /// Namen der aktiven Teile, in fester Reihenfolge.
    fn aktive(&self) -> Vec<&'static str> {
        Teil::ALLE
            .iter()
            .filter(|teil| self.teile[teil.index()])
            .map(|teil| teil.name())
            .collect()
    }

    /// Volle Takte seit dem Start beim aktuellen Tempo.
    fn takte_bis(&self, jetzt: Instant) -> f64 {
        match self.start {
            Some(start) => jetzt.saturating_duration_since(start).as_secs_f64()
                / kern::takt_dauer(self.bpm),
            None => 0.0,
        }
    }

    /// Tempo/Feinheit übernehmen und dabei die **musikalische** Position
    /// erhalten.
    ///
    /// Warum neu verankern: `start + neues Tempo` würde die Phase springen
    /// lassen (bei doppeltem Tempo wäre man plötzlich einen Takt weiter). Die
    /// bisherige Taktposition wird deshalb in die neue Zeit umgerechnet; alle
    /// drei Tabs bleiben nach einer Tempoänderung an derselben Stelle.
    fn parameter(&mut self, bpm: f32, feinheit: u16, jetzt: Instant) {
        // Die App rechnet überall in 20…300 BPM; ein unbrauchbarer Wert lässt
        // das bisherige Tempo stehen, statt die Uhr zu vergiften.
        let bpm = if bpm.is_finite() {
            bpm.clamp(20.0, 300.0)
        } else {
            self.bpm
        };
        let feinheit = feinheit.clamp(1, 256);
        let altes_bpm = self.bpm;
        let alte_feinheit = self.feinheit;
        let tempo_neu = (bpm - altes_bpm).abs() > 0.1;
        if !tempo_neu && feinheit == alte_feinheit {
            return;
        }
        if self.start.is_some() {
            let takte = self.takte_bis(jetzt);
            if takte.is_finite() {
                let sekunden = takte * kern::takt_dauer(bpm);
                if let Some(neu) = jetzt.checked_sub(Duration::from_secs_f64(sekunden)) {
                    self.start = Some(neu);
                }
            }
        }
        self.bpm = bpm;
        self.feinheit = feinheit;
        // Nicht bei jedem Link-Nachkommastellen-Zucken loggen, sondern nur,
        // wenn sich der angezeigte Wert oder das Raster wirklich aendert.
        if (bpm.round() - altes_bpm.round()).abs() > 0.5 || feinheit != alte_feinheit {
            log::info!("transport: BPM {bpm:.0}, 1/{feinheit}");
        }
    }
}

static UHR: OnceLock<Mutex<Uhr>> = OnceLock::new();

/// Zugriff auf die eine Uhr; ein vergifteter Mutex darf die App nicht
/// mitreißen (ein Panic in einem Render-Thread soll den Ton nicht verstummen
/// lassen).
fn uhr() -> MutexGuard<'static, Uhr> {
    UHR.get_or_init(|| Mutex::new(Uhr::neu()))
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
}

/// Startet die Uhr, falls sie steht. Laufende Teile bleiben unverändert.
///
/// Ein zweiter Aufruf ist bewusst wirkungslos: Genau das macht den Sync aus —
/// ein später startender Tab bekommt den ursprünglichen Startzeitpunkt.
///
/// Teil der Transport-API neben [`teil_an`]/[`teil_aus`] (globaler Start fuer
/// Not-Aus-Tests); die App selbst meldet ihre Erzeuger einzeln an.
#[allow(dead_code)]
pub fn starten() {
    let mut u = uhr();
    if u.start.is_none() {
        u.start = Some(Instant::now());
        let liste = namensliste(&u.aktive());
        log::info!("transport: Start ({liste})");
    }
}

/// Stoppt die Uhr und alle Teile (Not-Aus bzw. Verlassen der App).
#[allow(dead_code)]
pub fn stoppen() {
    let mut u = uhr();
    if u.start.is_none() && u.teile.iter().all(|an| !an) {
        return;
    }
    let liste = namensliste(&u.aktive());
    u.start = None;
    u.teile = [false; 3];
    log::info!("transport: Stop ({liste})");
}

/// Läuft die gemeinsame Uhr?
pub fn laeuft() -> bool {
    uhr().laeuft()
}

/// Meldet einen Erzeuger an. Ist die Uhr noch aus, startet sie **jetzt**;
/// läuft sie schon, übernimmt der neue Teil die bestehende Phase.
pub fn teil_an(teil: Teil) {
    let mut u = uhr();
    let index = teil.index();
    let war_an = u.teile[index];
    u.teile[index] = true;
    if u.start.is_none() {
        u.start = Some(Instant::now());
    }
    if !war_an {
        let liste = namensliste(&u.aktive());
        log::info!("transport: Start ({liste})");
    }
}

/// Meldet einen Erzeuger ab. Die Uhr läuft weiter, solange ein anderer Teil
/// aktiv ist; erst der letzte Stopp hält sie an.
pub fn teil_aus(teil: Teil) {
    let mut u = uhr();
    let index = teil.index();
    if !u.teile[index] {
        return;
    }
    u.teile[index] = false;
    log::info!("transport: Stop ({})", teil.name());
    if u.teile.iter().all(|an| !an) {
        u.start = None;
    }
}

/// Ist dieser Erzeuger angemeldet (auch wenn sein Tab gerade nicht sichtbar
/// ist)?
pub fn teil_aktiv(teil: Teil) -> bool {
    uhr().teile[teil.index()]
}

/// Aktuelles Tempo der Uhr (zuletzt von der App gesetzt).
pub fn bpm() -> f32 {
    uhr().bpm
}

/// Aktuelle Feinheit der Uhr (Rasterschritte je Takt).
pub fn feinheit() -> u16 {
    uhr().feinheit
}

/// Setzt Tempo und Feinheit der einen Uhr (die App ruft das je Frame mit dem
/// App-Tempo und der Rhythmus-Feinheit). Eine Änderung erhält die Taktposition.
pub fn set_bpm(bpm: f32, feinheit: u16) {
    let jetzt = Instant::now();
    uhr().parameter(bpm, feinheit, jetzt);
}

/// Rasterposition seit dem Start in Schritten (gebrochen).
///
/// `bpm`/`feinheit` werden dabei mit der Uhr abgeglichen (eine abweichende
/// Angabe verankert die Uhr neu, ohne die Taktposition zu verlieren). Wer nur
/// die zuletzt gesetzten Werte braucht, nimmt [`position_jetzt`].
///
/// Teil der geforderten Transport-API (Test/Diagnose); die App liest die
/// zuletzt gesetzten Werte ueber [`position_jetzt`].
#[allow(dead_code)]
pub fn position(bpm: f32, feinheit: u16, jetzt: Instant) -> f64 {
    let mut u = uhr();
    u.parameter(bpm, feinheit, jetzt);
    let Some(start) = u.start else {
        return 0.0;
    };
    kern::position_schritte(
        0.0,
        u.bpm,
        u.feinheit,
        jetzt.saturating_duration_since(start).as_secs_f64(),
    )
}

/// Position in Schritten mit dem zuletzt gesetzten Tempo.
pub fn position_jetzt() -> f64 {
    let u = uhr();
    let Some(start) = u.start else {
        return 0.0;
    };
    kern::position_schritte(
        0.0,
        u.bpm,
        u.feinheit,
        Instant::now().saturating_duration_since(start).as_secs_f64(),
    )
}

/// Schritt im aktuellen Takt (`0..feinheit`) zum angegebenen Zeitpunkt.
///
/// Teil der geforderten Transport-API; die App nutzt [`step_jetzt`].
#[allow(dead_code)]
pub fn step(bpm: f32, feinheit: u16, jetzt: Instant) -> u16 {
    schritt_aus_position(position(bpm, feinheit, jetzt), feinheit.max(1))
}

/// Schritt im aktuellen Takt mit dem zuletzt gesetzten Tempo.
pub fn step_jetzt() -> u16 {
    schritt_aus_position(position_jetzt(), uhr().feinheit.max(1))
}

/// Rechnet eine gebrochene Position auf den Schritt im Takt.
fn schritt_aus_position(position: f64, feinheit: u16) -> u16 {
    if !position.is_finite() || position < 0.0 {
        return 0;
    }
    // `as u64` sättigt: nach Stunden Laufzeit bleibt die Rechnung endlich.
    (position.floor() as u64 % u64::from(feinheit.max(1))) as u16
}

/// „aus“ oder „läuft (Rhythmus + Bild)“ — Anzeige in allen drei Tabs.
pub fn status_text() -> String {
    let u = uhr();
    if !u.laeuft() {
        return "aus".to_string();
    }
    let liste = u.aktive();
    if liste.is_empty() {
        "läuft (Uhr)".to_string()
    } else {
        format!("läuft ({})", liste.join(" + "))
    }
}

/// Namen mit Komma verbinden; leere Liste wird als „Uhr“ gelesen (globale
/// Start/Stop-Aufrufe ohne angemeldeten Teil).
fn namensliste(teile: &[&'static str]) -> String {
    if teile.is_empty() {
        "Uhr".to_string()
    } else {
        teile.join(", ")
    }
}
