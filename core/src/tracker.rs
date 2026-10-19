//! Tracker-Ansicht: Zellen, Kanäle und Spaltenlogik zum polyphonen Raster.
//!
//! Seit 2026-10-19 trägt jede Note des Rasters einen **Kanal** (0…63,
//! [`crate::grid::Note::channel`]). Eine **Zelle** ist die Kombination aus
//! Schritt und Kanal und hält höchstens eine Note. Die Raster-Ansicht arbeitet
//! weiter schrittweise über [`crate::grid::Pattern::toggle`] (ein Schritt, eine
//! Note, Kanal 0), die Tracker-Ansicht zellenweise über [`setzen`] und
//! [`loeschen`]; [`nach_kanal`] liefert die Spalte eines Kanals als Zellenvektor
//! (ein Eintrag je Schritt) für die Anzeige.
//!
//! Zellentext: `C-4 01` (Notenname und Instrument), leere Zellen sind
//! `--- --`. Die Volume-Spalte (`C-4 01 v64`, Impulse-Tracker-Schreibweise)
//! zeigt [`zelle_text`], sobald [`crate::grid::Note`] ein `volume`-Feld
//! bekommt — heute gibt es keines, deshalb steht hier auch keines (die Anzeige
//! wäre sonst erfunden).

use crate::grid::{Note, Pattern};

/// Notenname des Rasters ([`crate::grid::note_name`], Renoise-Schreibweise:
/// 60 ist `C-4`, negative Tonhöhen sind `Off`). Wie überall in der App, damit
/// Raster, Instrumente und CSV dieselben Namen zeigen.
pub use crate::grid::note_name;

/// Zahl der Kanäle der Tracker-Ansicht (0…63).
pub const KANAELE: u8 = 64;

/// Kanal in den gültigen Bereich 0..=63 klemmen (unten und oben).
pub fn klemme_kanal(k: i32) -> u8 {
    k.clamp(0, i32::from(KANAELE) - 1) as u8
}

/// Spaltennummer eines Kanals (0…63) für die Tracker-Anzeige; geklemmt wie
/// [`klemme_kanal`], damit nie eine Spalte außerhalb der Tabelle entsteht.
pub fn kanal_spalte(kanal: u8) -> usize {
    klemme_kanal(i32::from(kanal)) as usize
}

/// Text einer Tracker-Zelle: `C-4 01` (Notenname und zweistelliges Instrument),
/// leere Zellen sind `--- --` (sechs Zeichen, wie eine belegte Zelle).
///
/// Volume gibt es im Notenmodell noch nicht: die Anzeige wäre `C-4 01 v64`
/// (angehängt mit einem Leerzeichen, `v` + zweistelliger Wert 0…64). Sobald
/// `Note` ein `volume`-Feld hat, gehört hier
/// `if let Some(v) = note.volume { text += &format!(" v{v:02}") }` hinzu.
pub fn zelle_text(note: Option<&Note>) -> String {
    match note {
        None => "--- --".to_string(),
        // Instrument zweistellig und dezimal — wie der CSV-Austausch mit dem
        // Desktop (`timestamp,instrument,pitch,glide`).
        Some(note) => format!("{} {:02}", note_name(note.pitch), note.instrument),
    }
}

/// Eine Spalte der Tracker-Ansicht: je Schritt des Patterns eine Zelle
/// (Index = Schritt), `None` heißt leer.
///
/// Trägt eine Zelle mehrere Noten (entsteht nur über die schrittweisen
/// Raster-Funktionen), gewinnt die letzte — wie beim CSV-Einlesen.
pub fn nach_kanal(pattern: &Pattern, kanal: u8) -> Vec<Option<Note>> {
    let kanal = klemme_kanal(i32::from(kanal));
    let mut zellen = vec![None; pattern.steps_per_cycle() as usize];
    for note in pattern.notes().iter().filter(|n| n.channel == kanal) {
        if let Some(zelle) = zellen.get_mut(note.step as usize) {
            *zelle = Some(*note);
        }
    }
    zellen
}

/// Setzt eine Note in ihre Zelle (Schritt + Kanal) und **ersetzt**, was dort
/// stand — es entsteht kein Duplikat. Der Kanal wird auf 0..=63 geklemmt,
/// Schritte ausserhalb der Patternlänge werden umgebrochen (wie
/// [`Pattern::toggle`]); danach ist das Pattern nach Schritt und Kanal sortiert.
///
/// Rückgabe: `true`, wenn die Zelle vorher leer war („angelegt“).
pub fn setzen(pattern: &mut Pattern, note: Note) -> bool {
    let step = note.step % pattern.steps_per_cycle();
    let kanal = klemme_kanal(i32::from(note.channel));
    let war_leer = pattern.zelle(step, kanal).is_none();
    pattern
        .noten_mut()
        .retain(|n| !(n.step == step && n.channel == kanal));
    pattern.noten_mut().push(Note {
        step,
        channel: kanal,
        ..note
    });
    sortiere(pattern);
    war_leer
}

