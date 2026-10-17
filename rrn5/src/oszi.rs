//! Oszillator-Scan des Bildes und Anzeige-Umschalter (neu 2026-10-17).
//!
//! Dünne Hülle um `jamcore::oszillator`: die Rechnung (welcher x-Anteil ist
//! bei welcher Zeit dran, wie werden die Kreis-x-Bänder gemappt, welche
//! Harmonie gilt an einem x, welche Note liegt in welcher Zeile) liegt dort
//! getestet; hier steht nur der Zustand des Desktop-rrn5 samt Statuszeile,
//! Tastenschritten, der Auswahl der Noten (tiefste bzw. kleiner Akkord) und
//! der Drossel des Linien-Tons.
//!
//! Aufbau (alles standardmäßig **aus**, der bestehende Scan bleibt unberührt):
//!
//! - **Linie** (`linie_an`): eine senkrechte Linie im Bild; die Pixel der
//!   Linie, die zur Auswahl passen (Farbe+Toleranz, Takt-Maske, Harmonien),
//!   werden als Noten per OSC gesendet. Die Linie ist nur im
//!   **Verschiebe-Modus** (`verschieben`) beweglich: dort setzt ein Klick die
//!   Spalte und die Pfeiltasten schieben sie weiter — sonst macht ein Tipp die
//!   Farbauswahl.
//! - **x-Bereich** (`bereich_an`): statt der Linie läuft der Durchlauf über
//!   den Bereich (die Split-Grenzen `Alt+D`/`Alt+C`), Dauer 1/64 … 4 Takt.
//!   `durchlauf = Sweep` wandert einmal über den Bereich, `Alles` sendet alle
//!   Spalten gleichzeitig.
//! - **Takt-Maske ⇄ alle y** (`maske_an`): mit Maske kommen nur Spalten mit
//!   `iialpha ≠ 0` durch (Amplitude skaliert Farbe und Lautstärke), ohne Maske
//!   ist jede Spalte und jede Zeile durch.
//! - **Harmonie-Zeit** (`basis`): die x-Bänder der Harmonien (aus der
//!   Quintenzirkel-/Notenmengen-Logik, `notessetx`) werden auf das **Gesamt-x**
//!   (ganze Bildbreite) oder das **Bereichs-x** (nur der Durchlauf-Bereich)
//!   gemappt; die Harmonie wechselt mit der Linie/Zeit.
//! - **Anzeige** (`Anzeige`, Taste Shift+F1): „grün" markiert die Treffer wie
//!   die Android-App grün, „echte Farben" zeigt nur die Auswahl in
//!   Originalfarben und sonst nichts (kein Grün) — das ist der bisherige Stand
//!   des Fensters „Selected color" und deshalb hier die Vorgabe.

use std::time::Instant;

use jamcore::oszillator::{
    anteil_je_zeit, harm_basis, harmonie_an, note_fuer_zeile, Durchlauf, XBasis,
};

/// Vorgabe der Dauer: ein Vierteltakt (wie die App).
pub const DAUER_VORGABE: f64 = 0.25;
/// Kürzeste Dauer: 1/64 Takt.
pub const DAUER_MIN: f64 = 1.0 / 64.0;
/// Längste Dauer: 4 Takte.
pub const DAUER_MAX: f64 = 4.0;
/// Drossel des Linien-Tons: höchstens alle 30 ms eine neue Note.
///
/// Ohne Drossel würde bei jedem Frame (60+ je Sekunde) je Trefferzeile eine
/// Note fliegen — das klingt wie der bestehende Scan (Flut) und nicht wie ein
/// Oszillator. 30 ms ≈ 33 Noten je Sekunde: schnell genug für ein
/// „Dauerton"-Gefühl, langsam genug, dass Renoise jede Note als eigene
/// anspricht.
pub const TAKT_MS: u64 = 30;
/// Im „alle Spalten"-Modus höchstens so viele Noten je Takt (kleiner Akkord).
pub const ALLE_MAX_NOTEN: usize = 4;

/// Grün der Android-App für markierte Treffer (`markieren_bild`).
pub const APP_GRUEN: (u8, u8, u8) = (20, 235, 60);

/// Anzeige-Umschalter „grün ⇄ echte Farben" (Taste Shift+F1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anzeige {
    /// Treffer grün markieren (wie die App).
    Gruen,
    /// Nur die Auswahl in Originalfarben, alles andere dunkel, kein Grün.
    Echt,
}

