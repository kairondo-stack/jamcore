//! Notenbereiche fuer die Instrumente/Samples eines Moduls.
//!
//! Der Instrument-Tab listet die Eintraege des geladenen Moduls und laesst
//! jedem einen **Notenbereich** zuweisen (z. B. C-3 bis D#5). Klingen mehrere
//! Bereiche fuer eine Note, spielen **alle** (Schichtung,
//! Nutzerentscheidung 2026-10-14); ausserhalb aller Bereiche bleibt es still.
//!
//! Zusaetzlich hat jeder Eintrag einen **Bildbereich** `x`/`y` (Anteile der
//! Bildbreite/-hoehe, Vorgabe `0..=1` = ganzes Bild). Der Bild-Scan klingt nur,
//! wenn Note **und** x **und** y im Bereich liegen ([`treffer`]); die Fader im
//! Tab lassen sich mit dem Auswaehlhaken auf mehrere Eintraege gleichzeitig
//! setzen ([`setze_bereich`]).
//!
//! Solange **kein** Eintrag einen Bereich hat, ist die Zuordnung leer und die
//! Aufrufer nehmen ihren Standard (wie bisher Instrument 30) — sonst waere der
//! Modul-Klang nach dem ersten Start des Tabs verstummt.

/// Ein Bereich fuer **mehrere** Eintraege gleichzeitig (Knopf „Bereich auf
/// Markierte"): Notengrenzen (MIDI 0..=119) und die Bildanteile x/y (0..=1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bereich {
    pub von: i32,
    pub bis: i32,
    pub x_von: f32,
    pub x_bis: f32,
    pub y_von: f32,
    pub y_bis: f32,
}

/// Ein Modul-Eintrag mit Namen, optionalem Notenbereich (MIDI 0..=119) und
/// einem Bildbereich x/y (Anteile 0..=1, Vorgabe 0..=1 = ganzes Bild).
#[derive(Clone, Debug, PartialEq)]
pub struct Eintrag {
    /// 0-basierter Index in libopenmpt (Instrumente oder Samples).
    pub index: i32,
    pub name: String,
    /// Untere und obere Grenze, beide eingeschlossen; `None` = ohne Bereich.
    pub von: Option<u8>,
    pub bis: Option<u8>,
    /// x-Anteil der Bildbreite, in dem der Eintrag klingen darf.
    pub x_von: f32,
    pub x_bis: f32,
    /// y-Anteil der Bildhoehe, in dem der Eintrag klingen darf.
    pub y_von: f32,
    pub y_bis: f32,
}

impl Eintrag {
    /// Neuer Eintrag ohne Notenbereich; Bildbereich = ganzes Bild.
    pub fn neu(index: i32, name: String) -> Self {
        Self {
            index,
            name,
            von: None,
            bis: None,
            x_von: 0.0,
            x_bis: 1.0,
            y_von: 0.0,
            y_bis: 1.0,
        }
    }

    /// Klingt diese Note im Bereich des Eintrags?
    pub fn trifft(&self, note: u8) -> bool {
        match (self.von, self.bis) {
            (Some(von), Some(bis)) => note >= von && note <= bis,
            _ => false,
        }
    }

    /// Liegt die Bildstelle (x, y) im Bereich des Eintrags? Die Vorgabe
    /// `0.0..=1.0` trifft jede Stelle des Bildes.
    pub fn trifft_xy(&self, x: f32, y: f32) -> bool {
        x >= self.x_von && x <= self.x_bis && y >= self.y_von && y <= self.y_bis
    }

    /// Bereich setzen: Grenzen werden getauscht, wenn sie verdreht sind, und
    /// auf 0..=119 geklemmt (mehr spielt libopenmpt nicht).
    pub fn bereich_setzen(&mut self, a: u8, b: u8) {
        let (a, b) = (a.min(119), b.min(119));
        self.von = Some(a.min(b));
        self.bis = Some(a.max(b));
    }

    /// Bildbereich setzen: Grenzen werden getauscht, wenn sie verdreht sind,
    /// und auf 0..=1 geklemmt (Anteile der Bildbreite/-hoehe).
    pub fn xy_setzen(&mut self, x_von: f32, x_bis: f32, y_von: f32, y_bis: f32) {
        let (xa, xb) = (x_von.clamp(0.0, 1.0), x_bis.clamp(0.0, 1.0));
        let (ya, yb) = (y_von.clamp(0.0, 1.0), y_bis.clamp(0.0, 1.0));
        self.x_von = xa.min(xb);
        self.x_bis = xa.max(xb);
        self.y_von = ya.min(yb);
        self.y_bis = ya.max(yb);
    }