/// Löscht die Note in genau dieser Zelle (Schritt + Kanal).
///
/// Schritte außerhalb der Patternlänge treffen nichts — anders als [`setzen`]
/// wird hier **nicht** umgebrochen, damit nie eine andere Zelle als die
/// gemeinte verschwindet. Der Kanal wird geklemmt.
///
/// Rückgabe: `true`, wenn eine Note gelöscht wurde.
pub fn loeschen(pattern: &mut Pattern, step: u16, kanal: u8) -> bool {
    if step >= pattern.steps_per_cycle() {
        return false;
    }
    let kanal = klemme_kanal(i32::from(kanal));
    let vorher = pattern.notes().len();
    pattern
        .noten_mut()
        .retain(|n| !(n.step == step && n.channel == kanal));
    pattern.notes().len() != vorher
}

/// Sortiert die Noten des Patterns: erst nach Schritt, dann nach Kanal
/// (Gleichstand: Tonhöhe) — die Reihenfolge der Tracker-Zeilen.
pub fn sortiere(pattern: &mut Pattern) {
    pattern
        .noten_mut()
        .sort_by_key(|n| (n.step, n.channel, n.pitch));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(step: u16, kanal: u8, pitch: i32) -> Note {
        Note {
            step,
            instrument: 1,
            pitch,
            glide: false,
            channel: kanal,
        }
    }

    #[test]
    fn kanal_wird_geklemmt() {
        assert_eq!(klemme_kanal(0), 0);
        assert_eq!(klemme_kanal(63), 63, "letzter Kanal");
        assert_eq!(klemme_kanal(64), 63, "ueber 63 wird geklemmt");
        assert_eq!(klemme_kanal(999), 63);
        assert_eq!(klemme_kanal(-1), 0, "negativ wird geklemmt");
        // Auch das Setzen landet in der geklemmten Zelle.
        let mut p = Pattern::with_steps(16);
        assert!(setzen(&mut p, note(2, 200, 60)));
        assert_eq!(p.notes()[0].channel, 63);
        assert_eq!(nach_kanal(&p, 63)[2].map(|n| n.channel), Some(63));
        // KANAELE ist die Zahl der Spalten, der letzte Kanal ist KANAELE - 1.
        assert_eq!(kanal_spalte(63), KANAELE as usize - 1);
        assert_eq!(kanal_spalte(200), KANAELE as usize - 1);
    }

    #[test]
    fn zelle_text_note_und_instrument() {
        // Renoise-Schreibweise des vorhandenen `grid::note_name`: 60 = C-4.
        assert_eq!(zelle_text(Some(&note(0, 0, 60))), "C-4 01");
        assert_eq!(zelle_text(Some(&note(0, 3, 61))), "C#4 01");
        let mut tief = note(0, 0, 0);
        tief.instrument = 10;
        assert_eq!(zelle_text(Some(&tief)), "C--1 10", "Instrument zweistellig");
        let mut leer_instrument = note(0, 0, 72);
        leer_instrument.instrument = 0;
        assert_eq!(zelle_text(Some(&leer_instrument)), "C-5 00");
    }

    #[test]
    fn leere_zelle_ist_strich() {
        assert_eq!(zelle_text(None), "--- --");
        assert_eq!(
            zelle_text(None).len(),
            zelle_text(Some(&note(0, 0, 60))).len(),
            "leere und belegte Zelle sind gleich breit (Monospace-Tabelle)"
        );
    }

    #[test]
    fn setzen_ersetzt_dieselbe_zelle() {
        let mut p = Pattern::with_steps(16);
        assert!(setzen(&mut p, note(4, 0, 60)), "leere Zelle: angelegt");
        let mut zweite = note(4, 0, 64);
        zweite.instrument = 2;
        assert!(!setzen(&mut p, zweite), "belegte Zelle: ersetzt");
        assert_eq!(p.notes().len(), 1, "kein Duplikat in der Zelle");
        assert_eq!(p.notes()[0].pitch, 64, "die neue Note gewinnt");
        assert_eq!(p.notes()[0].instrument, 2);
        // Ein anderer Kanal auf demselben Schritt ist eine eigene Zelle.
        assert!(setzen(&mut p, note(4, 7, 67)), "andere Zelle bleibt frei");
        assert_eq!(p.notes().len(), 2);
        // Schritt ausserhalb der Patternlaenge wird umgebrochen (wie toggle).
        assert!(!setzen(&mut p, note(20, 0, 62)));
        assert_eq!(p.notes().len(), 2, "Schritt 20 = Schritt 4");
        assert_eq!(p.notes()[0].pitch, 62);
    }

    #[test]
    fn loeschen_trifft_nur_die_zelle() {
        let mut p = Pattern::with_steps(16);
        setzen(&mut p, note(3, 0, 60));
        setzen(&mut p, note(3, 5, 62));
        setzen(&mut p, note(4, 0, 64));
        assert!(loeschen(&mut p, 3, 0), "Zelle (3, 0) geloescht");
        assert_eq!(p.notes().len(), 2);
        assert_eq!(p.zelle(3, 5).map(|n| n.pitch), Some(62), "Kanal 5 bleibt");
        assert_eq!(p.zelle(4, 0).map(|n| n.pitch), Some(64), "Schritt 4 bleibt");
        assert!(!loeschen(&mut p, 3, 0), "dieselbe Zelle ist jetzt leer");
        assert!(!loeschen(&mut p, 99, 0), "hinter der Patternlaenge");
        assert_eq!(p.notes().len(), 2);
        // Kanäle werden geklemmt, also trifft auch 200 die Zelle 63.
        setzen(&mut p, note(6, 63, 67));
        assert!(loeschen(&mut p, 6, 200), "200 wird zu 63 geklemmt");
        assert_eq!(p.notes().len(), 2);
    }

    #[test]
    fn nach_kanal_liefert_je_schritt() {
        let mut p = Pattern::with_steps(8);
        setzen(&mut p, note(0, 1, 60));
        setzen(&mut p, note(3, 1, 64));
        setzen(&mut p, note(3, 2, 67));
        let spalte = nach_kanal(&p, 1);
        assert_eq!(spalte.len(), 8, "ein Eintrag je Schritt");
        assert_eq!(spalte[0].map(|n| n.pitch), Some(60));
        assert_eq!(spalte[3].map(|n| n.pitch), Some(64));
        assert!(spalte[1].is_none(), "leerer Schritt");
        assert!(spalte[2].is_none());
        assert!(
            spalte.iter().all(|z| z.is_none_or(|n| n.channel == 1)),
            "nur der eigene Kanal"
        );
        // Kanal 63 und der geklemmte Wert 200 treffen dieselbe Spalte.
        setzen(&mut p, note(5, 63, 72));
        assert_eq!(nach_kanal(&p, 200)[5].map(|n| n.pitch), Some(72));
        // Leerer Kanal und kurzes Pattern.
        assert!(nach_kanal(&p, 40).iter().all(|z| z.is_none()));
        assert_eq!(nach_kanal(&Pattern::with_steps(1), 0).len(), 1);
    }

    #[test]
    fn sortierung_nach_schritt_und_kanal() {
        let mut p = Pattern::with_steps(16);
        // Unsortiert aufbauen (nur der Kern kommt an die Notenliste heran).
        p.noten_mut().push(note(2, 3, 60));
        p.noten_mut().push(note(1, 0, 62));
        p.noten_mut().push(note(2, 1, 61));
        p.noten_mut().push(note(0, 7, 64));
        sortiere(&mut p);
        let reihenfolge: Vec<(u16, u8)> = p.notes().iter().map(|n| (n.step, n.channel)).collect();
        assert_eq!(reihenfolge, vec![(0, 7), (1, 0), (2, 1), (2, 3)]);
        // Setzen hält die Reihenfolge ebenfalls ein.
        setzen(&mut p, note(1, 5, 67));
        let reihenfolge: Vec<(u16, u8)> = p.notes().iter().map(|n| (n.step, n.channel)).collect();
        assert_eq!(reihenfolge, vec![(0, 7), (1, 0), (1, 5), (2, 1), (2, 3)]);
    }

    #[test]
    fn bestehende_grid_tests_bleiben() {
        // Euklid, Laub, Bild-Follow, Maske und CSV/XML legen Noten ohne Kanal
        // an: sie landen auf Kanal 0 und die Tracker-Ansicht zeigt sie dort.
        let mut p = Pattern::with_steps(16);
        assert!(p.toggle(3, 60), "Raster-Toggle wie bisher");
        assert_eq!(p.notes()[0].channel, 0, "Raster = Kanal 0");
        assert_eq!(zelle_text(p.zelle(3, 0)), "C-4 00");
        assert!(p.insert(Note {
            step: 4,
            instrument: 2,
            pitch: 62,
            glide: false,
            channel: 0,
        }));
        assert_eq!(p.notes().len(), 2, "eine Note je Schritt wie im Raster");
        assert!(p.toggle_buendel(8, 9, 64), "Buendel setzt Kanal 0");
        assert!(p
            .notes()
            .iter()
            .filter(|n| n.step >= 8 && n.step <= 9)
            .all(|n| n.channel == 0));
        assert_eq!(nach_kanal(&p, 0).len(), 16, "Raster-Spalte vollstaendig");
    }
}