impl Default for Anzeige {
    /// Vorgabe „echte Farben": das ist der bisherige Stand des Desktop-rrn5
    /// (die App startet auf „grün", weil dort die grüne Markierung der
    /// bisherige Stand ist). Beide Schalter sind derselbe Umschalter.
    fn default() -> Self {
        Anzeige::Echt
    }
}

/// Anzeige umschalten.
pub fn anzeige_umschalten(a: Anzeige) -> Anzeige {
    match a {
        Anzeige::Gruen => Anzeige::Echt,
        Anzeige::Echt => Anzeige::Gruen,
    }
}

/// Name der Anzeige für Statuszeile und Log.
pub fn anzeige_text(a: Anzeige) -> &'static str {
    match a {
        Anzeige::Gruen => "grün",
        Anzeige::Echt => "echte Farben",
    }
}

/// Name des Durchlaufs für Statuszeile und Log.
pub fn durchlauf_text(d: Durchlauf) -> &'static str {
    match d {
        Durchlauf::Sweep => "Sweep",
        Durchlauf::Alles => "alle Spalten",
    }
}

/// Name der Harmonie-x-Basis für Statuszeile und Log.
pub fn basis_text(b: XBasis) -> &'static str {
    match b {
        XBasis::Gesamt => "Gesamt",
        XBasis::Bereich => "Bereich",
    }
}

/// Farbe eines Treffers für die Anzeige: „grün" markiert (mit der
/// Masken-Amplitude gedimmt), „echte Farben" nimmt die Originalfarbe.
pub fn treffer_farbe(anzeige: Anzeige, r: u8, g: u8, b: u8, alpha: f32) -> (u8, u8, u8) {
    match anzeige {
        Anzeige::Gruen => {
            let (ar, ag, ab) = APP_GRUEN;
            jamcore::livescan::scale_rgb(ar, ag, ab, alpha)
        }
        Anzeige::Echt => jamcore::livescan::scale_rgb(r, g, b, alpha),
    }
}

/// Anzeige-Kopie des Fensters „Selected color" für den grünen Modus: alles,
/// was nicht Hintergrundfarbe ist, wird grün markiert — wie `markieren_bild`
/// in der App. Der Puffer selbst bleibt unangetastet (er ist die Palette des
/// Live-Scans, `build_palette` liest die gemalten Originalfarben).
///
/// Grenze der Vereinfachung: markiert wird alles außer dem Hintergrund, also
/// auch der weiße Marker (1,1,1) und die Harmonik-Farben des mittleren
/// Mausknopfs. In „echte Farben" bleibt der Puffer unverändert.
pub fn anzeige_kopie(buf: &[u8], bg: (u8, u8, u8)) -> Vec<u8> {
    let (gr, gg, gb) = APP_GRUEN;
    let mut kopie = buf.to_vec();
    for px in kopie.chunks_exact_mut(4) {
        if (px[0], px[1], px[2]) != bg {
            px[0] = gr;
            px[1] = gg;
            px[2] = gb;
            px[3] = 255;
        }
    }
    kopie
}

/// Zustand des Oszillator-Scans.
pub struct Oszi {
    /// Linie an/aus (Taste Shift+F2).
    pub linie_an: bool,
    /// x-Spalte der Linie (0 … Breite−1).
    pub linie_x: i32,
    /// Verschiebe-Modus (Taste Shift+F3): ein Klick im Bild setzt die Linie,
    /// die Pfeiltasten ←/→ schieben sie — statt zu malen (sonst macht ein Tipp
    /// die Farbauswahl).
    pub verschieben: bool,
    /// Bereich statt Linie (Taste Shift+F4).
    pub bereich_an: bool,
    /// Dauer des Durchlaufs in Takten (1/64 … 4, Alt+−/Alt++).
    pub dauer_takte: f64,
    /// Sweep (einmal durch) oder alle Spalten gleichzeitig (Taste Shift+F5).
    pub durchlauf: Durchlauf,
    /// Takt-Maske an (Taste Shift+F6); aus = „alle y".
    pub maske_an: bool,
    /// Harmonie-x: Gesamt oder Bereich (Taste Shift+F7).
    pub basis: XBasis,
    /// Zeitgeber des Durchlaufs (wie beim Stroboskop, gedeckelt).
    letzte_zeit: Instant,
    /// Takte seit dem Einschalten — treibt den Sweep.
    takte_seit_start: f64,
    /// Letzter gesendeter Linien-/Bereichs-Ton (Drossel, [`TAKT_MS`]).
    letzte_note: Instant,
}

