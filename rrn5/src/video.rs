//! Kamera-/Videodatei-Eingabe via OpenCV, plus reine Helfer.

use opencv::prelude::*;
use opencv::videoio::{VideoCapture, CAP_ANY, CAP_PROP_POS_FRAMES};

/// BGR (3 Bytes/Pixel) nach RGBA (4 Bytes/Pixel).
pub fn bgr_to_rgba(bgr: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgr.len() / 3 * 4);
    for px in bgr.chunks_exact(3) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    out
}

/// Shared-Memory-Veröffentlichungsmodus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShmMode {
    Every,
    Throttle,
    Off,
}

impl ShmMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ShmMode::Every => "every",
            ShmMode::Throttle => "throttle",
            ShmMode::Off => "off",
        }
    }
}

pub fn parse_shm_mode(s: &str) -> Option<ShmMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "every" | "alle" => Some(ShmMode::Every),
        "throttle" | "gedrosselt" => Some(ShmMode::Throttle),
        "off" | "aus" => Some(ShmMode::Off),
        _ => None,
    }
}

pub fn next_shm_mode(m: ShmMode) -> ShmMode {
    match m {
        ShmMode::Every => ShmMode::Throttle,
        ShmMode::Throttle => ShmMode::Off,
        ShmMode::Off => ShmMode::Every,
    }
}

pub struct VideoSource {
    cap: VideoCapture,
    looping: bool,
}

impl VideoSource {
    pub fn open_camera(index: i32) -> Result<Self, String> {
        let cap = VideoCapture::new(index, CAP_ANY)
            .map_err(|e| format!("Kamera {index}: {e}"))?;
        if !cap.is_opened().unwrap_or(false) {
            return Err(format!("Kamera {index} konnte nicht geöffnet werden"));
        }
        let mut src = VideoSource { cap, looping: false };
        let _ = src.cap.set(3, 640.0);
        let _ = src.cap.set(4, 480.0);
        Ok(src)
    }

    pub fn open_file(path: &str) -> Result<Self, String> {
        let cap = VideoCapture::from_file(path, CAP_ANY)
            .map_err(|e| format!("Video {path}: {e}"))?;
        if !cap.is_opened().unwrap_or(false) {
            return Err(format!("Video {path} konnte nicht geöffnet werden"));
        }
        Ok(VideoSource { cap, looping: true })
    }

    pub fn next_frame_rgba(&mut self) -> Option<(i32, i32, Vec<u8>)> {
        if let Some(frame) = self.read_frame() {
            return Some(frame);
        }
        if self.looping {
            let _ = self.cap.set(CAP_PROP_POS_FRAMES, 0.0);
            return self.read_frame();
        }
        None
    }

    fn read_frame(&mut self) -> Option<(i32, i32, Vec<u8>)> {
        let mut mat = Mat::default();
        let ok = self.cap.read(&mut mat).ok()?;
        if !ok || mat.empty() {
            return None;
        }
        let (w, h) = (mat.cols(), mat.rows());
        let bgr = mat.data_bytes().ok()?;
        Some((w, h, bgr_to_rgba(bgr)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgr_to_rgba_swaps_channels_and_sets_alpha() {
        let bgr = vec![30, 20, 10, 0, 128, 255];
        assert_eq!(bgr_to_rgba(&bgr), vec![10, 20, 30, 255, 255, 128, 0, 255]);
        assert_eq!(bgr_to_rgba(&[]), Vec::<u8>::new());
    }

    #[test]
    fn shm_mode_parsing_and_cycle() {
        assert_eq!(parse_shm_mode("every"), Some(ShmMode::Every));
        assert_eq!(parse_shm_mode("THROTTLE"), Some(ShmMode::Throttle));
        assert_eq!(parse_shm_mode("off"), Some(ShmMode::Off));
        assert_eq!(parse_shm_mode("quatsch"), None);
        assert_eq!(next_shm_mode(ShmMode::Every), ShmMode::Throttle);
        assert_eq!(next_shm_mode(ShmMode::Throttle), ShmMode::Off);
        assert_eq!(next_shm_mode(ShmMode::Off), ShmMode::Every);
    }

    #[test]
    fn missing_video_file_reports_error() {
        assert!(VideoSource::open_file("/definitiv/nicht/vorhanden.avi").is_err());
    }
}
