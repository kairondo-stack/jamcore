//! Akkordname aus klingenden Noten — Portierung der Intervalltabellen aus den
//! Max-Patches `showchord` (31 Namen) und `mollchords` (Moll-Familie).
//!
//! Die Tabellen stehen als Halbtonschritte ab dem Grundton, genau wie im
//! Patch (`0,4,7`, `0,4,7,14`, …). Verglichen wird über Notenklassen
//! (`mod 12`); damit sind die im Patch einzeln geprüften ±12-Oktavvarianten
//! (`f2==($f1-12+3)` usw.) automatisch abgedeckt: Oktavlage und Reihenfolge
//! der Noten sind egal, Duplikate fallen weg.
//!
//! Grundton-Kandidat ist jede vorkommende Notenklasse (tiefste zuerst) — so
//! werden auch Umkehrungen erkannt (`E-G-C` → C-Dur). Es können mehrere
//! Kandidaten herauskommen (`A-C-E-G` → A m7 und C 6); [`name`] nimmt den
//! ersten (tiefsten Grundton), [`kandidaten`] liefert alle.
//!
//! Patch-Funde (übernommen wie in der Vorlage):
//! - `showchord` prüft `mmaj7/11` und `mmaj7/9/11` mit `+10` (kleine 7 statt
//!   großer), vermutlich Kopierfehler — hier unverändert wie im Patch.
//! - `maj7/#11(b5)`, `7/9/#11(b5)` und `7/9/b13(#5)` schreiben die Vorzeichen
//!   als b5/#5; über die Notenklassen ist das dasselbe wie #11/b13.
//! - `add11` fehlt im Patch (dort nur `madd11`) und ist als 32. Tabelle ergänzt.

/// Alle Akkordtabellen in Prüfreihenfolge: Name und Intervallmuster ab dem
/// Grundton (Halbtonschritte, 0 = Grundton; 9/11/13 als 14/17/21 notiert).
pub const TABELLEN: &[(&str, &[i32])] = &[
    ("Dur", &[0, 4, 7]),
    ("Moll", &[0, 3, 7]),
    ("sus2", &[0, 2, 7]),
    ("sus4", &[0, 5, 7]),
    ("add9", &[0, 4, 7, 14]),
    // Im Patch fehlt Dur-add11 (dort nur `madd11`); aus der Auftragsliste
    // („add9/11") ergänzt.
    ("add11", &[0, 4, 7, 17]),
    ("6", &[0, 4, 7, 9]),
    ("6/9", &[0, 4, 7, 9, 14]),
    ("m6", &[0, 3, 7, 9]),
    ("m6/9", &[0, 3, 7, 9, 14]),
    ("m7", &[0, 3, 7, 10]),
    ("m7/9", &[0, 3, 7, 10, 14]),
    ("m7/b5", &[0, 3, 6, 10]),
    ("madd9", &[0, 3, 7, 14]),
    ("madd11", &[0, 3, 7, 17]),
    ("maj7", &[0, 4, 7, 11]),
    ("maj7/#5", &[0, 4, 8, 11]),
    ("maj7/#11(b5)", &[0, 4, 6, 11]),
    ("maj7/13", &[0, 4, 7, 11, 21]),
    ("maj7/9", &[0, 4, 7, 11, 14]),
    ("maj7/9/13", &[0, 4, 7, 11, 14, 21]),
    ("mmaj7", &[0, 3, 7, 11]),
    // Patch-Vorlage prüft `f4==f1+10` (kleine 7) statt +11 — übernommen.
    ("mmaj7/11", &[0, 3, 7, 10, 17]),
    ("mmaj7/9", &[0, 3, 7, 11, 14]),
    // Patch-Vorlage: ebenfalls +10 (kleine 7).
    ("mmaj7/9/11", &[0, 3, 7, 10, 14, 17]),
    ("7", &[0, 4, 7, 10]),
    ("7/9", &[0, 4, 7, 10, 14]),
    ("7/9/13", &[0, 4, 7, 10, 14, 21]),
    ("7/9/#11(b5)", &[0, 4, 6, 10, 14]),
    ("7/9/b13(#5)", &[0, 4, 8, 10, 14]),
    ("7/sus4", &[0, 5, 7, 10]),
    ("7b9", &[0, 4, 7, 10, 13]),
];

