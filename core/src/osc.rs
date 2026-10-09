//! OSC-Ausgang für Renoise (Desktop und Android).
//!
//! Renoise hört auf `/renoise/...`; die Ports 7000/7001/7002 werden wie in der
//! C++-Version behandelt (Basis 7000, 7001 = +1, 7002 = +2). Adresse und
//! Basisport sind bewusst Konfiguration, damit dasselbe Modul im Android-Player
//! ohne Umgebungsvariablen benutzt werden kann.

use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::{SocketAddr, UdpSocket};

pub const DEFAULT_BASE_PORT: u16 = 7000;
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// Bekannte Ports auf die Basis legen (7000 -> base, 7001 -> base+1, 7002 -> base+2).
pub fn mapped_port(port: u16, base: u16) -> u16 {
    match port {
        7000 => base,
        7001 => base.saturating_add(1),
        7002 => base.saturating_add(2),
        other => other,
    }
}

/// Oszillierender Ausgang zu einem festen Ziel.
pub struct OscOut {
    socket: UdpSocket,
    target: SocketAddr,
    base_port: u16,
}

impl OscOut {
    pub fn new(host: &str, base_port: u16) -> Result<Self, String> {
        let target: SocketAddr = format!("{host}:{base_port}")
            .parse()
            .map_err(|e| format!("Adresse {host}:{base_port} ungültig: {e}"))?;
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("UDP-Socket: {e}"))?;
        Ok(OscOut {
            socket,
            target,
            base_port,
        })
    }

    pub fn base_port(&self) -> u16 {
        self.base_port
    }

    pub fn target(&self) -> SocketAddr {
        self.target
    }

    /// Sendet auf `port` (7000/7001/7002 werden gemappt) und liefert die
    /// tatsächlich benutzte Portnummer zurück.
    pub fn send(&self, port: u16, addr: &str, args: Vec<OscType>) -> Result<u16, String> {
        let packet = OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args,
        });
        let data = encoder::encode(&packet).map_err(|e| format!("OSC-Encode {addr}: {e}"))?;
        let used = mapped_port(port, self.base_port);
        self.socket
            .send_to(&data, (self.target.ip(), used))
            .map_err(|e| format!("OSC-Send {addr} -> {used}: {e}"))?;
        Ok(used)
    }

    /// Leitet `send` weiter, ohne Fehler zu melden (für Aufrufer, die nur senden).
    pub fn send_quiet(&self, port: u16, addr: &str, args: Vec<OscType>) {
        let _ = self.send(port, addr, args);
    }
}

/// Renoise-Pfade, die die Desktop-Tools benutzen.
pub struct Renoise<'a>(&'a OscOut);

impl<'a> Renoise<'a> {
    pub fn new(out: &'a OscOut) -> Self {
        Renoise(out)
    }

    /// `/renoise/trigger/note_on` -1 -1 note volume
    pub fn note_on(&self, port: u16, note: i32, volume: i32) -> Result<u16, String> {
        self.0.send(
            port,
            "/renoise/trigger/note_on",
            vec![
                OscType::Int(-1),
                OscType::Int(-1),
                OscType::Int(note),
                OscType::Int(volume),
            ],
        )
    }

    pub fn bpm(&self, port: u16, bpm: f32) -> Result<u16, String> {
        self.0
            .send(port, "/renoise/song/bpm", vec![OscType::Float(bpm)])
    }

    pub fn transport_start(&self, port: u16) -> Result<u16, String> {
        self.0.send(port, "/renoise/transport/start", vec![])
    }

    pub fn transport_stop(&self, port: u16) -> Result<u16, String> {
        self.0.send(port, "/renoise/transport/stop", vec![])
    }

    pub fn pattern_follow(&self, port: u16, on: bool) -> Result<u16, String> {
        self.0.send(
            port,
            "/renoise/song/edit/pattern_follow",
            vec![OscType::Bool(on)],
        )
    }

    /// MIDI-Trigger wie im Desktop-rrn5 (`/renoise/trigger/midi`, Port 7002).
    pub fn trigger_midi(&self, port: u16, value: i32) -> Result<u16, String> {
        self.0
            .send(port, "/renoise/trigger/midi", vec![OscType::Int(value)])
    }
}

