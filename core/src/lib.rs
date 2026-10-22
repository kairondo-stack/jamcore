//! Hardwarefreie Logik für Desktop und Android:
//! Mikrofon-BPM, Live-Scan-Filter, Quintenzirkel-Geometrie, Pattern-Raster,
//! OSC-Ausgang.

pub mod bmp;
pub mod akkord;
pub mod audio;
pub mod audioedit;
pub mod diary;
pub mod euclid;
pub mod gesten;
pub mod grid;
pub mod harmonie;
pub mod instrument;
pub mod kanal;
pub mod laub;
pub mod itwrite;
pub mod klang;
pub mod livescan;
pub mod lissa;
pub mod maske;
pub mod micbpm;
pub mod modulebrowser;
pub mod openmpt;
pub mod oszillator;
pub mod osc;
pub mod quint;
pub mod relay;
pub mod scan;
pub mod scanmask;
pub mod schrittweite;
pub mod session;
pub mod spirale;
pub mod slice;
pub mod stroboskop;
pub mod takte;
pub mod tracker;
pub mod transformation;
pub mod videotempo;
pub mod wav;
pub mod yuv;

/// Ableton-Link-SDK (GPLv2, siehe `Cargo.toml`, Feature `link`).
/// Android braucht die Ergänzungen aus `android/link-android-toolchain.cmake`.
#[cfg(feature = "link")]
pub use rusty_link;
