//! Hardwarefreie Logik für Desktop und Android:
//! Mikrofon-BPM, Live-Scan-Filter, Quintenzirkel-Geometrie, Pattern-Raster,
//! OSC-Ausgang.

pub mod bmp;
pub mod diary;
pub mod gesten;
pub mod grid;
pub mod harmonie;
pub mod instrument;
pub mod klang;
pub mod livescan;
pub mod maske;
pub mod micbpm;
pub mod openmpt;
pub mod osc;
pub mod quint;
pub mod scanmask;
pub mod schrittweite;
pub mod session;
pub mod takte;
pub mod transformation;
pub mod wav;
pub mod yuv;

/// Ableton-Link-SDK (GPLv2, siehe `Cargo.toml`, Feature `link`).
/// Android braucht die Ergänzungen aus `android/link-android-toolchain.cmake`.
#[cfg(feature = "link")]
pub use rusty_link;
