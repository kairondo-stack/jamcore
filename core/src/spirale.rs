//! Spiral-Scan: der Abtastpfad läuft als **Spirale** über das Bild, der Radius
//! bestimmt die Tonhöhe.
//!
//! Formeln aus den Max-Patches `spiralobpm` / `Matrizenleser_Spiral_archimedisch`
//! (siehe `docs/2026-10-15-max-patches-uebersicht.md`, Gruppe 3):
//!
//! ```text
//! Archimedes   r = a·φ            x = r·cos φ, y = r·sin φ
//! „arch ln"    r = ln(a·φ)        (zweite Familie des Patches)
//! φ wächst je Schritt um `schritt` rad (im Patch 1/2/5/9 rad)
//! Note = radius/max · tasten + offset   (Patch: `expr ($f1*$f3/$f2+$f4)`)
//! ```
//!
//! Der Punkt liegt in Bildkoordinaten um die **Bildmitte**; `a` ist damit der
//! Radiuszuwachs je rad in Pixeln (Vorgabe 1.0: zwei Schritte à 2 rad wachsen
//! um 4 px). `radius()` ist der Abstand der **geklemmten** Bildzelle von der
//! Mitte — genau der Radius, den der Scan tatsächlich abtastet; `radius_roh()`
//! ist der ungeklemmte mathematische Radius (a·φ bzw. ln(a·φ)) und dient dem
//! Sweep-Ende, wenn die Spirale das Bild verlassen hat.
//!
//! **Festlegungen, die der Patch offen lässt:**
//! - `ln(a·φ)` ist für `a·φ ≤ 1` null oder negativ; der Radius wird dort auf 0
//!   geklemmt, damit beide Familien **im Zentrum beginnen** (sonst läge der
//!   Startpunkt gespiegelt gegenüber).
//! - Der Radius wird beim Bildrand geklemmt: der Scan liest immer eine gültige
//!   Zelle, die Tonhöhe sättigt am Rand (die Note wird auf 0…119 geklemmt).
//! - `max` der Notenformel ist der größte Abstand von der Mitte im Bild
//!   (halbe Diagonale) — der Patch nennt ihn `sqrtmax`.

/// Welche Spiralfamilie den Pfad beschreibt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Familie {
    /// `r = a·φ` (Archimedes).
    Archimedes,
    /// `r = ln(a·φ)` — die „arch ln"-Familie.
    Log,
}

/// Vorgabe für `a`: 1 Pixel Radiuszuwachs je rad.
pub const A_VORGABE: f64 = 1.0;
/// Kleinster brauchbarer `a`-Wert (0 wäre eine stehende Spirale im Zentrum).
pub const A_MIN: f64 = 0.01;
/// Größter `a`-Wert (das Bild ist nach wenigen rad verlassen).
pub const A_MAX: f64 = 100.0;
/// Vorgabe für die Schrittweite: 2 rad je Schritt (Patch-Werte 1/2/5/9).
pub const SCHRITT_VORGABE: f64 = 2.0;
/// Kleinste Schrittweite; darunter bewegt sich der Punkt kaum.
pub const SCHRITT_MIN: f64 = 0.01;
/// Größte Schrittweite — 9 rad ist der größte Wert der Patch-Reihe.
pub const SCHRITT_MAX: f64 = 9.0;

/// Zustand des Spiral-Scans: Familie, Radiusfaktor `a`, Winkel `phi` und
/// Schrittweite in rad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spirale {
    pub familie: Familie,
    pub a: f64,
    pub phi: f64,
    pub schritt: f64,
}

impl Spirale {
    /// Neue Spirale im Zentrum (`phi = 0`) mit den Vorgabewerten.
    pub fn neu(familie: Familie) -> Self {
        Self {
            familie,
            a: A_VORGABE,
            phi: 0.0,
            schritt: SCHRITT_VORGABE,
        }
    }

    /// Schrittweite in rad setzen (geklemmt auf [`SCHRITT_MIN`]…[`SCHRITT_MAX`];
    /// NaN wird zur kleinsten brauchbaren Zahl).
    pub fn setze_schritt(&mut self, rad: f64) {
        self.schritt = if rad.is_finite() {
            rad.clamp(SCHRITT_MIN, SCHRITT_MAX)
        } else {
            SCHRITT_MIN
        };
    }

    /// Radiusfaktor `a` setzen (geklemmt auf [`A_MIN`]…[`A_MAX`]; NaN wird zur
    /// kleinsten brauchbaren Zahl).
    pub fn setze_a(&mut self, a: f64) {
        self.a = if a.is_finite() {
            a.clamp(A_MIN, A_MAX)
        } else {
            A_MIN
        };
    }