impl Default for Oszi {
    fn default() -> Self {
        Self::new()
    }
}

impl Oszi {
    pub fn new() -> Self {
        Self {
            linie_an: false,
            linie_x: 0,
            verschieben: false,
            bereich_an: false,
            dauer_takte: DAUER_VORGABE,
            durchlauf: Durchlauf::Sweep,
            maske_an: true,
            basis: XBasis::Gesamt,
            letzte_zeit: Instant::now(),
            takte_seit_start: 0.0,
            // Die Drossel steht auf „fällig": der erste Ton darf sofort.
            letzte_note: Instant::now() - std::time::Duration::from_millis(TAKT_MS),
        }
    }

    /// Scan an/aus (Linie oder Bereich).
    pub fn an(&self) -> bool {
        self.linie_an || self.bereich_an
    }

    /// Linie an/aus (Shift+F2); beim Einschalten beginnt der Durchlauf neu.
    pub fn linie_umschalten(&mut self) -> bool {
        self.linie_an = !self.linie_an;
        if self.linie_an {
            self.start();
        }
        self.linie_an
    }

    /// Bereich statt Linie (Shift+F4).
    pub fn bereich_umschalten(&mut self) -> bool {
        self.bereich_an = !self.bereich_an;
        if self.bereich_an {
            self.start();
        }
        self.bereich_an
    }

    /// Verschiebe-Modus an/aus (Shift+F3).
    pub fn verschieben_umschalten(&mut self) -> bool {
        self.verschieben = !self.verschieben;
        self.verschieben
    }

    /// Sweep ⇄ alle Spalten (Shift+F5).
    pub fn durchlauf_umschalten(&mut self) -> Durchlauf {
        self.durchlauf = match self.durchlauf {
            Durchlauf::Sweep => Durchlauf::Alles,
            Durchlauf::Alles => Durchlauf::Sweep,
        };
        self.start();
        self.durchlauf
    }

    /// Takt-Maske ⇄ alle y (Shift+F6); `true` = Maske an.
    pub fn maske_umschalten(&mut self) -> bool {
        self.maske_an = !self.maske_an;
        self.maske_an
    }

    /// Harmonie-x Gesamt ⇄ Bereich (Shift+F7).
    pub fn basis_umschalten(&mut self) -> XBasis {
        self.basis = match self.basis {
            XBasis::Gesamt => XBasis::Bereich,
            XBasis::Bereich => XBasis::Gesamt,
        };
        self.basis
    }

    /// Dauer durch `n` teilen (Alt+−), geklemmt auf 1/64 … 4 Takt.
    pub fn dauer_teile(&mut self, n: f64) {
        self.dauer_takte = klemme_dauer(self.dauer_takte / n.max(1.0));
    }

    /// Dauer mit `n` malnehmen (Alt++), geklemmt auf 1/64 … 4 Takt.
    pub fn dauer_vielfache(&mut self, n: f64) {
        self.dauer_takte = klemme_dauer(self.dauer_takte * n.max(1.0));
    }

    /// Linie um `dx` Spalten schieben (Pfeiltasten im Verschiebe-Modus).
    pub fn linie_schieben(&mut self, dx: i32, breite: i32) {
        self.setze_linie(self.linie_x + dx, breite);
    }

    /// Linie auf die Spalte `x` setzen (Klick im Verschiebe-Modus).
    pub fn setze_linie(&mut self, x: i32, breite: i32) {
        if breite <= 0 {
            self.linie_x = 0;
            return;
        }
        self.linie_x = x.clamp(0, breite - 1);
    }

    /// Durchlauf neu beginnen (Einschalten, Moduswechsel).
    pub fn start(&mut self) {
        self.letzte_zeit = Instant::now();
        self.takte_seit_start = 0.0;
    }

    /// Takt-Zähler fortschreiben; `bpm` ist das Tempo der App (0/NaN → 120).
    pub fn schritt(&mut self, bpm: f64) {
        let jetzt = Instant::now();
        let dt = jetzt
            .duration_since(self.letzte_zeit)
            .as_secs_f64()
            .min(0.25);
        self.letzte_zeit = jetzt;
        let tempo = if bpm.is_finite() && bpm > 0.0 { bpm } else { 120.0 };
        self.takte_seit_start += dt * tempo / 60.0;
    }

