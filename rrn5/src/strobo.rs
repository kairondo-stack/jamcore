// Stroboskop-Ansicht der Bildquelle — dünne Hülle um `jamcore::stroboskop`.
//
// Die Formel (Lissajous-Phase, endlos in eine Richtung, Quellspalte
// `(Ansicht − Versatz) mod Breite`) liegt getestet im Kern; hier steht nur der
// Zustand des Desktop-rrn5 (an/aus, Richtung, Umdrehungen, Takte je Zyklus)
// samt Takt-Zähler und die Umrechnung „Zustand → Versatz in Pixeln". Vorbild
// ist die Android-App (`android/src/image.rs`, Felder `strobo_*`), damit beide
// dasselbe zeigen.
//
// Die Ansicht selbst zeichnet `View::present_versetzt` (main.rs) aus
// **demselben** Pixelpuffer — es gibt keinen zweiten Puffer; der Bild-Scan
// liest die Quellspalte über `quell_spalte`.

use std::time::Instant;

use jamcore::{lissa, stroboskop, takte};

/// Vorgabe der App: zwei Takte je Zyklus, eine Umdrehung, nach rechts.
pub const TAKTE_VORGABE: f64 = 2.0;

pub struct Stroboskop {
    /// Ansicht an/aus.
    pub an: bool,
    /// Richtung: `true` = nach rechts (Bild wandert rechts heraus, links hinein).
    pub rechts: bool,
    /// Umdrehungen je Zyklus (geklemmt auf 1/64 … 16).
    pub umdrehungen: f64,
    /// Takte je Zyklus (geklemmt auf 1/36 … 36).
    pub takte: f64,
    /// Takt-Zähler seit dem Einschalten; läuft mit dem Tempo der App.
    takte_seit_start: f64,
    /// Zeitgeber für den Takt-Zähler.
    letzte_zeit: Instant,
    /// Zuletzt gerechneter Versatz in Pixeln (0, solange die Ansicht aus ist).
    versatz_px: i32,
}

impl Default for Stroboskop {
    fn default() -> Self {
        Self::new()
    }
}

impl Stroboskop {
    pub fn new() -> Self {
        Self {
            an: false,
            rechts: true,
            umdrehungen: lissa::UMDREHUNGEN_VORGABE,
            takte: TAKTE_VORGABE,
            takte_seit_start: 0.0,
            letzte_zeit: Instant::now(),
            versatz_px: 0,
        }
    }

    /// Ansicht umschalten; beim Einschalten beginnt der Zyklus bei null.
    pub fn umschalten(&mut self) {
        self.an = !self.an;
        self.takte_seit_start = 0.0;
        self.letzte_zeit = Instant::now();
        self.versatz_px = 0;
    }

    /// Takt-Zähler fortschreiben und den Versatz in Pixeln liefern.
    ///
    /// `bpm` ist das Tempo der App; der Zähler läuft wie in der Android-App mit
    /// `bpm/60` je Sekunde. Unbrauchbare Tempi fallen auf 120 zurück, und ein
    /// großer Zeitsprung (Fenster verschoben, angehaltener Rechner) wird auf
    /// 0,25 s gedeckelt, damit das Bild nicht springt.
    pub fn schritt(&mut self, bpm: f64, breite: i32) -> i32 {
        let jetzt = Instant::now();
        let dt = jetzt
            .duration_since(self.letzte_zeit)
            .as_secs_f64()
            .min(0.25);
        self.letzte_zeit = jetzt;
        if !self.an {
            self.versatz_px = 0;
            return 0;
        }
        let tempo = if bpm.is_finite() && bpm > 0.0 { bpm } else { 120.0 };
        self.takte_seit_start += dt * tempo / 60.0;
        self.versatz_px = self.versatz_fuer(self.takte_seit_start, breite);
        self.versatz_px
    }

    /// Versatz in Pixeln für einen Takt-Stand (Formel aus dem Kern).
    fn versatz_fuer(&self, takte_seit_start: f64, breite: i32) -> i32 {
        let anteil = stroboskop::versatz_anteil(
            takte_seit_start,
            self.takte,
            self.umdrehungen,
            self.rechts,
        );
        stroboskop::versatz_pixel(anteil, breite)
    }