/// Tempo für die beiden Renoise-Ports: 7000 bekommt `bpm`, 7001 `bpm * teiler`
/// (wie `send_bpm` im Desktop-rrn5). Ein Teiler, der nichts ändert oder
/// unbrauchbar ist, liefert `None` für 7001.
pub fn bpm_und_teiler(bpm: f32, teiler: f32) -> (f32, Option<f32>) {
    if teiler.is_finite() && teiler > 0.0 && (teiler - 1.0).abs() > 1e-6 {
        (bpm, Some(bpm * teiler))
    } else {
        (bpm, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Bindet einen Empfänger auf einem freien Port und liefert (Port, Socket).
    fn receiver() -> (u16, UdpSocket) {
        let sock = UdpSocket::bind("127.0.0.1:0").expect("bind");
        sock.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let port = sock.local_addr().expect("addr").port();
        (port, sock)
    }

    fn decode_addr(data: &[u8]) -> String {
        let end = data.iter().position(|b| *b == 0).unwrap_or(0);
        String::from_utf8_lossy(&data[..end]).into_owned()
    }

    #[test]
    fn trigger_midi_reaches_receiver_on_mapped_port() {
        let (port, sock) = receiver();
        // 7002 wird auf Basis+2 abgebildet — die Basis liegt zwei darunter,
        // damit der Empfaenger genau den gemappten Port hoert.
        let out = OscOut::new("127.0.0.1", port.saturating_sub(2)).expect("out");
        let used = Renoise::new(&out).trigger_midi(7002, 100).expect("send");
        assert_eq!(used, port, "7002 wird auf Basis+2 gemappt");
        let mut buf = [0u8; 256];
        let (n, _) = sock.recv_from(&mut buf).expect("recv");
        assert_eq!(decode_addr(&buf[..n]), "/renoise/trigger/midi");
    }

    #[test]
    fn bpm_teiler_faltet_das_tempo_fuer_port_7001() {
        // Wie `send_bpm` im Desktop-rrn5: 7000 bekommt das Tempo, 7001 das
        // gefaltete Tempo — aber nur, wenn der Teiler wirklich etwas aendert.
        assert_eq!(bpm_und_teiler(128.0, 1.0), (128.0, None));
        assert_eq!(bpm_und_teiler(128.0, 0.5), (128.0, Some(64.0)));
        assert_eq!(bpm_und_teiler(128.0, 2.0), (128.0, Some(256.0)));
        assert_eq!(
            bpm_und_teiler(128.0, 0.0),
            (128.0, None),
            "Null ist kein Teiler"
        );
        assert_eq!(bpm_und_teiler(128.0, -1.0), (128.0, None));
        assert_eq!(bpm_und_teiler(128.0, f32::NAN), (128.0, None));
    }

    #[test]
    fn mapped_port_offsets_known_ports() {
        assert_eq!(mapped_port(7000, 7000), 7000);
        assert_eq!(mapped_port(7001, 7000), 7001);
        assert_eq!(mapped_port(7002, 7000), 7002);
        assert_eq!(mapped_port(7000, 7003), 7003);
        assert_eq!(mapped_port(7001, 7003), 7004);
        assert_eq!(mapped_port(7002, 7003), 7005);
        assert_eq!(mapped_port(1234, 7000), 1234);
    }

    #[test]
    fn note_on_reaches_receiver_with_renoise_address() {
        let (port, sock) = receiver();
        let out = OscOut::new("127.0.0.1", port).expect("out");
        Renoise::new(&out).note_on(7000, 60, 100).expect("send");
        let mut buf = [0u8; 256];
        let (n, _) = sock.recv_from(&mut buf).expect("recv");
        assert_eq!(decode_addr(&buf[..n]), "/renoise/trigger/note_on");
    }

    #[test]
    fn bpm_is_encoded_as_float_argument() {
        let (port, sock) = receiver();
        let out = OscOut::new("127.0.0.1", port).expect("out");
        Renoise::new(&out).bpm(7000, 128.0).expect("send");
        let mut buf = [0u8; 256];
        let (n, _) = sock.recv_from(&mut buf).expect("recv");
        assert_eq!(decode_addr(&buf[..n]), "/renoise/song/bpm");
        // Danach kommt die Typ-Liste ",f"; der Wert ist big-endian f32.
        let tail = &buf[..n];
        let comma = tail.iter().position(|b| *b == b',').expect("tags");
        assert_eq!(&tail[comma + 1..comma + 2], b"f");
        let value_at = (comma + 4) & !3;
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&tail[value_at..value_at + 4]);
        assert_eq!(f32::from_be_bytes(bytes), 128.0);
    }

    #[test]
    fn port_7001_is_mapped_to_base_plus_one() {
        let (port, sock) = receiver();
        let out = OscOut::new("127.0.0.1", port - 1).expect("out");
        let used = out
            .send(7001, "/renoise/transport/start", vec![])
            .expect("send");
        assert_eq!(used, port);
        let mut buf = [0u8; 256];
        let (n, _) = sock.recv_from(&mut buf).expect("recv");
        assert_eq!(decode_addr(&buf[..n]), "/renoise/transport/start");
    }

    #[test]
    fn invalid_host_is_reported() {
        assert!(OscOut::new("kein.host.", 7000).is_err());
    }
}
