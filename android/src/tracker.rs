//! Tracker-Ansicht des Rhythmus-Tabs: Zeilen = Schritte, Spalten = Kanäle
//! (0…63), Zelle = Note + Instrument (`C-5 01`, leer `--- --`).
//!
//! Wie im Impulse Tracker / in ModGuru: **eine** Zelle ist gewählt, die Pads
//! und Tasten des Rhythmus-Tabs schreiben ihre Note dorthin (die Verdrahtung
//! steht in `rhythm.rs`). Gezeichnet wird das ganze Raster mit **einem**
//! Painter — 64 × 512 einzelne egui-Widgets wären auf dem Handy zu teuer; nur
//! die sichtbaren Zeilen und Spalten bekommen Text und Linien. Ein Tipp wird
//! in (Schritt, Kanal) zurückgerechnet und an den Aufrufer gemeldet.
//!
//! Scrollen: ein `ScrollArea::both` mit eigener Id, damit die Stelle beim
//! Umschalten Raster ⇄ Tracker und beim Neuzeichnen erhalten bleibt („wo war
//! ich“) — 64 Spalten und bis zu 512 Zeilen sind sonst nicht erreichbar.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Stroke, StrokeKind};
use jamcore::grid::Pattern;
use jamcore::tracker::{nach_kanal, zelle_text, KANAELE};

/// Breite einer Kanal-Spalte in Punkten (6 Zeichen Monospace + Rand; ein
/// dreistelliges Instrument füllt sie gerade noch).
const SPALTE_BREITE: f32 = 46.0;
/// Höhe einer Schritt-Zeile in Punkten (auf dem Handy ein bequemes Ziel).
const ZEILE_HOEHE: f32 = 16.0;
/// Höhe der Kopfzeile (`Step | Ch1 …`).
const KOPF_HOEHE: f32 = 18.0;
/// Breite der Schritt-Spalte links.
const STEP_BREITE: f32 = 40.0;
/// Schriftgröße der Zellen und der Kopfzeile.
const SCHRIFT: f32 = 11.0;
/// Sichtbare Höhe des Rasters (Punkte): auf dem Handy etwa ein halber
/// Bildschirm, damit die Regler des Tabs erreichbar bleiben.
const SICHT_MIN: f32 = 200.0;
const SICHT_MAX: f32 = 460.0;

const HINTERGRUND: Color32 = Color32::from_rgb(22, 24, 30);
const KOPF_FARBE: Color32 = Color32::from_rgb(40, 44, 56);
const KOPF_STEP: Color32 = Color32::from_rgb(32, 36, 46);
const KOPF_GEWAEHLT: Color32 = Color32::from_rgb(58, 68, 92);
const KANAL_FARBE: Color32 = Color32::from_rgb(32, 36, 48);
const ZEILE_FARBE: Color32 = Color32::from_rgb(44, 50, 68);
const SPIEL_FARBE: Color32 = Color32::from_rgb(48, 78, 60);
const ZELLE_FARBE: Color32 = Color32::from_rgb(52, 76, 112);
const ZELLE_RAND: Color32 = Color32::from_rgb(150, 200, 255);
const LINIE: Color32 = Color32::from_rgb(36, 40, 50);
const LINIE_VIERTEL: Color32 = Color32::from_rgb(58, 64, 80);
const LINIE_TAKT: Color32 = Color32::from_rgb(92, 100, 122);
const TEXT_NOTE: Color32 = Color32::from_rgb(190, 240, 200);
const TEXT_LEER: Color32 = Color32::from_rgb(88, 94, 108);
const TEXT_NUMMER: Color32 = Color32::from_rgb(120, 126, 140);
const TEXT_GRAU: Color32 = Color32::from_rgb(150, 155, 168);
const TEXT_WEISS: Color32 = Color32::from_rgb(245, 245, 255);

