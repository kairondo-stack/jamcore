//! Akkordzeile für die App: Name aus den klingenden Noten (`jamcore::akkord`).
//!
//! Eine kleine Zeile, die später im Instrument-Tab (`crate::instr`) oder im
//! Player (`crate::player`) unter den klingenden Noten stehen kann:
//! `crate::akkord::zeile(ui, &noten)`. Sie zeichnet
//! „Akkord: C Dur (Kandidaten: C Dur, A m7)" und gibt den Anzeigetext zurück.

use eframe::egui;

use jamcore::akkord::kandidaten;

/// Kleine Akkord-Presets fuer die Echtzeit-Noteneingabe im Rhythmus-Tab
/// („Akkord als Mini-Presets“). Der Name steht auf dem Knopf, die Intervalle
/// sind Halbtonschritte ab dem Grundton — dieselbe Schreibweise wie
/// `jamcore::akkord::TABELLEN` (9/11/13 als 14/17/21).
///
/// **Erweitern:** ein Paar `("Name", &[0, …])` anhaengen; `rhythm.rs` zeichnet
/// alle Eintraege als Knoepfe und setzt sie auf den gewaehlten Tracker-Schritt.
/// Die Presets liegen fest im Code (kein Dateiformat) — so sind sie ohne
/// Speicherort und Berechtigungen sofort da. Gespeichert wird nichts weiter:
/// der Nutzer kann sie hören, bevor er sie ins Pattern schreibt.
pub const PRESETS: &[(&str, &[i32])] = &[
    ("Dur", &[0, 4, 7]),
    ("Moll", &[0, 3, 7]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("sus4", &[0, 5, 7]),
    ("sus2", &[0, 2, 7]),
    ("Power", &[0, 7]),
];

/// Die Toene eines Presets ab `grundton` (MIDI, auf 0…127 geklemmt).
/// `preset` ausserhalb der Liste ergibt eine leere Liste (kein Panic).
pub fn noten(preset: usize, grundton: i32) -> Vec<i32> {
    let intervalle = PRESETS.get(preset).map(|(_, i)| *i).unwrap_or(&[]);
    intervalle
        .iter()
        .map(|i| (grundton + i).clamp(0, 127))
        .collect()
}

/// Notenklasse als Name (C, C#, …; Oktave egal, wie im Renoise-Raster).
fn klassenname(note: i32) -> String {
    const NAMEN: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    NAMEN[note.rem_euclid(12) as usize].to_string()
}

/// Zeichnet die Akkordzeile und gibt den Anzeigetext zurück
/// (Grundton + Name, z. B. „C Dur"; ohne Treffer „–").
///
/// Kandidaten stehen in Klammern, wenn es mehr als einen gibt (Umkehrungen,
/// z. B. „A m7 (Kandidaten: A m7, C 6)").
pub fn zeile(ui: &mut egui::Ui, noten: &[i32]) -> String {
    let kandidaten = kandidaten(noten);
    let text = match kandidaten.first() {
        Some((grundton, akkord)) => {
            let mut text = format!("{} {}", klassenname(*grundton), akkord);
            if kandidaten.len() > 1 {
                let alle: Vec<String> = kandidaten
                    .iter()
                    .map(|(g, n)| format!("{} {}", klassenname(*g), n))
                    .collect();
                text.push_str(&format!(" (Kandidaten: {})", alle.join(", ")));
            }
            text
        }
        None => "–".to_string(),
    };
    ui.label(egui::RichText::new(format!("Akkord: {text}")).small());
    text
}
