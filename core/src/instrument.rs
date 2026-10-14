//! Notenbereiche fuer die Instrumente/Samples eines Moduls.
//!
//! Der Instrument-Tab listet die Eintraege des geladenen Moduls und laesst
//! jedem einen **Notenbereich** zuweisen (z. B. C-3 bis D#5). Klingen mehrere
//! Bereiche fuer eine Note, spielen **alle** (Schichtung,
//! Nutzerentscheidung 2026-10-14); ausserhalb aller Bereiche bleibt es still.
//!
//! Solange **kein** Eintrag einen Bereich hat, ist die Zuordnung leer und die
//! Aufrufer nehmen ihren Standard (wie bisher Instrument 30) — sonst waere der
//! Modul-Klang nach dem ersten Start des Tabs verstummt.

/// Ein Modul-Eintrag mit Namen und optionalem Notenbereich (MIDI 0..=119).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    /// 0-basierter Index in libopenmpt (Instrumente oder Samples).
    pub index: i32,
    pub name: String,
    /// Untere und obere Grenze, beide eingeschlossen; `None` = ohne Bereich.
    pub von: Option<u8>,
    pub bis: Option<u8>,
}

impl Eintrag {
    /// Neuer Eintrag ohne Bereich.
    pub fn neu(index: i32, name: String) -> Self {
        Self {
            index,
            name,
            von: None,
            bis: None,
        }
    }

    /// Klingt diese Note im Bereich des Eintrags?
    pub fn trifft(&self, note: u8) -> bool {
        match (self.von, self.bis) {
            (Some(von), Some(bis)) => note >= von && note <= bis,
            _ => false,
        }
    }

    /// Bereich setzen: Grenzen werden getauscht, wenn sie verdreht sind, und
    /// auf 0..=119 geklemmt (mehr spielt libopenmpt nicht).
    pub fn bereich_setzen(&mut self, a: u8, b: u8) {
        let (a, b) = (a.min(119), b.min(119));
        self.von = Some(a.min(b));
        self.bis = Some(a.max(b));
    }

    /// Bereich wieder entfernen.
    pub fn bereich_loeschen(&mut self) {
        self.von = None;
        self.bis = None;
    }
}

/// Welche Eintraege fuer eine Note klingen.
///
/// `None` heisst: **kein** Eintrag in der Liste hat einen Bereich — der
/// Aufrufer nimmt seinen Standard. `Some(leer)` heisst: es gibt Bereiche,
/// aber die Note faellt in keinen — dann bleibt es still.
pub fn treffer(eintraege: &[Eintrag], note: u8) -> Option<Vec<i32>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mit_bereich(index: i32, name: &str, von: u8, bis: u8) -> Eintrag {
        let mut e = Eintrag::neu(index, name.to_string());
        e.bereich_setzen(von, bis);
        e
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
        assert_eq!(treffer(&liste, 60), Some(vec![0, 1]));
        assert_eq!(treffer(&liste, 50), Some(vec![0]), "nur A");
        assert_eq!(treffer(&liste, 74), Some(vec![1]), "nur B");
    }

    #[test]
    fn ausserhalb_bleibt_still() {
        let liste = vec![mit_bereich(0, "A", 48, 60)];
        assert_eq!(treffer(&liste, 61), Some(Vec::new()), "still");
        // Ohne einen einzigen Bereich gibt es keinen Treffer-Sonderfall:
        // der Aufrufer nimmt seinen Standard.
        let ohne = vec![Eintrag::neu(0, "A".to_string())];
        assert_eq!(treffer(&ohne, 60), None);
        assert_eq!(treffer(&[], 60), None);
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