    /// Position des Durchlaufs im Bereich als Anteil (0 … 1); `None` = alle
    /// Spalten gleichzeitig (Modus „alle").
    pub fn durchlauf_anteil(&self) -> Option<f64> {
        anteil_je_zeit(self.takte_seit_start, self.dauer_takte, self.durchlauf)
    }

    /// Ist wieder ein Ton fällig? (Drossel [`TAKT_MS`].)
    pub fn ton_faellig(&self) -> bool {
        self.letzte_note.elapsed().as_millis() as u64 >= TAKT_MS
    }

    /// Einen Ton als gesendet vermerken (Drossel neu aufziehen).
    pub fn ton_gemeldet(&mut self) {
        self.letzte_note = Instant::now();
    }

    /// x-Anteil (0 … 1) für die Harmonie-Auswertung an der Spalte `x`:
    /// „Gesamt" nimmt die ganze Bildbreite, „Bereich" nur den Durchlauf. Die
    /// x-Bänder der Harmonien bleiben dabei in Anteilen der **ganzen** Achse
    /// (siehe [`harmonie_baender`]) — im Bereichs-Modus wirkt der Bereich wie
    /// eine gestauchte Kopie der Achse, die Harmonie wechselt also an den
    /// gleichen **relativen** Stellen.
    ///
    /// `gesamt_schritte` ist in rrn5 das Pixel-Raster der Breite (ein Schritt
    /// je Spalte) — der Kern braucht es nur, um einen Bereich, der schmaler als
    /// ein Schritt ist, auf einen Schritt zu dehnen.
    pub fn harm_anteil(&self, x: i32, breite: i32, links: f32, rechts: f32) -> f32 {
        let (von, bis) = harm_basis(self.basis, breite.max(1) as u16, (links, rechts));
        if bis <= von {
            return 0.0;
        }
        ((x as f32 / breite.max(1) as f32) - von) / (bis - von)
    }

    /// Dauer als Text („1/4 Takt").
    pub fn dauer_text(&self) -> String {
        dauer_text(self.dauer_takte)
    }

    /// Statuszeile des Oszillator-Scans (Log und Startmeldung).
    pub fn text(&self) -> String {
        format!(
            "Oszillator: {} ({}, {}, {}, {})",
            if self.bereich_an { "Bereich" } else { "Linie" },
            self.dauer_text(),
            durchlauf_text(self.durchlauf),
            if self.maske_an {
                "Takt-Maske"
            } else {
                "alle y"
            },
            format!("Harmonie-x {}", basis_text(self.basis))
        )
    }}

/// Dauer in den erlaubten Bereich klemmen (1/64 … 4 Takt); nur `NaN` (und
/// damit Unbrauchbares) wird zur Vorgabe — ±∞ klemmt wie ein zu großer Wert.
pub fn klemme_dauer(dauer: f64) -> f64 {
    if dauer.is_nan() {
        return DAUER_VORGABE;
    }
    dauer.clamp(DAUER_MIN, DAUER_MAX)
}

/// Anzeige der Dauer („1/4 Takt", „1/64 Takt", „1 Takt", „4 Takte").
///
/// Bewusst eigene Rechnung statt [`jamcore::takte::text`]: dessen Raster ist
/// 1/36 … 36 Takt (Stroboskop/Sweep), die Dauer des Oszillator-Bereichs geht
/// aber bis 1/64 Takt — `takte::text(1/64)` würde „1/36 Takt" zeigen.
pub fn dauer_text(dauer: f64) -> String {
    let t = klemme_dauer(dauer);
    if t >= 1.0 {
        let gerundet = t.round();
        if (t - gerundet).abs() < 1e-9 {
            return if gerundet == 1.0 {
                "1 Takt".to_string()
            } else {
                format!("{gerundet:.0} Takte")
            };
        }
    }
    // Bruch suchen: kleinster Nenner bis 64, bei dem der Wert ganzzahlig wird.
    for nenner in 1..=64usize {
        let zaehler = t * nenner as f64;
        if (zaehler - zaehler.round()).abs() < 1e-9 && zaehler >= 1.0 {
            return if zaehler.round() == 1.0 {
                format!("{zaehler:.0}/{nenner} Takt")
            } else {
                format!("{zaehler:.0}/{nenner} Takte")
            };
        }
    }
    format!("{t:.2} Takte")
}

