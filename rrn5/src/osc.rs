// OSC an Renoise (Ports 7000/7001/7002), wie liblo in der C++-Version.
// Rrn5 sendet nur (empfängt nicht), daher kein OSC-Server nötig.
use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

fn send(port: u16, addr: &str, args: Vec<OscType>) {
    let msg = OscMessage {
        addr: addr.to_string(),
        args,
    };
    let data = match encoder::encode(&OscPacket::Message(msg)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("OSC encode error: {e}");
            return;
        }
    };
    if let Ok(sock) = UdpSocket::bind(("127.0.0.1", 0)) {
        let _ = sock.send_to(&data, ("127.0.0.1", port));
    }
}

pub fn t7000(addr: &str, args: Vec<OscType>) {
    send(7000, addr, args);
}

pub fn t7001(addr: &str, args: Vec<OscType>) {
    send(7001, addr, args);
}

pub fn t7002(addr: &str, args: Vec<OscType>) {
    send(7002, addr, args);
}

/// /renoise/song/bpm "f" bpm  (an 7000 und 7001, wobei 7001 mit Teiler multipliziert)
pub fn send_bpm(fixed_bpm: f32, teiler: f32) {
    t7000("/renoise/song/bpm", vec![OscType::Float(fixed_bpm)]);
    t7001(
        "/renoise/song/bpm",
        vec![OscType::Float(fixed_bpm * teiler)],
    );
}

pub fn transport_start() {
    t7000("/renoise/transport/start", vec![]);
    t7001("/renoise/transport/start", vec![]);
}

pub fn transport_stop() {
    t7000("/renoise/transport/stop", vec![]);
    t7001("/renoise/transport/stop", vec![]);
}

pub fn note_on(port: u16, note: i32, volume: i32) {
    send(
        port,
        "/renoise/trigger/note_on",
        vec![
            OscType::Int(-1),
            OscType::Int(-1),
            OscType::Int(note),
            OscType::Int(volume),
        ],
    );
}

pub fn trigger_midi(port: u16, volume: i32) {
    send(port, "/renoise/trigger/midi", vec![OscType::Int(volume)]);
}