/// Welche Ansicht der Rhythmus-Tab zeigt: den Pad-Raster-Step-Editor oder die
/// Tracker-Tabelle mit 64 Kanal-Spalten (Umschalter „Raster ⇄ Tracker“).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ansicht {
    /// Pad-Raster (Step-Editor) — wie bisher, Euklid/Laub/Maske/CSV/XML.
    Raster,
    /// Tracker: Zeilen = Schritte, Spalten = Kanäle, Zelle = Note Instrument.
    Tracker,
}

/// Was die Ansicht vom Rhythmus-Tab braucht: die gewählte Zelle und der
/// laufende Schritt der Wiedergabe (alle 0-basiert, wie im Pattern).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zustand {
    /// Gewählter Schritt (Zeile).
    pub schritt: u16,
    /// Gewählter Kanal (Spalte).
    pub kanal: u8,
    /// Laufender Schritt der Wiedergabe — die Zeile wird hervorgehoben.
    pub spiel_schritt: u16,
}

/// Zeichnet das Tracker-Raster.
///
/// Rückgabe: `Some((Schritt, Kanal))`, wenn eine Zelle angetippt wurde.
/// `scroll_zu` ist ein Wunsch des Aufrufers („Schritt ±“), die genannte Zeile
/// in den Blick zu holen; der Wunsch wird hier verbraucht.
pub fn ansicht(
    ui: &mut egui::Ui,
    pattern: &Pattern,
    zustand: &Zustand,
    scroll_zu: &mut Option<u16>,
) -> Option<(u16, u8)> {
    let schritte = pattern.steps_per_cycle().max(1);
    let breite = STEP_BREITE + KANAELE as f32 * SPALTE_BREITE;
    let hoehe = KOPF_HOEHE + schritte as f32 * ZEILE_HOEHE;
    let sicht = (ui.ctx().content_rect().height() * 0.55).clamp(SICHT_MIN, SICHT_MAX);
    let mut getippt = None;
    egui::ScrollArea::both()
        .id_salt("tracker_raster")
        .scroll_source(egui::scroll_area::ScrollSource::ALL)
        .max_height(sicht)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (response, painter) =
                ui.allocate_painter(egui::vec2(breite, hoehe), egui::Sense::click());
            let r = response.rect;
            zeichnen(&painter, r, ui.clip_rect(), pattern, zustand);
            // Die gewünschte Zeile in den Blick holen. Das Rechteck geht über
            // die ganze Breite, deshalb bleibt die waagerechte Scrollposition,
            // und ohne Ausrichtung scrollt egui nur so weit wie nötig.
            if let Some(zeile) = scroll_zu.take() {
                let y0 = r.top() + KOPF_HOEHE + zeile.min(schritte - 1) as f32 * ZEILE_HOEHE;
                ui.scroll_to_rect(
                    Rect::from_min_max(
                        egui::pos2(r.left(), y0),
                        egui::pos2(r.right(), y0 + ZEILE_HOEHE),
                    ),
                    None,
                );
            }
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    getippt = treffer(r, pos, schritte);
                }
            }
        });
    getippt
}

/// Antipp-Ort in (Schritt, Kanal) umrechnen. `None` ausserhalb der Zellen
/// (Kopfzeile, Schritt-Spalte, hinter dem Patternende).
fn treffer(r: Rect, pos: egui::Pos2, schritte: u16) -> Option<(u16, u8)> {
    let x = pos.x - r.left() - STEP_BREITE;
    let y = pos.y - r.top() - KOPF_HOEHE;
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let spalte = (x / SPALTE_BREITE).floor();
    let zeile = (y / ZEILE_HOEHE).floor();
    if spalte < 0.0 || zeile < 0.0 || spalte >= KANAELE as f32 || zeile >= schritte as f32 {
        return None;
    }
    Some((zeile as u16, spalte as u8))
}