    /// Einen Schritt weiter: `phi += schritt`.
    pub fn weiter(&mut self) {
        self.phi += self.schritt;
    }

    /// Ungeklemmter mathematischer Radius (`a·φ` bzw. `ln(a·φ)`, nie negativ).
    pub fn radius_roh(&self) -> f64 {
        match self.familie {
            Familie::Archimedes => (self.a * self.phi).max(0.0),
            // ln(a·φ) ist für a·φ ≤ 1 null oder negativ: dort bleibt der Punkt
            // im Zentrum (siehe Modulkopf).
            Familie::Log => {
                let x = self.a * self.phi;
                if x > 1.0 {
                    x.ln()
                } else {
                    0.0
                }
            }
        }
    }

    /// Bildzelle des aktuellen Punktes (Mitte + r·(cos φ, sin φ)), auf
    /// `0…w-1`/`0…h-1` geklemmt — der Scan liest immer eine gültige Zelle.
    pub fn punkt(&self, w: f64, h: f64) -> (i32, i32) {
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
            return (0, 0);
        }
        let r = self.radius_roh();
        let x = w / 2.0 + r * self.phi.cos();
        let y = h / 2.0 + r * self.phi.sin();
        (klemme(x, w), klemme(y, h))
    }

    /// Radius des aktuellen Punktes in Pixeln: Abstand der (am Bildrand
    /// geklemmten) Bildzelle von der Bildmitte — genau der Radius, den der
    /// Scan abtastet.
    pub fn radius(&self, w: f64, h: f64) -> f64 {
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
            return 0.0;
        }
        let (px, py) = self.punkt(w, h);
        let dx = px as f64 - w / 2.0;
        let dy = py as f64 - h / 2.0;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Tonhöhe aus dem Radius: `radius/max · tasten + offset`, geklemmt auf
/// 0…119 (Renoise-Notenbereich). Ohne brauchbares `max` (0, negativ, NaN)
/// bleibt nur der `offset` (der Grundton darf negativ sein, das Ergebnis
/// nicht).
pub fn note_aus_radius(radius: f64, max: f64, tasten: i32, offset: i32) -> i32 {
    let roh = if !radius.is_finite() || !max.is_finite() || max <= 0.0 || tasten <= 0 {
        offset as f64
    } else {
        radius / max * tasten as f64 + offset as f64
    };
    if !roh.is_finite() {
        return offset.clamp(0, 119);
    }
    (roh.round() as i64).clamp(0, 119) as i32
}

