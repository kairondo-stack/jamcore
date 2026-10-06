//! Hardwarefreie Logik für Desktop und Android:
//! Mikrofon-BPM, Live-Scan-Filter, Quintenzirkel-Geometrie, Pattern-Raster,
//! OSC-Ausgang.

pub mod grid;
pub mod livescan;
pub mod micbpm;
pub mod openmpt;
pub mod osc;
pub mod quint;

/// Ableton-Link-SDK (GPLv2, siehe `Cargo.toml`, Feature `link`).
/// Android braucht die Ergänzungen aus `android/link-android-toolchain.cmake`.
#[cfg(feature = "link")]
pub use rusty_link;