/// Erstes sichtbares Feld (Zeile oder Spalte) im Ausschnitt `clip`.
fn feld_von(anfang: f32, basis: f32, hoehe: f32, anzahl: u32) -> u32 {
    (((anfang - basis) / hoehe).floor().max(0.0) as u32).min(anzahl)
}

/// Ende des sichtbaren Bereichs (exklusiv).
fn feld_bis(ende: f32, basis: f32, hoehe: f32, anzahl: u32) -> u32 {
    (((ende - basis) / hoehe).ceil().max(0.0) as u32).min(anzahl)
}

/// Rechteck einer ganzen Zeile (über die volle Rasterbreite).
fn zeilen_rect(r: Rect, zeile: u16) -> Rect {
    let y0 = r.top() + KOPF_HOEHE + zeile as f32 * ZEILE_HOEHE;
    Rect::from_min_max(
        egui::pos2(r.left(), y0),
        egui::pos2(r.right(), y0 + ZEILE_HOEHE),
    )
}

/// Das Raster selbst: Hintergrund, Hervorhebungen, Linien, Kopfzeile, Zellen.
/// Nur der sichtbare Ausschnitt bekommt Text und Linien — sonst entstünden
/// 64 × 512 Galleys je Frame.
fn zeichnen(painter: &egui::Painter, r: Rect, clip: Rect, pattern: &Pattern, zustand: &Zustand) {
    let schritte = pattern.steps_per_cycle().max(1);
    let schrift = FontId::monospace(SCHRIFT);
    let zeile_von = feld_von(clip.top(), r.top() + KOPF_HOEHE, ZEILE_HOEHE, schritte as u32);
    let zeile_bis = feld_bis(clip.bottom(), r.top() + KOPF_HOEHE, ZEILE_HOEHE, schritte as u32);
    let spalte_von = feld_von(clip.left(), r.left() + STEP_BREITE, SPALTE_BREITE, KANAELE as u32);
    let spalte_bis = feld_bis(clip.right(), r.left() + STEP_BREITE, SPALTE_BREITE, KANAELE as u32);

    // Grundfläche und Kopfzeile.
    painter.rect_filled(r, 0.0, HINTERGRUND);
    let kopf = Rect::from_min_max(r.min, egui::pos2(r.right(), r.top() + KOPF_HOEHE));
    painter.rect_filled(kopf, 0.0, KOPF_FARBE);
    painter.rect_filled(
        Rect::from_min_max(kopf.min, egui::pos2(r.left() + STEP_BREITE, kopf.bottom())),
        0.0,
        KOPF_STEP,
    );

    // Gewählte Spalte (ganze Höhe) und ihr Kopf.
    let x_gewaehlt = r.left() + STEP_BREITE + zustand.kanal as f32 * SPALTE_BREITE;
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(x_gewaehlt, kopf.bottom()),
            egui::pos2(x_gewaehlt + SPALTE_BREITE, r.bottom()),
        ),
        0.0,
        KANAL_FARBE,
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(x_gewaehlt, kopf.top()),
            egui::pos2(x_gewaehlt + SPALTE_BREITE, kopf.bottom()),
        ),
        0.0,
        KOPF_GEWAEHLT,
    );

    // Gewählte Zeile, darüber die laufende Zeile der Wiedergabe (sie gewinnt).
    if zustand.schritt < schritte {
        painter.rect_filled(zeilen_rect(r, zustand.schritt), 0.0, ZEILE_FARBE);
    }
    if zustand.spiel_schritt < schritte {
        painter.rect_filled(zeilen_rect(r, zustand.spiel_schritt), 0.0, SPIEL_FARBE);
    }

    // Linien wie im Tracker: jeder Schritt dünn, jeder vierte (Viertelnote)
    // heller, jeder sechzehnte (Takt) am hellsten.
    for zeile in zeile_von..zeile_bis {
        let y = r.top() + KOPF_HOEHE + zeile as f32 * ZEILE_HOEHE;
        let (farbe, breite) = if zeile % 16 == 0 {
            (LINIE_TAKT, 1.0)
        } else if zeile % 4 == 0 {
            (LINIE_VIERTEL, 1.0)
        } else {
            (LINIE, 0.5)
        };
        painter.line_segment(
            [egui::pos2(r.left(), y), egui::pos2(r.right(), y)],
            Stroke::new(breite, farbe),
        );
    }
    for spalte in spalte_von..spalte_bis {
        let x = r.left() + STEP_BREITE + spalte as f32 * SPALTE_BREITE;
        let farbe = if spalte % 4 == 0 { LINIE_VIERTEL } else { LINIE };
        painter.line_segment(
            [egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
            Stroke::new(0.5, farbe),
        );
    }

    // Kopfzeile: `Step | Ch1 | … | Ch64`.
    painter.text(
        egui::pos2(r.left() + 4.0, kopf.center().y),
        Align2::LEFT_CENTER,
        "Step",
        schrift.clone(),
        TEXT_GRAU,
    );
    for spalte in spalte_von..spalte_bis {
        let x = r.left() + STEP_BREITE + (spalte as f32 + 0.5) * SPALTE_BREITE;
        painter.text(
            egui::pos2(x, kopf.center().y),
            Align2::CENTER_CENTER,
            format!("Ch{}", spalte + 1),
            schrift.clone(),
            if spalte as u8 == zustand.kanal {
                TEXT_WEISS
            } else {
                TEXT_GRAU
            },
        );
    }

    // Schrittnummern links (1-basiert wie im übrigen Tab).
    for zeile in zeile_von..zeile_bis {
        let y0 = r.top() + KOPF_HOEHE + zeile as f32 * ZEILE_HOEHE;
        painter.text(
            egui::pos2(r.left() + STEP_BREITE - 5.0, y0 + ZEILE_HOEHE * 0.5),
            Align2::RIGHT_CENTER,
            format!("{}", zeile + 1),
            schrift.clone(),
            if zeile as u16 == zustand.schritt {
                TEXT_WEISS
            } else {
                TEXT_NUMMER
            },
        );
    }

    // Zellen: je Kanal eine Spalte (`nach_kanal`), dann nur die sichtbaren
    // Zeilen. Leere Zellen zeigen `--- --` wie im Tracker.
    for spalte in spalte_von..spalte_bis {
        let daten = nach_kanal(pattern, spalte as u8);
        let x0 = r.left() + STEP_BREITE + spalte as f32 * SPALTE_BREITE;
        for zeile in zeile_von..zeile_bis {
            let y0 = r.top() + KOPF_HOEHE + zeile as f32 * ZEILE_HOEHE;
            let note = daten.get(zeile as usize).and_then(|eintrag| eintrag.as_ref());
            let gewaehlt = zeile as u16 == zustand.schritt && spalte as u8 == zustand.kanal;
            if gewaehlt {
                let zelle = Rect::from_min_max(
                    egui::pos2(x0 + 1.0, y0 + 1.0),
                    egui::pos2(x0 + SPALTE_BREITE - 1.0, y0 + ZEILE_HOEHE - 1.0),
                );
                painter.rect_filled(zelle, 2.0, ZELLE_FARBE);
                painter.rect_stroke(
                    zelle,
                    egui::CornerRadius::same(2),
                    Stroke::new(1.0, ZELLE_RAND),
                    StrokeKind::Inside,
                );
            }
            let farbe = if gewaehlt {
                TEXT_WEISS
            } else if note.is_some() {
                TEXT_NOTE
            } else {
                TEXT_LEER
            };
            painter.text(
                egui::pos2(x0 + 4.0, y0 + ZEILE_HOEHE * 0.5),
                Align2::LEFT_CENTER,
                zelle_text(note),
                schrift.clone(),
                farbe,
            );
        }
    }
}