    /// Bildbereich wieder auf das ganze Bild (0..=1) stellen.
    pub fn xy_zuruecksetzen(&mut self) {
        self.x_von = 0.0;
        self.x_bis = 1.0;
        self.y_von = 0.0;
        self.y_bis = 1.0;
    }

    /// Schraenkt der Eintrag das Bild ein (nicht 0..=1)?
    pub fn xy_eingeschraenkt(&self) -> bool {
        self.x_von > 0.0 || self.x_bis < 1.0 || self.y_von > 0.0 || self.y_bis < 1.0
    }

    /// Bereich wieder entfernen.
    pub fn bereich_loeschen(&mut self) {
        self.von = None;
        self.bis = None;
    }
}

/// Welche Eintraege fuer eine Note klingen (ohne Bildanteile).
///
/// `None` heisst: **kein** Eintrag in der Liste hat einen Bereich — der
/// Aufrufer nimmt seinen Standard. `Some(leer)` heisst: es gibt Bereiche,
/// aber die Note faellt in keinen — dann bleibt es still.
pub fn treffer_note(eintraege: &[Eintrag], note: u8) -> Option<Vec<i32>> {
    if !eintraege.iter().any(|e| e.von.is_some() && e.bis.is_some()) {
        return None;
    }
    Some(
        eintraege
            .iter()
            .filter(|e| e.trifft(note))
            .map(|e| e.index)
            .collect(),
    )
}

/// Welche Eintraege an der Bildstelle (x, y) fuer eine Note klingen.
///
/// Wie [`treffer_note`], zusaetzlich auf den Bildbereich eingeschraenkt
/// (x/y als Anteile 0..=1). Ist **kein** Eintrag mit Notenbereich gesetzt,
/// bleibt die Liste leer — den „nichts gesetzt"-Fall (`None`, Standard
/// Instrument 30) erkennt der Aufrufer an [`treffer_note`].
pub fn treffer(eintraege: &[Eintrag], note: u8, x: f32, y: f32) -> Vec<i32> {
    // `treffer_note` beantwortet wie bisher „gibt es ueberhaupt Bereiche?".
    if treffer_note(eintraege, note).is_none() {
        return Vec::new();
    }
    eintraege
        .iter()
        .filter(|e| e.trifft(note) && e.trifft_xy(x, y))
        .map(|e| e.index)
        .collect()
}

