//! Akkordzeile für die App: Name aus den klingenden Noten (`jamcore::akkord`).
//!
//! Eine kleine Zeile, die später im Instrument-Tab (`crate::instr`) oder im
//! Player (`crate::player`) unter den klingenden Noten stehen kann:
//! `crate::akkord::zeile(ui, &noten)`. Sie zeichnet
//! „Akkord: C Dur (Kandidaten: C Dur, A m7)" und gibt den Anzeigetext zurück.

use eframe::egui;

use jamcore::akkord::kandidaten;

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