/// Bildkoordinate `v` auf `0…n-1` klemmen; unbrauchbare Werte werden 0.
fn klemme(v: f64, n: f64) -> i32 {
    if !v.is_finite() || !n.is_finite() || n < 1.0 {
        return 0;
    }
    v.round().clamp(0.0, n - 1.0) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archimedes_waechst() {
        let mut s = Spirale::neu(Familie::Archimedes);
        let r0 = s.radius(640.0, 480.0);
        for _ in 0..5 {
            s.weiter();
        }
        let r1 = s.radius(640.0, 480.0);
        assert!(r1 > r0, "Archimedes waechst: {r0} -> {r1}");
        // Fünf Schritte à 2 rad mit a = 1: genau 10 px mathematischer Radius.
        assert!(
            (s.radius_roh() - 5.0 * SCHRITT_VORGABE * A_VORGABE).abs() < 1e-9,
            "phi waechst je Schritt: {}",
            s.radius_roh()
        );
        // Die Bildzelle ist ganzzahlig gerundet, ihr Radius weicht daher um
        // höchstens ein Pixel ab (hier 9,43 statt 10).
        assert!(
            (r1 - s.radius_roh()).abs() < 1.0,
            "gerundete Zelle liegt auf dem Radius: {r1} vs {}",
            s.radius_roh()
        );
    }

    #[test]
    fn log_waechst_langsamer() {
        let mut arch = Spirale::neu(Familie::Archimedes);
        let mut log = Spirale::neu(Familie::Log);
        let vorher = log.radius_roh();
        for _ in 0..50 {
            arch.weiter();
            log.weiter();
        }
        // Nach 50 Schritten à 2 rad: φ = 100, ln(100) = 4,605…
        assert!(
            log.radius_roh() > vorher,
            "Log waechst ueberhaupt: {vorher} -> {}",
            log.radius_roh()
        );
        assert!(
            (log.radius_roh() - 100.0f64.ln()).abs() < 1e-9,
            "r = ln(a*phi): {}",
            log.radius_roh()
        );
        assert!(
            log.radius(640.0, 480.0) < arch.radius(640.0, 480.0),
            "ln waechst langsamer als a*phi: {} < {}",
            log.radius(640.0, 480.0),
            arch.radius(640.0, 480.0)
        );
    }

    #[test]
    fn punkt_bleibt_im_bild() {
        for familie in [Familie::Archimedes, Familie::Log] {
            let mut s = Spirale::neu(familie);
            for i in 0..4000 {
                s.weiter();
                let (x, y) = s.punkt(320.0, 240.0);
                assert!(
                    (0..320).contains(&x) && (0..240).contains(&y),
                    "{familie:?} Schritt {i}: ({x}, {y}) liegt nicht im Bild"
                );
            }
        }
        // Unbrauchbare Maße liefern die Ecke 0/0 statt zu paniken.
        let s = Spirale::neu(Familie::Archimedes);
        assert_eq!(s.punkt(0.0, 0.0), (0, 0));
        assert_eq!(s.punkt(-5.0, 10.0), (0, 0));
        assert_eq!(s.punkt(f64::NAN, 10.0), (0, 0));
    }

    #[test]
    fn note_aus_radius_normiert() {
        // Patch: `expr ($f1*$f3/$f2+$f4)` = radius/max·tasten + offset.
        assert_eq!(note_aus_radius(0.0, 100.0, 120, 0), 0, "Zentrum = tiefste Note");
        assert_eq!(note_aus_radius(50.0, 100.0, 120, 0), 60, "halber Radius");
        assert_eq!(note_aus_radius(200.0, 400.0, 120, 0), 60, "auch normiert");
        assert_eq!(
            note_aus_radius(100.0, 100.0, 120, 0),
            119,
            "120 wird auf 119 geklemmt"
        );
        assert_eq!(
            note_aus_radius(100.0, 100.0, 120, -12),
            108,
            "offset wirkt als Grundton"
        );
        assert_eq!(note_aus_radius(10.0, 100.0, 120, 12), 24, "mit offset");
        assert_eq!(note_aus_radius(-1.0, 100.0, 120, 0), 0, "negativer Radius");
        assert_eq!(note_aus_radius(f64::NAN, 100.0, 120, 0), 0, "NaN");
        assert_eq!(
            note_aus_radius(10.0, 0.0, 120, 5),
            5,
            "ohne max bleibt nur der offset"
        );
        assert_eq!(
            note_aus_radius(100.0, 100.0, 200, 0),
            119,
            "auch grosse Tonumfaenge enden bei 119"
        );
    }

    #[test]
    fn schritt_und_a_geklemmt() {
        let mut s = Spirale::neu(Familie::Archimedes);
        assert_eq!(s.schritt, SCHRITT_VORGABE, "Vorgabe 2 rad");
        assert_eq!(s.a, A_VORGABE, "Vorgabe a = 1");
        // Zu klein/negativ: die Spirale soll sich noch bewegen.
        s.setze_schritt(-4.0);
        assert_eq!(s.schritt, SCHRITT_MIN);
        s.setze_schritt(0.0);
        assert_eq!(s.schritt, SCHRITT_MIN);
        // Zu gross: die Patch-Reihe endet bei 9 rad.
        s.setze_schritt(1.0e9);
        assert_eq!(s.schritt, SCHRITT_MAX);
        s.setze_schritt(SCHRITT_VORGABE);
        assert_eq!(s.schritt, SCHRITT_VORGABE, "gueltiger Wert bleibt");
        // a ebenso: nie null/negativ, nie unendlich gross.
        s.setze_a(0.0);
        assert_eq!(s.a, A_MIN);
        s.setze_a(-2.0);
        assert_eq!(s.a, A_MIN);
        s.setze_a(1.0e9);
        assert_eq!(s.a, A_MAX);
        s.setze_a(0.5);
        assert_eq!(s.a, 0.5, "gueltiger Wert bleibt");
        // NaN ist kein Wert: kleinste brauchbare Zahl.
        s.setze_schritt(f64::NAN);
        assert_eq!(s.schritt, SCHRITT_MIN);
        s.setze_a(f64::NAN);
        assert_eq!(s.a, A_MIN);
    }

    #[test]
    fn spirale_beginnt_im_zentrum() {
        for familie in [Familie::Archimedes, Familie::Log] {
            let s = Spirale::neu(familie);
            assert_eq!(s.phi, 0.0, "{familie:?}: phi startet bei null");
            assert_eq!(
                s.punkt(320.0, 240.0),
                (160, 120),
                "{familie:?} startet im Zentrum"
            );
            assert_eq!(s.radius(320.0, 240.0), 0.0, "{familie:?}: Radius null");
        }
    }
}