    /// Versatz der Ansicht in Pixeln (aus = 0).
    pub fn versatz_px(&self) -> i32 {
        if self.an {
            self.versatz_px
        } else {
            0
        }
    }

    /// Quellspalte hinter einer Spalte der bewegten Ansicht; ohne Stroboskop
    /// bleibt die Spalte unverändert (der Scan liest dann direkt).
    pub fn quell_spalte(&self, ansicht_x: i32, breite: i32) -> i32 {
        if !self.an {
            return ansicht_x;
        }
        stroboskop::quell_spalte(ansicht_x, self.versatz_px, breite)
    }

    /// Richtung umdrehen — die Stelle im Bild bleibt stehen (kein Sprung).
    pub fn richtung_umdrehen(&mut self) {
        let alt = self.versatz_anteil_jetzt();
        self.rechts = !self.rechts;
        self.phase_halten(alt);
    }

    /// Umdrehungen durch `n` teilen (Faderknopf „÷2").
    pub fn umdrehungen_teile(&mut self, n: f64) {
        let alt = self.versatz_anteil_jetzt();
        self.umdrehungen = lissa::teile_umdrehungen(self.umdrehungen, n);
        self.phase_halten(alt);
    }

    /// Umdrehungen mit `n` malnehmen (Faderknopf „×2").
    pub fn umdrehungen_vielfache(&mut self, n: f64) {
        let alt = self.versatz_anteil_jetzt();
        self.umdrehungen = lissa::vielfache_umdrehungen(self.umdrehungen, n);
        self.phase_halten(alt);
    }

    /// Takte je Zyklus durch `n` teilen.
    pub fn takte_teile(&mut self, n: f64) {
        let alt = self.versatz_anteil_jetzt();
        self.takte = takte::teile(self.takte, n);
        self.phase_halten(alt);
    }

    /// Takte je Zyklus mit `n` malnehmen.
    pub fn takte_vielfache(&mut self, n: f64) {
        let alt = self.versatz_anteil_jetzt();
        self.takte = takte::vielfache(self.takte, n);
        self.phase_halten(alt);
    }

    /// Anzeige „1 Umdrehung / 2 Takte" (wie die App).
    pub fn text(&self) -> String {
        format!(
            "{} / {}",
            lissa::text_umdrehungen(self.umdrehungen),
            takte::text(self.takte)
        )
    }

    /// Versatz der aktuellen Phase als Anteil der Bildbreite (0…1).
    fn versatz_anteil_jetzt(&self) -> f64 {
        stroboskop::versatz_anteil(
            self.takte_seit_start,
            self.takte,
            self.umdrehungen,
            self.rechts,
        )
    }

