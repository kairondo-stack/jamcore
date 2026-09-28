//! Erzeugt ein kleines MJPG-Testvideo (Farbwechsel pro Frame) für E2E-Tests.
use opencv::prelude::*;
use opencv::videoio::{VideoWriter, VideoWriterTrait, CAP_ANY};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/opencode/testvideo.avi".to_string());
    let (w, h, fps, frames) = (64, 48, 10.0, 10);
    let mut writer = VideoWriter::new(
        &path,
        opencv::videoio::VideoWriter::fourcc('M', 'J', 'P', 'G')?,
        fps,
        opencv::core::Size::new(w, h),
        true,
    )?;
    for i in 0..frames {
        let mut mat = Mat::new_rows_cols_with_default(
            h,
            w,
            opencv::core::CV_8UC3,
            opencv::core::Scalar::new(((i * 25) % 256) as f64, 40.0, 200.0, 0.0),
        )?;
        writer.write(&mat)?;
        let _ = &mut mat;
    }
    writer.release()?;
    println!("Testvideo geschrieben: {path} ({w}x{h}, {frames} Frames)");
    Ok(())
}
