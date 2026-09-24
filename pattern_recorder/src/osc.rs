// OSC an Renoise (Port 7005), wie liblo in der C++-Version
use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

const OSC_PORT: u16 = 7005;

fn osc_send(msg: OscMessage) {
    let data = match encoder::encode(&OscPacket::Message(msg)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("OSC encode error: {e}");
            return;
        }
    };
    if let Ok(sock) = UdpSocket::bind(("127.0.0.1", 0)) {
        let _ = sock.send_to(&data, ("127.0.0.1", OSC_PORT));
    }
}

/// /renoise/trigger/note_on "iiii" -1 -1 pitch 127
pub fn send_note_to_renoise(_instrument: i32, pitch: i32) {
    let addr = "/renoise/trigger/note_on".to_string();
    let args = vec![
        OscType::Int(-1),
        OscType::Int(-1),
        OscType::Int(pitch),
        OscType::Int(127),
    ];
    osc_send(OscMessage { addr, args });
}

/// /renoise/song/bpm "f" bpm
pub fn send_bpm_to_renoise(bpm: f32) {
    let addr = "/renoise/song/bpm".to_string();
    let args = vec![OscType::Float(bpm)];
    osc_send(OscMessage { addr, args });
}

/// /renoise/transport/start bzw. stop (keine Argumente)
pub fn transport(playing: bool) {
    let addr = if playing { "/renoise/transport/start" } else { "/renoise/transport/stop" }.to_string();
    osc_send(OscMessage { addr, args: vec![] });
}

/// /renoise/song/edit/mode "B"
pub fn edit_mode(on: bool) {
    let addr = "/renoise/song/edit/mode".to_string();
    let args = vec![OscType::Bool(on)];
    osc_send(OscMessage { addr, args });
}