    /// „Takte seit Start" so setzen, dass der Versatz `alt` stehen bleibt —
    /// sonst springt das Bild beim Ändern von Richtung/Rate/Zyklus (Fix aus dem
    /// Android-Review, dort `strobo_phase_erhalten`). Anders als dort wird auch
    /// die Richtung sprungfrei umgestellt: die Stelle im Bild bleibt, nur die
    /// Bewegung dreht sich um.
    fn phase_halten(&mut self, alt: f64) {
        if !self.an {
            // Aus: der Takt-Zähler bleibt liegen (Einschalten beginnt neu).
            return;
        }
        let vorzeichen = if self.rechts { 1.0 } else { -1.0 };
        self.takte_seit_start = vorzeichen * alt * self.takte / self.umdrehungen.max(1e-9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aus_bleibt_bei_null() {
        let mut s = Stroboskop::new();
        assert_eq!(s.schritt(120.0, 100), 0, "aus bewegt nichts");
        assert_eq!(s.versatz_px(), 0);
        assert_eq!(s.quell_spalte(7, 100), 7, "ohne Stroboskop bleibt die Spalte");
    }

    #[test]
    fn versatz_folgt_der_formel_des_kerns() {
        let s = Stroboskop::new();
        // Vorgabe: 2 Takte je Zyklus, 1 Umdrehung, nach rechts.
        assert_eq!(s.versatz_fuer(0.0, 100), 0);
        assert_eq!(s.versatz_fuer(1.0, 100), 50, "halber Zyklus = halbe Breite");
        assert_eq!(s.versatz_fuer(2.0, 100), 0, "voller Zyklus = wieder null");
        // Links läuft es gespiegelt.
        let mut links = Stroboskop::new();
        links.rechts = false;
        assert_eq!(links.versatz_fuer(1.0, 100), 50);
        assert_eq!(links.versatz_fuer(0.5, 100), 75, "links ist gespiegelt");
    }

    #[test]
    fn schritt_nimmt_den_takt_stand() {
        let mut s = Stroboskop::new();
        s.umschalten();
        assert!(s.an);
        // Der Takt-Zähler steht bei einem halben Zyklus (1 von 2 Takten) →
        // der Schritt darf nur um die winzige echte Laufzeit daneben liegen.
        s.takte_seit_start = 1.0;
        let px = s.schritt(120.0, 100);
        assert!(px.abs_diff(50) <= 1, "halber Zyklus: {px}");
        assert_eq!(s.versatz_px(), px, "der Versatz wird gemerkt");
        // Und die Quellspalte liegt hinter der Ansicht.
        assert_eq!(s.quell_spalte(0, 100), (0 - px).rem_euclid(100));
    }

    #[test]
    fn umschalten_beginnt_neu() {
        let mut s = Stroboskop::new();
        s.umschalten();
        s.takte_seit_start = 1.5;
        s.schritt(120.0, 100);
        s.umschalten();
        assert!(!s.an);
        assert_eq!(s.versatz_px(), 0);
        s.umschalten();
        assert!(s.an);
        assert_eq!(s.versatz_px(), 0, "nach dem Einschalten beginnt der Zyklus neu");
    }

    #[test]
    fn richtung_und_raten_halten_die_phase() {
        let mut s = Stroboskop::new();
        s.umschalten();
        s.takte_seit_start = 1.5;
        let vorher = s.versatz_fuer(s.takte_seit_start, 200);
        s.richtung_umdrehen();
        assert_eq!(s.versatz_fuer(s.takte_seit_start, 200), vorher, "Richtung");
        s.umdrehungen_teile(2.0);
        assert_eq!(s.versatz_fuer(s.takte_seit_start, 200), vorher, "÷2");
        s.umdrehungen_vielfache(2.0);
        assert_eq!(s.versatz_fuer(s.takte_seit_start, 200), vorher, "×2");
        s.takte_vielfache(3.0);
        assert_eq!(s.versatz_fuer(s.takte_seit_start, 200), vorher, "×3 Takte");
        s.takte_teile(3.0);
        assert_eq!(s.versatz_fuer(s.takte_seit_start, 200), vorher, "÷3 Takte");
    }

    #[test]
    fn raten_sind_geklemmt_und_haben_text() {
        let mut s = Stroboskop::new();
        s.umschalten();
        for _ in 0..20 {
            s.umdrehungen_vielfache(2.0);
        }
        assert_eq!(s.umdrehungen, lissa::UMDREHUNGEN_MAX);
        for _ in 0..20 {
            s.takte_vielfache(3.0);
        }
        assert_eq!(s.takte, takte::MAX);
        assert_eq!(s.text(), "16 Umdrehungen / 36 Takte");
        for _ in 0..40 {
            s.umdrehungen_teile(3.0);
        }
        assert_eq!(s.umdrehungen, lissa::UMDREHUNGEN_MIN);
    }

    #[test]
    fn aus_laesst_die_raten_in_ruhe() {
        // Bei ausgeschalteter Ansicht darf ein Ratenwechsel die Phase nicht
        // anfassen (sonst springt das Bild beim Einschalten).
        let mut s = Stroboskop::new();
        s.takte_seit_start = 3.0;
        s.umdrehungen_teile(2.0);
        s.takte_teile(2.0);
        s.richtung_umdrehen();
        assert_eq!(s.takte_seit_start, 3.0);
    }
}