/// x-Bänder der Harmonien als Anteile `(von, bis, Nummer)` für
/// [`harmonie_an`]: Band 0 gilt bis `setx[0]`, Band `v` von `setx[v−1]` bis
/// `setx[v]` — wie `in_notesset_range` (dort in Spalten, hier in Anteilen der
/// Bildbreite). Hinter dem letzten Band gilt keine Harmonie (`harmonie_an`
/// liefert dann `None`).
///
/// Die **Nummer** ist der Index der Harmonie (der Eintrag in `notesset`, der
/// die erlaubten Notenklassen trägt) — nicht die Stelle in der sortierten
/// Reihenfolge. Grenzen werden geklemmt und aufsteigend sortiert; leere Bänder
/// (Grenze 0 oder doppelt) fallen weg, ihr Nachfolger übernimmt.
pub fn harmonie_baender(setx: &[Vec<i32>], breite: i32) -> Vec<(f32, f32, usize)> {
    let w = breite.max(1) as f32;
    let mut grenzen: Vec<(i32, usize)> = setx
        .iter()
        .enumerate()
        .filter_map(|(v, s)| s.first().copied().map(|x| (x.clamp(0, breite.max(1)), v)))
        .collect();
    grenzen.sort_by_key(|(x, _)| *x);
    let mut baender = Vec::with_capacity(grenzen.len());
    let mut von = 0.0f32;
    for (bis_x, v) in grenzen {
        let bis = bis_x as f32 / w;
        if bis > von {
            baender.push((von, bis, v));
        }
        von = von.max(bis);
    }
    baender
}

/// Welche Harmonie gilt an `x_anteil`? `None` = kein Band (gesperrt, wenn
/// Bänder da sind; ohne Bänder entscheidet der Akkord-Filter).
pub fn harm_an(x_anteil: f32, baender: &[(f32, f32, usize)]) -> Option<usize> {
    harmonie_an(x_anteil, baender)
}

/// Tiefste Note aus den Treffern einer Spalte (Linie bzw. eine Spalte des
/// Bereichs): je Trefferzeile `note_fuer_zeile`, gespielt wird die **tiefste**
/// mit ihrer Lautstärke.
///
/// Bewusste Wahl (im Plan offengelassen „tiefste Note oder kleiner Akkord"):
/// Der Linien-Ton ist **ein** Ton je Takt. Alle Trefferzeilen zu spielen wären
/// bei einer Linie mit vielen Treffern ein voller Akkord je 30 ms — das flutet
/// wie der bestehende Scan, statt wie ein Oszillator zu klingen.
pub fn tiefste_note(
    treffer: &[(f32, u8, u8, u8)],
    noten: i32,
    offset: i32,
) -> Option<(i32, i32, (u8, u8, u8))> {
    let mut best: Option<(i32, (u8, u8, u8))> = None;
    for (y_anteil, r, g, b) in treffer {
        let note = note_fuer_zeile(*y_anteil, noten, offset);
        if best.is_none_or(|(n, _)| note < n) {
            best = Some((note, (*r, *g, *b)));
        }
    }
    best.map(|(note, rgb)| (note, 127, rgb))
}

