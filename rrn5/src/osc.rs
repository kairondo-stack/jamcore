// OSC an Renoise (Ports 7000/7001/7002), wie liblo in der C++-Version.
// Rrn5 sendet nur (empfängt nicht), daher kein OSC-Server nötig.
use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

/// Basisport (Default 7000); 7001/7002 sind Basis+1/+2.
pub fn base_port() -> u16 {
    std::env::var("RRN5_OSC_BASE_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(7000)
}

/// Bekannte Ports auf die Basis legen (7000→base, 7001→base+1, 7002→base+2).
pub fn mapped_port(port: u16, base: u16) -> u16 {
    match port {
        7000 => base,
        7001 => base.saturating_add(1),
        7002 => base.saturating_add(2),
        other => other,
    }
}

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
        let _ = sock.send_to(&data, ("127.0.0.1", mapped_port(port, base_port())));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_port_offsets_known_ports() {
        assert_eq!(mapped_port(7000, 7000), 7000);
        assert_eq!(mapped_port(7001, 7000), 7001);
        assert_eq!(mapped_port(7002, 7000), 7002);
        assert_eq!(mapped_port(7000, 7200), 7200);
        assert_eq!(mapped_port(7001, 7200), 7201);
        assert_eq!(mapped_port(7002, 7200), 7202);
        assert_eq!(mapped_port(9999, 7200), 9999);
    }
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