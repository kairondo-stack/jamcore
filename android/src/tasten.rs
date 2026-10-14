//! USB-Nummernblock: Tastenereignisse fuer die Masken-Felder.
//!
//! Auf Android liefert winit die Nummernblock-Tasten als `Key::Num0..Num9` und
//! die Eingabetaste als `Key::Enter` (egui-winit 0.36.2 mappt
//! `Digit0|Numpad0 → Num0` und `Enter|NumpadEnter → Enter`). Der Spike am
//! Geraet (2026-10-14) hat das bestaetigt: `KEYCODE_NUMPAD_9` kam als `Num9`,
//! `KEYCODE_NUMPAD_ENTER` als `Enter` an.

use eframe::egui;

/// Was der Nummernblock in diesem Frame ausgeloest hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taste {
    /// Eine Ziffer 0..=9 — die Zuordnung zur Feldposition macht der Aufrufer
    /// (`maske::position_linear` bzw. `maske::position_numpad`).
    Ziffer(u8),
    /// Eingabe/Return: in Maske 2 das „Ende".
    Enter,
}

/// Alle neuen Tastendruecke dieses Frames (ohne Auto-Wiederholung).
pub fn tasten(ui: &egui::Ui) -> Vec<Taste> {
    let mut raus = Vec::new();
    ui.ctx().input(|eingabe| {
        for ereignis in &eingabe.events {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } = ereignis
            else {
                continue;
            };
            let taste = match key {
                egui::Key::Num0 => Some(Taste::Ziffer(0)),
                egui::Key::Num1 => Some(Taste::Ziffer(1)),
                egui::Key::Num2 => Some(Taste::Ziffer(2)),
                egui::Key::Num3 => Some(Taste::Ziffer(3)),
                egui::Key::Num4 => Some(Taste::Ziffer(4)),
                egui::Key::Num5 => Some(Taste::Ziffer(5)),
                egui::Key::Num6 => Some(Taste::Ziffer(6)),
                egui::Key::Num7 => Some(Taste::Ziffer(7)),
                egui::Key::Num8 => Some(Taste::Ziffer(8)),
                egui::Key::Num9 => Some(Taste::Ziffer(9)),
                egui::Key::Enter => Some(Taste::Enter),
                _ => None,
            };
            if let Some(taste) = taste {
                raus.push(taste);
            }
        }
    });
    raus
}