/// Notenklassen (0..12) eines Intervallmusters, sortiert und ohne Duplikate.
fn klassen(intervalle: &[i32]) -> Vec<i32> {
    let mut v: Vec<i32> = intervalle.iter().map(|i| i.rem_euclid(12)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Name des Akkords aus den Noten (MIDI); `None`, wenn keine Tabelle greift.
pub fn name(noten: &[i32]) -> Option<String> {
    kandidaten(noten).into_iter().next().map(|(_, n)| n)
}

/// Kandidaten mit Grundton (für Anzeige „C 7/9"): `(grundton, name)`,
/// aufsteigend nach Grundton.
///
/// Als Grundton wird jede vorkommende Notenklasse probiert (je Klasse der
/// tiefste Ton); die Noten werden dabei auf den Grundton normiert und
/// Duplikate entfernt, Oktaven spielen keine Rolle. Leere oder unbrauchbare
/// Listen ergeben eine leere Kandidatenliste (kein Panic).
pub fn kandidaten(noten: &[i32]) -> Vec<(i32, String)> {
    // Ein Grundton-Kandidat je Notenklasse, der tiefste Ton der Klasse.
    let mut grundtoene: Vec<i32> = Vec::new();
    for &n in noten {
        let klasse = n.rem_euclid(12);
        match grundtoene.iter_mut().find(|g| g.rem_euclid(12) == klasse) {
            Some(g) => {
                if n < *g {
                    *g = n;
                }
            }
            None => grundtoene.push(n),
        }
    }
    grundtoene.sort_unstable();

    // Notenklassen der klingenden Noten (Duplikate weg).
    let mut noten_klassen: Vec<i32> = noten.iter().map(|n| n.rem_euclid(12)).collect();
    noten_klassen.sort_unstable();
    noten_klassen.dedup();

    let mut raus = Vec::new();
    for &grundton in &grundtoene {
        let gk = grundton.rem_euclid(12);
        let mut relativ: Vec<i32> = noten_klassen
            .iter()
            .map(|k| (k - gk).rem_euclid(12))
            .collect();
        relativ.sort_unstable();
        relativ.dedup();
        if let Some((akkord, _)) = TABELLEN.iter().find(|(_, m)| klassen(m) == relativ) {
            raus.push((grundton, (*akkord).to_string()));
        }
    }
    raus
}

/// Intervallmuster (0 = Grundton) eines Namens, für Tests/Anzeige.
pub fn muster(name: &str) -> Option<Vec<i32>> {
    TABELLEN
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, m)| m.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_dur() {
        assert_eq!(name(&[60, 64, 67]), Some("Dur".into()));
        assert_eq!(kandidaten(&[60, 64, 67]), vec![(60, "Dur".to_string())]);
    }

    #[test]
    fn c_moll() {
        assert_eq!(name(&[60, 63, 67]), Some("Moll".into()));
    }

    #[test]
    fn c_maj7() {
        assert_eq!(name(&[60, 64, 67, 71]), Some("maj7".into()));
    }

    #[test]
    fn c_m7() {
        assert_eq!(name(&[60, 63, 67, 70]), Some("m7".into()));
    }

    #[test]
    fn sus4() {
        assert_eq!(name(&[60, 65, 67]), Some("sus4".into()));
        // sus2 steht direkt daneben in der Tabelle.
        assert_eq!(name(&[60, 62, 67]), Some("sus2".into()));
    }

    #[test]
    fn add9() {
        assert_eq!(name(&[60, 64, 67, 74]), Some("add9".into()));
        // Dieselbe None als 62 (D4, eine Oktave unter der 9) — die
        // ±12-Variante des Patches.
        assert_eq!(name(&[60, 64, 67, 62]), Some("add9".into()));
    }

    #[test]
    fn siebenb9() {
        assert_eq!(name(&[60, 64, 67, 70, 73]), Some("7b9".into()));
    }

    #[test]
    fn umkehrung_erkannt() {
        // E-G-C: tiefster Ton ist E, der Akkord ist C-Dur (Grundton C).
        assert_eq!(name(&[52, 55, 60]), Some("Dur".into()));
        assert_eq!(kandidaten(&[52, 55, 60]), vec![(60, "Dur".to_string())]);
    }

    #[test]
    fn kein_treffer_bei_zwei_noten() {
        assert_eq!(name(&[]), None);
        assert_eq!(name(&[60]), None);
        assert_eq!(name(&[60, 64]), None);
        assert_eq!(kandidaten(&[60, 64]), Vec::new());
    }

    #[test]
    fn duplikate_egal() {
        // C und E doppelt (auch oktaviert), G dazu: derselbe Akkord.
        assert_eq!(name(&[60, 60, 64, 67, 72]), Some("Dur".into()));
        assert_eq!(name(&[48, 60, 64, 67]), name(&[60, 64, 67]));
    }

    #[test]
    fn mehrere_kandidaten_nach_grundton_sortiert() {
        // A-C-E-G: A m7 oder C 6 — A ist der tiefere Grundton, `name` nimmt ihn.
        assert_eq!(
            kandidaten(&[57, 60, 64, 67]),
            vec![(57, "m7".to_string()), (60, "6".to_string())]
        );
        assert_eq!(name(&[57, 60, 64, 67]), Some("m7".into()));
    }

    #[test]
    fn oktavlagen_und_reihenfolge_egal() {
        // G-C-E-G (Umkehrung) und dieselben Noten rückwärts.
        assert_eq!(name(&[55, 60, 64, 79]), Some("Dur".into()));
        assert_eq!(name(&[79, 64, 60, 55]), Some("Dur".into()));
        // Negative MIDI-Werte werden normalisiert (H, D#, F# = B-Dur).
        assert_eq!(name(&[-9, -6, -1]), Some("Dur".into()));
    }

    #[test]
    fn kaputte_werte_kein_panic() {
        assert_eq!(name(&[i32::MIN, i32::MAX]), None);
        assert_eq!(name(&[i32::MIN]), None);
        // Extremwerte rechnen trotzdem sauber (C-Dur um i32::MIN herum).
        assert_eq!(
            name(&[i32::MIN, i32::MIN + 4, i32::MIN + 7]),
            Some("Dur".into())
        );
    }

    #[test]
    fn alle_31_patch_namen_abgedeckt() {
        let namen: Vec<&str> = TABELLEN.iter().map(|(n, _)| *n).collect();
        // 31 Namen aus showchord + add11 (im Patch nur madd11).
        assert_eq!(namen.len(), 32, "{namen:?}");
        for pflicht in [
            "Dur",
            "Moll",
            "sus2",
            "sus4",
            "add9",
            "6",
            "6/9",
            "m6",
            "m6/9",
            "m7",
            "m7/9",
            "m7/b5",
            "madd9",
            "madd11",
            "maj7",
            "maj7/#5",
            "maj7/#11(b5)",
            "maj7/13",
            "maj7/9",
            "maj7/9/13",
            "mmaj7",
            "mmaj7/11",
            "mmaj7/9",
            "mmaj7/9/11",
            "7",
            "7/9",
            "7/9/13",
            "7/9/#11(b5)",
            "7/9/b13(#5)",
            "7/sus4",
            "7b9",
        ] {
            assert!(muster(pflicht).is_some(), "{pflicht} fehlt");
        }
    }

    #[test]
    fn jede_tabelle_wird_erkannt() {
        for (akkord, m) in TABELLEN {
            let noten: Vec<i32> = m.iter().map(|i| 60 + i).collect();
            assert_eq!(
                name(&noten),
                Some((*akkord).to_string()),
                "Tabelle {akkord}"
            );
        }
    }

    #[test]
    fn tabellen_sind_verschieden() {
        for (i, (na, ma)) in TABELLEN.iter().enumerate() {
            for (nb, mb) in &TABELLEN[i + 1..] {
                assert_ne!(klassen(ma), klassen(mb), "{na} und {nb} sind gleich");
            }
        }
    }

    #[test]
    fn muster_der_tabellen() {
        assert_eq!(muster("Dur"), Some(vec![0, 4, 7]));
        assert_eq!(muster("add9"), Some(vec![0, 4, 7, 14]));
        assert_eq!(muster("7b9"), Some(vec![0, 4, 7, 10, 13]));
        assert_eq!(muster("gibtsnicht"), None);
    }
}
