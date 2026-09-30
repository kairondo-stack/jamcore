//! Ableton-Link-Spike: Session beitreten, Tempo/Phase anzeigen.
//! Mit LINK_SET_BPM wird nach 1 s einmalig ein Tempo gesetzt (Master);
//! andere Prozesse sollten dieses Tempo übernehmen.
use rusty_link::{AblLink, SessionState};
use std::time::{Duration, Instant};

fn main() {
    let bpm: f64 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(120.0);
    let set_bpm: Option<f64> = std::env::var("LINK_SET_BPM")
        .ok()
        .and_then(|v| v.parse().ok());
    let seconds: f64 = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);

    let link = AblLink::new(bpm);
    link.enable(true);
    let mut session = SessionState::new();
    println!("link session gestartet (bpm={bpm}, set={set_bpm:?})");

    let start = Instant::now();
    let mut committed = false;
    while start.elapsed().as_secs_f64() < seconds {
        if let Some(b) = set_bpm {
            if !committed && start.elapsed().as_secs_f64() >= 1.0 {
                link.capture_app_session_state(&mut session);
                session.set_tempo(b, link.clock_micros());
                link.commit_app_session_state(&session);
                println!("link set tempo={b}");
                committed = true;
            }
        }
        link.capture_app_session_state(&mut session);
        let beat = session.beat_at_time(link.clock_micros(), 4.0);
        println!(
            "link tempo={:.2} peers={} beat={:.3} playing={}",
            session.tempo(),
            link.num_peers(),
            beat,
            session.is_playing()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    link.enable(false);
    println!("link session beendet");
}