/// Kleiner Akkord aus mehreren Spalten („alle Spalten gleichzeitig"): bis zu
/// [`ALLE_MAX_NOTEN`] verschiedene, tiefste Noten (Note, Lautstärke).
///
/// Bewusste Wahl: Alle Spalten eines breiten Bereichs liefern je Takt viele
/// Noten — deshalb nur die tiefsten, entdoppelt und gedeckelt. Dieselbe Note
/// aus mehreren Spalten bleibt einmal stehen (die **lauteste** Fassung).
pub fn kleiner_akkord(mut noten: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    noten.retain(|(n, _)| (0..=127).contains(n));
    noten.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    noten.dedup_by_key(|(n, _)| *n);
    noten.truncate(ALLE_MAX_NOTEN);
    noten
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorgabe_ist_aus_und_echte_farben() {
        let o = Oszi::new();
        assert!(!o.an(), "Linie und Bereich sind standardmäßig aus");
        assert_eq!(Anzeige::default(), Anzeige::Echt);
        assert_eq!(anzeige_text(Anzeige::Echt), "echte Farben");
        assert_eq!(anzeige_text(Anzeige::Gruen), "grün");
        assert_eq!(anzeige_umschalten(Anzeige::Echt), Anzeige::Gruen);
        assert_eq!(anzeige_umschalten(Anzeige::Gruen), Anzeige::Echt);
    }

    #[test]
    fn linie_und_bereich_schalten_und_starten_neu() {
        let mut o = Oszi::new();
        o.schritt(120.0);
        o.takte_seit_start = 2.0;
        assert!(o.linie_umschalten());
        assert_eq!(o.takte_seit_start, 0.0, "Einschalten beginnt den Durchlauf neu");
        assert!(o.linie_umschalten() == false);
        assert!(o.bereich_umschalten());
        assert!(o.an(), "der Bereich ist der zweite Scan-Modus");
        assert_eq!(o.takte_seit_start, 0.0);
        assert!(!o.bereich_umschalten());
        assert!(!o.an(), "beides aus = kein Oszillator-Scan");
    }

    #[test]
    fn verschiebe_modus_und_maske_und_basis() {
        let mut o = Oszi::new();
        assert!(!o.verschieben);
        assert!(o.verschieben_umschalten());
        assert!(o.verschieben_umschalten() == false);
        assert!(o.maske_an, "Vorgabe: Takt-Maske wie die App");
        assert!(!o.maske_umschalten(), "aus = alle y");
        assert!(o.maske_umschalten());
        assert_eq!(o.basis, XBasis::Gesamt);
        assert_eq!(o.basis_umschalten(), XBasis::Bereich);
        assert_eq!(o.basis_umschalten(), XBasis::Gesamt);
    }

    #[test]
    fn durchlauf_wechselt_zwischen_sweep_und_alle() {
        let mut o = Oszi::new();
        assert_eq!(o.durchlauf, Durchlauf::Sweep);
        assert_eq!(o.durchlauf_umschalten(), Durchlauf::Alles);
        assert_eq!(o.durchlauf_umschalten(), Durchlauf::Sweep);
    }

    #[test]
    fn dauer_ist_geklemmt_auf_ein_vierundsechzigstel_bis_vier_takte() {
        let mut o = Oszi::new();
        assert_eq!(o.dauer_takte, DAUER_VORGABE);
        for _ in 0..20 {
            o.dauer_teile(2.0);
        }
        assert_eq!(o.dauer_takte, DAUER_MIN, "viele Male halbieren klemmt unten");
        for _ in 0..20 {
            o.dauer_vielfache(2.0);
        }
        assert_eq!(o.dauer_takte, DAUER_MAX, "oben ebenso");
        // ÷3/×3 wie die App-Takte: 1/4 → 1/12 → 1/4.
        let mut o = Oszi::new();
        o.dauer_teile(3.0);
        assert!((o.dauer_takte - 1.0 / 12.0).abs() < 1e-12);
        o.dauer_vielfache(3.0);
        assert!((o.dauer_takte - DAUER_VORGABE).abs() < 1e-12);
        // Unbrauchbare Werte fallen auf die Vorgabe.
        assert_eq!(klemme_dauer(f64::NAN), DAUER_VORGABE);
        assert_eq!(klemme_dauer(0.0), DAUER_MIN);
        assert_eq!(klemme_dauer(f64::INFINITY), DAUER_MAX);
        assert_eq!(klemme_dauer(f64::NEG_INFINITY), DAUER_MIN);
        // Die Anzeige rechnet im 1/64-Raster (nicht im 1/36-Raster der Takte).
        assert_eq!(dauer_text(DAUER_MIN), "1/64 Takt");
        assert_eq!(dauer_text(1.0 / 12.0), "1/12 Takt");
        assert_eq!(dauer_text(DAUER_VORGABE), "1/4 Takt");
        assert_eq!(dauer_text(0.5), "1/2 Takt");
        assert_eq!(dauer_text(1.0), "1 Takt");
        assert_eq!(dauer_text(DAUER_MAX), "4 Takte");
    }

    #[test]
    fn linie_bleibt_im_bild() {
        let mut o = Oszi::new();
        o.setze_linie(5, 10);
        assert_eq!(o.linie_x, 5);
        o.setze_linie(-3, 10);
        assert_eq!(o.linie_x, 0);
        o.setze_linie(99, 10);
        assert_eq!(o.linie_x, 9);
        o.setze_linie(4, 10);
        o.linie_schieben(1, 10);
        assert_eq!(o.linie_x, 5, "Pfeil rechts");
        o.linie_schieben(-2, 10);
        assert_eq!(o.linie_x, 3, "Pfeil links");
        o.linie_schieben(-99, 10);
        assert_eq!(o.linie_x, 0, "links klemmt");
        o.setze_linie(3, 0);
        assert_eq!(o.linie_x, 0, "ohne Breite bleibt die Linie bei 0");
    }

    #[test]
    fn drossel_laesst_erst_nach_dem_takt_wieder_durch() {
        let mut o = Oszi::new();
        assert!(o.ton_faellig(), "der erste Ton darf sofort");
        o.ton_gemeldet();
        assert!(!o.ton_faellig(), "unmittelbar danach nicht");
        std::thread::sleep(std::time::Duration::from_millis(TAKT_MS + 5));
        assert!(o.ton_faellig(), "nach dem Takt wieder");
    }

    #[test]
    fn anteil_je_zeit_und_durchlauf_aus_dem_kern() {
        let mut o = Oszi::new();
        o.dauer_takte = 0.5;
        o.durchlauf = Durchlauf::Sweep;
        o.takte_seit_start = 0.0;
        assert_eq!(o.durchlauf_anteil(), Some(0.0), "Sweep beginnt am Anfang");
        o.takte_seit_start = 0.25;
        assert_eq!(o.durchlauf_anteil(), Some(0.5), "halbe Dauer = halber Weg");
        o.takte_seit_start = 0.49;
        let fast_ende = o.durchlauf_anteil().unwrap();
        assert!(fast_ende > 0.97 && fast_ende < 1.0, "fast Ende: {fast_ende}");
        o.takte_seit_start = 0.5;
        assert_eq!(
            o.durchlauf_anteil(),
            Some(0.0),
            "am Ende des Durchlaufs beginnt der nächste"
        );
        o.durchlauf = Durchlauf::Alles;
        assert_eq!(
            o.durchlauf_anteil(),
            None,
            "der Modus alle hat keine Position"
        );
    }

    #[test]
    fn harmonie_baender_aus_den_setx_spalten() {
        // Zwei Harmonien: Grenzen 100 und 200 bei Breite 400.
        let setx = vec![vec![100], vec![200]];
        let b = harmonie_baender(&setx, 400);
        assert_eq!(b, vec![(0.0, 0.25, 0), (0.25, 0.5, 1)]);
        assert_eq!(harm_an(0.1, &b), Some(0), "vor der ersten Grenze: Band 0");
        assert_eq!(harm_an(0.3, &b), Some(1), "zwischen den Grenzen: Band 1");
        assert_eq!(harm_an(0.7, &b), None, "hinter der letzten Grenze gesperrt");
        // Die Nummer bleibt der Harmonie-Index, auch wenn die Grenzen
        // unsortiert kommen (notesset[1] gehört zu Grenze 50).
        let schief = vec![vec![300], vec![50]];
        let b = harmonie_baender(&schief, 400);
        assert_eq!(b, vec![(0.0, 0.125, 1), (0.125, 0.75, 0)]);
        assert!(b.windows(2).all(|w| w[0].1 <= w[1].0), "{b:?}");
        // Keine Bänder: alles ohne Harmonie-Zwang.
        assert!(harmonie_baender(&[], 400).is_empty());
        assert_eq!(harm_an(0.5, &[]), None);
        // Grenzen außerhalb des Bildes werden geklemmt.
        let weit = vec![vec![900]];
        assert_eq!(harmonie_baender(&weit, 400), vec![(0.0, 1.0, 0)]);
        assert_eq!(
            harmonie_baender(&[vec![-5]], 400),
            Vec::<(f32, f32, usize)>::new(),
            "eine Grenze bei 0 lässt kein Band übrig"
        );
    }

    #[test]
    fn harm_anteil_folgt_der_basis() {
        let mut o = Oszi::new();
        // Gesamt: die ganze Breite (0 … 1).
        assert_eq!(o.harm_anteil(0, 100, 0.25, 0.75), 0.0);
        assert_eq!(o.harm_anteil(50, 100, 0.25, 0.75), 0.5);
        assert_eq!(o.harm_anteil(100, 100, 0.25, 0.75), 1.0);
        // Bereich: nur der Durchlauf-Bereich 25 % … 75 %.
        o.basis = XBasis::Bereich;
        assert_eq!(o.harm_anteil(25, 100, 0.25, 0.75), 0.0);
        assert_eq!(o.harm_anteil(50, 100, 0.25, 0.75), 0.5);
        assert_eq!(o.harm_anteil(75, 100, 0.25, 0.75), 1.0);
    }

    #[test]
    fn tiefste_note_kommt_aus_der_untersten_trefferzeile() {
        // note_fuer_zeile faellt mit y: y=0 ist die hoechste Note.
        let treffer = vec![(0.0, 10, 10, 10), (0.9, 200, 40, 120), (0.5, 50, 50, 50)];
        let (note, vol, rgb) = tiefste_note(&treffer, 120, 0).unwrap();
        assert_eq!(note, note_fuer_zeile(0.9, 120, 0));
        assert!(note < note_fuer_zeile(0.0, 120, 0), "tiefste, nicht hoechste");
        assert_eq!(vol, 127);
        assert_eq!(rgb, (200, 40, 120), "Farbe der gewaehlten Zeile");
        assert_eq!(tiefste_note(&[], 120, 0), None);
    }

    #[test]
    fn kleiner_akkord_entdoppelt_sortiert_und_deckelt() {
        let noten = vec![(64, 100), (40, 90), (64, 80), (52, 70), (67, 60), (72, 50)];
        let akkord = kleiner_akkord(noten);
        assert_eq!(akkord, vec![(40, 90), (52, 70), (64, 100), (67, 60)]);
        assert!(akkord.len() <= ALLE_MAX_NOTEN);
        // Unbrauchbare Noten fallen weg, leere Eingabe bleibt leer.
        assert_eq!(kleiner_akkord(vec![(-1, 50), (200, 50)]), Vec::new());
        assert_eq!(kleiner_akkord(Vec::new()), Vec::new());
    }

    #[test]
    fn trefferfarbe_markiert_gruen_oder_original() {
        assert_eq!(
            treffer_farbe(Anzeige::Gruen, 200, 40, 120, 1.0),
            APP_GRUEN
        );
        assert_eq!(
            treffer_farbe(Anzeige::Echt, 200, 40, 120, 1.0),
            (200, 40, 120)
        );
        assert_eq!(treffer_farbe(Anzeige::Echt, 200, 40, 120, 0.5), (100, 20, 60));
        assert_eq!(
            treffer_farbe(Anzeige::Gruen, 200, 40, 120, 0.0),
            (0, 0, 0),
            "die Masken-Amplitude dimmt auch das Gruen"
        );
    }

    #[test]
    fn anzeige_kopie_markiert_nur_nicht_hintergrund() {
        // Zwei Pixel: Hintergrund (bg) und ein Treffer in Originalfarbe.
        let buf = [10u8, 20, 30, 255, 200, 40, 120, 255];
        let kopie = anzeige_kopie(&buf, (10, 20, 30));
        assert_eq!(&kopie[0..4], &[10, 20, 30, 255], "Hintergrund bleibt");
        assert_eq!(&kopie[4..8], &[20, 235, 60, 255], "Treffer wird grün");
        // Der Puffer selbst bleibt unangetastet (Palette des Live-Scans).
        assert_eq!(&buf[4..8], &[200, 40, 120, 255]);
        // Unvollständige Pixelreste werden nicht angefasst.
        assert_eq!(anzeige_kopie(&[1, 2, 3], (0, 0, 0)), vec![1, 2, 3]);
    }

    #[test]
    fn statuszeile_zeigt_alle_schalter() {
        let mut o = Oszi::new();
        assert_eq!(
            o.text(),
            "Oszillator: Linie (1/4 Takt, Sweep, Takt-Maske, Harmonie-x Gesamt)"
        );
        o.bereich_an = true;
        o.dauer_takte = 1.0 / 64.0;
        o.durchlauf = Durchlauf::Alles;
        o.maske_an = false;
        o.basis = XBasis::Bereich;
        assert_eq!(
            o.text(),
            "Oszillator: Bereich (1/64 Takt, alle Spalten, alle y, Harmonie-x Bereich)"
        );
    }
}