/// Denselben Bereich auf **mehrere** Eintraege setzen (Knopf „Bereich auf
/// Markierte"): Notengrenzen werden geklemmt/gedreht wie bei
/// [`Eintrag::bereich_setzen`], die Bildanteile wie bei [`Eintrag::xy_setzen`].
/// Indizes ausserhalb der Liste werden uebergangen.
pub fn setze_bereich(eintraege: &mut [Eintrag], indizes: &[usize], bereich: Bereich) {
    let von = bereich.von.clamp(0, 119) as u8;
    let bis = bereich.bis.clamp(0, 119) as u8;
    for &index in indizes {
        if let Some(e) = eintraege.get_mut(index) {
            e.bereich_setzen(von, bis);
            e.xy_setzen(bereich.x_von, bereich.x_bis, bereich.y_von, bereich.y_bis);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mit_bereich(index: i32, name: &str, von: u8, bis: u8) -> Eintrag {
        let mut e = Eintrag::neu(index, name.to_string());
        e.bereich_setzen(von, bis);
        e
    }

    /// Eintrag mit Noten- **und** Bildbereich (x/y als Anteile 0..1).
    fn mit_bild(
        index: i32,
        name: &str,
        von: u8,
        bis: u8,
        x: (f32, f32),
        y: (f32, f32),
    ) -> Eintrag {
        let mut e = mit_bereich(index, name, von, bis);
        e.xy_setzen(x.0, x.1, y.0, y.1);
        e
    }

    #[test]
    fn x_bereich_klemmt() {
        let mut e = Eintrag::neu(0, "A".to_string());
        e.xy_setzen(-0.4, 1.7, 0.0, 1.0);
        assert_eq!((e.x_von, e.x_bis), (0.0, 1.0), "auf 0..1 geklemmt");
        e.xy_setzen(0.8, 0.2, 0.0, 1.0);
        assert_eq!((e.x_von, e.x_bis), (0.2, 0.8), "verdreht -> getauscht");
        e.xy_setzen(0.2, 0.8, 0.0, 1.0);
        assert_eq!((e.x_von, e.x_bis), (0.2, 0.8), "normale Lage bleibt");
    }

    #[test]
    fn y_bereich_klemmt() {
        let mut e = Eintrag::neu(0, "A".to_string());
        e.xy_setzen(0.0, 1.0, -1.0, 2.0);
        assert_eq!((e.y_von, e.y_bis), (0.0, 1.0), "auf 0..1 geklemmt");
        e.xy_setzen(0.0, 1.0, 0.75, 0.25);
        assert_eq!((e.y_von, e.y_bis), (0.25, 0.75), "verdreht -> getauscht");
        e.xy_zuruecksetzen();
        assert_eq!(
            (e.x_von, e.x_bis, e.y_von, e.y_bis),
            (0.0, 1.0, 0.0, 1.0),
            "zurueck auf das ganze Bild"
        );
    }

    #[test]
    fn ausserhalb_still() {
        // Note 48..=75, x 20..80 %, y 0..50 %.
        let liste = vec![mit_bild(0, "A", 48, 75, (0.2, 0.8), (0.0, 0.5))];
        assert_eq!(treffer(&liste, 60, 0.5, 0.25), vec![0], "mittendrin");
        assert_eq!(treffer(&liste, 60, 0.1, 0.25), Vec::<i32>::new(), "x zu klein");
        assert_eq!(treffer(&liste, 60, 0.9, 0.25), Vec::<i32>::new(), "x zu gross");
        assert_eq!(treffer(&liste, 60, 0.5, 0.6), Vec::<i32>::new(), "y zu gross");
        assert_eq!(treffer(&liste, 60, 0.5, -0.2), Vec::<i32>::new(), "y negativ");
        assert_eq!(treffer(&liste, 40, 0.5, 0.25), Vec::<i32>::new(), "Note zu tief");
        // Die Grenzen zaehlen mit (eingeschlossen).
        assert_eq!(treffer(&liste, 48, 0.2, 0.0), vec![0], "untere Grenzen");
        assert_eq!(treffer(&liste, 75, 0.8, 0.5), vec![0], "obere Grenzen");
    }

    #[test]
    fn standardbereich_ist_alles() {
        let mut e = Eintrag::neu(0, "A".to_string());
        assert_eq!(
            (e.x_von, e.x_bis, e.y_von, e.y_bis),
            (0.0, 1.0, 0.0, 1.0),
            "Vorgabe = ganzes Bild"
        );
        assert!(e.trifft_xy(0.0, 0.0));
        assert!(e.trifft_xy(1.0, 1.0));
        assert!(e.trifft_xy(0.37, 0.62));
        e.bereich_setzen(48, 75);
        let liste = vec![e];
        assert_eq!(treffer(&liste, 60, 0.0, 0.0), vec![0], "Ecke unten links");
        assert_eq!(treffer(&liste, 60, 1.0, 1.0), vec![0], "Ecke oben rechts");
    }

    #[test]
    fn mehrfach_bereich_setzen() {
        let mut liste = vec![
            Eintrag::neu(0, "A".to_string()),
            Eintrag::neu(1, "B".to_string()),
            Eintrag::neu(2, "C".to_string()),
            Eintrag::neu(3, "D".to_string()),
        ];
        let bereich = Bereich {
            von: 60,
            bis: 72,
            x_von: 0.2,
            x_bis: 0.8,
            y_von: 0.0,
            y_bis: 0.5,
        };
        setze_bereich(&mut liste, &[0, 1, 2], bereich);
        for (i, e) in liste.iter().enumerate().take(3) {
            assert_eq!((e.von, e.bis), (Some(60), Some(72)), "Note {i}");
            assert_eq!((e.x_von, e.x_bis), (0.2, 0.8), "x {i}");
            assert_eq!((e.y_von, e.y_bis), (0.0, 0.5), "y {i}");
        }
        assert_eq!(
            (
                liste[3].von,
                liste[3].bis,
                liste[3].x_von,
                liste[3].x_bis,
                liste[3].y_von,
                liste[3].y_bis
            ),
            (None, None, 0.0, 1.0, 0.0, 1.0),
            "unmarkierter Eintrag bleibt unveraendert"
        );
        // Verdrehte/zu grosse Werte werden wie beim Einzelbereich behandelt.
        let schief = Bereich {
            von: 300,
            bis: -4,
            x_von: 1.5,
            x_bis: 0.5,
            y_von: 0.3,
            y_bis: 0.1,
        };
        setze_bereich(&mut liste, &[3], schief);
        assert_eq!((liste[3].von, liste[3].bis), (Some(0), Some(119)), "Noten geklemmt");
        assert_eq!((liste[3].x_von, liste[3].x_bis), (0.5, 1.0), "x geklemmt/gedreht");
        assert_eq!((liste[3].y_von, liste[3].y_bis), (0.1, 0.3), "y getauscht");
        // Ein Index ausserhalb der Liste aendert nichts (kein Absturz).
        setze_bereich(&mut liste, &[99], bereich);
        assert_eq!((liste[3].y_von, liste[3].y_bis), (0.1, 0.3));
    }

    #[test]
    fn notenbereich_wie_bisher() {
        let liste = vec![mit_bereich(0, "A", 48, 70), mit_bereich(1, "B", 55, 75)];
        assert_eq!(treffer_note(&liste, 60), Some(vec![0, 1]));
        assert_eq!(treffer_note(&liste, 50), Some(vec![0]), "nur A");
        assert_eq!(treffer_note(&liste, 74), Some(vec![1]), "nur B");
        assert_eq!(treffer_note(&liste, 76), Some(Vec::new()), "still");
        // Ohne einen einzigen Bereich bleibt es bei `None` (Standard Instrument 30).
        assert_eq!(treffer_note(&[], 60), None);
        assert_eq!(treffer_note(&[Eintrag::neu(0, "leer".to_string())], 60), None);
        // Mit x/y an beliebiger Stelle dasselbe Bild: die Vorgabe 0..1 trifft alles.
        assert_eq!(treffer(&liste, 60, 0.13, 0.87), vec![0, 1]);
        assert_eq!(treffer(&liste, 50, 0.13, 0.87), vec![0]);
        assert_eq!(treffer(&liste, 76, 0.13, 0.87), Vec::<i32>::new());
        // Ohne gesetzte Bereiche ist auch die x/y-Abfrage still (kein Standard hier).
        assert_eq!(treffer(&[], 60, 0.5, 0.5), Vec::<i32>::new());
    }

    #[test]
    fn bereich_trifft_note() {
        // C-3 (48) bis D#5 (75), wie im Nutzerbeispiel.
        let e = mit_bereich(0, "Piano", 48, 75);
        assert!(e.trifft(48), "untere Grenze");
        assert!(e.trifft(60), "mittendrin");
        assert!(e.trifft(75), "obere Grenze");
        assert!(!e.trifft(47));
        assert!(!e.trifft(76));
        assert!(
            !Eintrag::neu(1, "leer".to_string()).trifft(60),
            "ohne Bereich"
        );
    }

    #[test]
    fn ueberlappung_spielt_alle() {
        // Zwei Eintraege teilen sich 55..70: beide klingen zusammen.
        let liste = vec![mit_bereich(0, "A", 48, 70), mit_bereich(1, "B", 55, 75)];
        assert_eq!(treffer_note(&liste, 60), Some(vec![0, 1]));
        assert_eq!(treffer_note(&liste, 50), Some(vec![0]), "nur A");
        assert_eq!(treffer_note(&liste, 74), Some(vec![1]), "nur B");
    }

    #[test]
    fn ausserhalb_bleibt_still() {
        let liste = vec![mit_bereich(0, "A", 48, 60)];
        assert_eq!(treffer_note(&liste, 61), Some(Vec::new()), "still");
        // Ohne einen einzigen Bereich gibt es keinen Treffer-Sonderfall:
        // der Aufrufer nimmt seinen Standard.
        let ohne = vec![Eintrag::neu(0, "A".to_string())];
        assert_eq!(treffer_note(&ohne, 60), None);
        assert_eq!(treffer_note(&[], 60), None);
    }

    #[test]
    fn bereich_von_groesser_bis_und_grenzen() {
        let mut e = Eintrag::neu(3, "X".to_string());
        e.bereich_setzen(75, 48);
        assert_eq!(
            (e.von, e.bis),
            (Some(48), Some(75)),
            "verdreht -> getauscht"
        );
        e.bereich_setzen(10, 200);
        assert_eq!((e.von, e.bis), (Some(10), Some(119)), "auf 119 geklemmt");
        e.bereich_loeschen();
        assert_eq!((e.von, e.bis), (None, None));
    }
}
