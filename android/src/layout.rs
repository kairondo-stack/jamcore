//! Kombi-Layout (S11b): zwei Module gleichzeitig.
//!
//! Hochformat: die Bereiche liegen untereinander. Querformat (breit und
//! flacher als breit): nebeneinander als SidePanel + CentralPanel. Welches
//! Modul wo läuft, entscheidet die Auswahlzeile; das Zeichnen selbst bleibt
//! beim Aufrufer (er kennt die Panels).

use eframe::egui;

/// Wählbares Modul für einen Kombi-Bereich.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KombiTeil {
    Leer,
    Bild,
    Rhythmus,
    Maske,
    Tempo,
    Session,
    Tagebuch,
    Kreis,
}

impl KombiTeil {
    pub const ALLE: [KombiTeil; 8] = [
        KombiTeil::Leer,
        KombiTeil::Bild,
        KombiTeil::Rhythmus,
        KombiTeil::Maske,
        KombiTeil::Tempo,
        KombiTeil::Session,
        KombiTeil::Tagebuch,
        KombiTeil::Kreis,
    ];

    pub fn name(self) -> &'static str {
        match self {
            KombiTeil::Leer => "leer",
            KombiTeil::Bild => "Kamera/Scan",
            KombiTeil::Rhythmus => "Rhythmus",
            KombiTeil::Maske => "Takt-Maske",
            KombiTeil::Tempo => "Tempo",
            KombiTeil::Session => "Session",
            KombiTeil::Tagebuch => "Tagebuch",
            KombiTeil::Kreis => "Kreis",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anordnung {
    Untereinander,
    Nebeneinander,
}

/// Quer genug für zwei Spalten: breit und flacher als breit.
pub fn anordnung(verfuegbare_breite: f32, verfuegbare_hoehe: f32) -> Anordnung {
    if verfuegbare_breite >= 560.0 && verfuegbare_breite > verfuegbare_hoehe {
        Anordnung::Nebeneinander
    } else {
        Anordnung::Untereinander
    }
}

/// Auswahlzeile "Oben/Unten" (im Querformat "Links/Rechts").
pub fn wahlzeile(ui: &mut egui::Ui, oben: &mut KombiTeil, unten: &mut KombiTeil) {
    let (links, rechts) = match anordnung(ui.available_width(), ui.available_height()) {
        Anordnung::Untereinander => ("Oben:", "Unten:"),
        Anordnung::Nebeneinander => ("Links:", "Rechts:"),
    };
    ui.horizontal(|ui| {
        ui.label(links);
        egui::ComboBox::from_id_salt("kombi_oben")
            .selected_text(oben.name())
            .show_ui(ui, |ui| {
                for teil in KombiTeil::ALLE {
                    if ui.selectable_label(*oben == teil, teil.name()).clicked() {
                        *oben = teil;
                    }
                }
            });
        ui.label(rechts);
        egui::ComboBox::from_id_salt("kombi_unten")
            .selected_text(unten.name())
            .show_ui(ui, |ui| {
                for teil in KombiTeil::ALLE {
                    if ui.selectable_label(*unten == teil, teil.name()).clicked() {
                        *unten = teil;
                    }
                }
            });
    });
    ui.separator();
}

/// Zeichnet beide Bereiche; `zeichnen` malt ein Modul in die übergebene Ui.
pub fn bereiche(
    ui: &mut egui::Ui,
    oben: KombiTeil,
    unten: KombiTeil,
    mut zeichnen: impl FnMut(&mut egui::Ui, KombiTeil),
) {
    match anordnung(ui.available_width(), ui.available_height()) {
        Anordnung::Untereinander => {
            let hoehe = (ui.available_height() - 12.0) / 2.0;
            let breite = ui.available_width();
            ui.allocate_ui(egui::vec2(breite, hoehe), |ui| {
                egui::ScrollArea::vertical()
                    .scroll_source(egui::scroll_area::ScrollSource::ALL)
                    .id_salt("kombi_bereich_oben")
                    .show(ui, |ui| zeichnen(ui, oben));
            });
            ui.separator();
            ui.allocate_ui(egui::vec2(breite, hoehe), |ui| {
                egui::ScrollArea::vertical()
                    .scroll_source(egui::scroll_area::ScrollSource::ALL)
                    .id_salt("kombi_bereich_unten")
                    .show(ui, |ui| zeichnen(ui, unten));
            });
        }
        Anordnung::Nebeneinander => {
            egui::Panel::left("kombi_links")
                .resizable(true)
                .default_size((ui.available_width() * 0.5).max(200.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .scroll_source(egui::scroll_area::ScrollSource::ALL)
                        .id_salt("kombi_bereich_links")
                        .show(ui, |ui| zeichnen(ui, oben));
                });
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .scroll_source(egui::scroll_area::ScrollSource::ALL)
                    .id_salt("kombi_bereich_rechts")
                    .show(ui, |ui| zeichnen(ui, unten));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hochformat_stapelt() {
        assert_eq!(anordnung(390.0, 800.0), Anordnung::Untereinander);
        assert_eq!(anordnung(872.0, 1900.0), Anordnung::Untereinander);
    }

    #[test]
    fn querformat_legt_nebeneinander() {
        assert_eq!(anordnung(1900.0, 872.0), Anordnung::Nebeneinander);
        assert_eq!(anordnung(760.0, 360.0), Anordnung::Nebeneinander);
    }

    #[test]
    fn schmale_flaeche_bleibt_gestapelt() {
        assert_eq!(
            anordnung(540.0, 300.0),
            Anordnung::Untereinander,
            "zu schmal"
        );
        assert_eq!(
            anordnung(700.0, 700.0),
            Anordnung::Untereinander,
            "quadratisch"
        );
    }

    #[test]
    fn alle_teile_haben_namen() {
        for teil in KombiTeil::ALLE {
            assert!(!teil.name().is_empty());
        }
    }
}
