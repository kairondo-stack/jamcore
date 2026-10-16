//! OSC-Relay für JamCore: lauscht auf den Handy-Ports und gibt die Pakete roh
//! an Renoise weiter.
//!
//! Bedienung (alles optional, Vorgaben in Klammern):
//!
//! ```text
//! cargo run --release --manifest-path bundle_udp/Cargo.toml [IP]
//!   IP                 Ziel-IP des Renoise-Rechners; ohne Angabe wird gefragt
//!                      (leere Eingabe = 127.0.0.1)
//!   OSC_LISTEN_PORTS   Lauschports                        (7000,7005)
//!   OSC_TARGETS        gemeinsames Ziel                   (<IP>:7003);
//!                      mehrere per Komma, Hostnamen und IPs
//!   OSC_MAP            je Handy ein eigenes Ziel (Instrument/Port):
//!                      IP=host:port,IP=host:port, z. B.
//!                      10.179.32.50=127.0.0.1:7005,10.179.32.51=127.0.0.1:7006
//! ```
//!
//! Hat ein Handy einen Eintrag in `OSC_MAP`, geht sein OSC **nur** an dieses
//! Ziel; alle anderen bekommen die Ziele aus `OSC_TARGETS`. Ist nur `OSC_MAP`
//! gesetzt, wird keine gemeinsame IP erfragt.
//!
//! Bis zu `MAX_QUELLEN` Handys werden als Quellen gezählt (ist die Liste voll,
//! weicht die älteste). Jede neue Quelle und alle 10 s der Stand („N/10 Handys")
//! stehen im Log. Senderfehler an einzelne Ziele werden geloggt, der Relay
//! läuft weiter und verschluckt nichts.

use jamcore::relay::{self, Quellen, MAX_QUELLEN};
use rosc::{decoder, OscType};
use std::io;
use std::io::Write;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Ziel-IP, wenn keine angegeben oder die Eingabe leer ist.
const DEFAULT_IP: &str = "127.0.0.1";
/// Vorgabe-Port des gemeinsamen Ziels (wie bisher).
const DEFAULT_TARGET_PORT: u16 = 7003;
/// So oft kommt die Statuszeile mit dem Stand der Handys.
const STATUS_SEKUNDEN: u64 = 10;

/// Transiente Empfangsfehler (z.B. EINTR nach Suspend/Resume) dürfen den
/// Relay nicht beenden.
fn is_transient_recv_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// Kaputte Argumente/Umgebungsvariablen als Ein-/Ausgabefehler melden
/// (kein Panic).
fn ungueltig(e: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, e)
}

/// Lauschports aus "7000,7005" parsen (Default bleibt 7000,7005).
fn parse_ports(spec: &str) -> Result<Vec<u16>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        out.push(
            part.parse::<u16>()
                .map_err(|e| format!("Port ungültig {part}: {e}"))?,
        );
    }
    if out.is_empty() {
        return Err("keine Ports angegeben".to_string());
    }
    Ok(out)
}

/// Ziele aus "host:port,host2:port2" auflösen (Hostnamen und IPs).
fn parse_targets(spec: &str) -> Result<Vec<SocketAddr>, String> {
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let addrs: Vec<SocketAddr> = part
            .to_socket_addrs()
            .map_err(|e| format!("Ziel ungültig {part}: {e}"))?
            .collect();
        if addrs.is_empty() {
            return Err(format!("Ziel ohne Adresse: {part}"));
        }
        out.extend(addrs);
    }
    Ok(out)
}

/// Ziel-IP erfragen; eine leere Eingabe ist der lokale Rechner.
fn frage_ip() -> io::Result<String> {
    print!("Ziel-IP des Renoise-Rechners (Enter = {DEFAULT_IP}): ");
    io::stdout().flush()?;
    let mut zeile = String::new();
    io::stdin().read_line(&mut zeile)?;
    let zeile = zeile.trim();
    Ok(if zeile.is_empty() {
        DEFAULT_IP.to_string()
    } else {
        zeile.to_string()
    })
}

/// Gemeinsame Ziele bestimmen: `OSC_TARGETS` gewinnt, sonst die IP vom Aufruf
/// (mit [`DEFAULT_TARGET_PORT`]). Ist nur `OSC_MAP` gesetzt, gibt es kein
/// gemeinsames Ziel — dann wird auch keine IP erfragt.
fn gemeinsame_ziele(
    arg: Option<String>,
    karte: &[(IpAddr, SocketAddr)],
) -> io::Result<Vec<SocketAddr>> {
    if let Ok(spec) = std::env::var("OSC_TARGETS") {
        return parse_targets(&spec).map_err(ungueltig);
    }
    let ip = match arg {
        Some(ip) if !ip.trim().is_empty() => ip.trim().to_string(),
        Some(_) => DEFAULT_IP.to_string(),
        None if !karte.is_empty() => return Ok(Vec::new()),
        None => frage_ip()?,
    };
    parse_targets(&format!("{ip}:{DEFAULT_TARGET_PORT}")).map_err(ungueltig)
}

/// Ziele für ein Paket: ein eigenes Ziel aus `OSC_MAP` gewinnt, sonst gehen
/// die Bytes an alle gemeinsamen Ziele (bestehendes Verhalten).
fn ziele_fuer(eigenes: Option<SocketAddr>, gemeinsam: &[SocketAddr]) -> Vec<SocketAddr> {
    match eigenes {
        Some(ziel) => vec![ziel],
        None => gemeinsam.to_vec(),
    }
}

/// Eine Quelle melden und ihr eigenes Ziel holen (ein Sperrvorgang).
/// Rückgabe: (neu?, Anzahl bekannter Handys, eigenes Ziel aus `OSC_MAP`).
fn quelle_anmelden(quellen: &Mutex<Quellen>, ip: IpAddr) -> (bool, usize, Option<SocketAddr>) {
    // Vergiftete Sperre (Panic im anderen Thread) darf den Relay nicht töten.
    let mut q = quellen.lock().unwrap_or_else(|e| e.into_inner());
    let neu = q.melden(ip);
    (neu, q.anzahl(), q.eigenes_ziel(ip))
}

/// Alle [`STATUS_SEKUNDEN`] den Stand der bekannten Handys melden.
fn status_melden(quellen: Arc<Mutex<Quellen>>) {
    loop {
        thread::sleep(Duration::from_secs(STATUS_SEKUNDEN));
        let anzahl = quellen.lock().unwrap_or_else(|e| e.into_inner()).anzahl();
        println!("relay: {} bekannt", relay::kurztext(anzahl));
    }
}

fn relay(
    listen_port: u16,
    gemeinsam: Vec<SocketAddr>,
    quellen: Arc<Mutex<Quellen>>,
) -> io::Result<()> {
    let rx = UdpSocket::bind(("0.0.0.0", listen_port))?;
    rx.set_read_timeout(Some(Duration::from_millis(200)))?;
    let tx = UdpSocket::bind(("0.0.0.0", 0))?;

    let ziel_text = if gemeinsam.is_empty() {
        "eigene Ziele je Handy (OSC_MAP)".to_string()
    } else {
        format!("{} Ziele", gemeinsam.len())
    };
    println!("Relay läuft auf Port {listen_port} -> {ziel_text}");
    let mut buf = [0u8; 65535];

    loop {
        match rx.recv_from(&mut buf) {
            Ok((n, src)) => {
                match decoder::decode_udp(&buf[..n]) {
                    Ok((_, rosc::OscPacket::Message(msg))) => log_message(listen_port, &msg),
                    Ok((_, _)) => {
                        eprintln!("[{listen_port}] Bundle erhalten; wird roh weitergeleitet")
                    }
                    Err(_) => eprintln!("[{listen_port}] Nicht-OSC-Datagramm von {src}"),
                }
                let (neu, anzahl, eigenes) = quelle_anmelden(&quellen, src.ip());
                if neu {
                    println!("relay: neues Handy {} ({anzahl}/{MAX_QUELLEN})", src.ip());
                }
                let ziele = ziele_fuer(eigenes, &gemeinsam);
                if ziele.is_empty() && neu {
                    // Nur einmal je Handy meckern, sonst rauscht das Log zu.
                    eprintln!(
                        "relay: kein Ziel für {} (OSC_TARGETS oder OSC_MAP setzen)",
                        src.ip()
                    );
                }
                for t in &ziele {
                    if let Err(e) = tx.send_to(&buf[..n], t) {
                        eprintln!("Senden an {t}: {e}");
                    }
                }
            }
            Err(e) if is_transient_recv_error(&e) => {}
            Err(e) => {
                eprintln!("[{listen_port}] Empfangsfehler (weiter): {e}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn log_message(port: u16, m: &rosc::OscMessage) {
    let args: Vec<String> = m
        .args
        .iter()
        .map(|a| match a {
            OscType::Int(i) => i.to_string(),
            OscType::Float(f) => f.to_string(),
            OscType::String(s) => format!("\"{s}\""),
            OscType::Bool(b) => b.to_string(),
            OscType::Nil => "nil".to_string(),
            other => format!("{other:?}"),
        })
        .collect();
    println!("[{port}] {} {}", m.addr, args.join(" "));
}

fn main() -> io::Result<()> {
    // Zuordnung je Handy (OSC_MAP) — leer heisst: nur gemeinsames Ziel.
    let karte = relay::karte_aus_text(&std::env::var("OSC_MAP").unwrap_or_default());
    let gemeinsam = gemeinsame_ziele(std::env::args().nth(1), &karte)?;
    let ports =
        parse_ports(&std::env::var("OSC_LISTEN_PORTS").unwrap_or_else(|_| "7000,7005".into()))
            .map_err(ungueltig)?;

    println!(
        "OSC Relay: lauscht {}, Ziel(e): {}",
        ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(" + "),
        if gemeinsam.is_empty() {
            "je Handy aus OSC_MAP".to_string()
        } else {
            gemeinsam
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    if !karte.is_empty() {
        println!("OSC_MAP: {} eigene Handy-Ziele", karte.len());
    }

    let quellen = Arc::new(Mutex::new({
        let mut q = Quellen::neu();
        q.setze_karte(karte);
        q.setze_gemeinsames_ziel(gemeinsam.first().copied());
        q
    }));

    let mut handles = Vec::new();
    for port in ports {
        let t_gemeinsam = gemeinsam.clone();
        let t_quellen = Arc::clone(&quellen);
        handles.push(thread::spawn(move || {
            if let Err(e) = relay(port, t_gemeinsam, t_quellen) {
                eprintln!("Relay auf Port {port} beendet: {e}");
            }
        }));
    }
    // Statuszeile läuft nebenher; der Relay selbst läuft endlos.
    thread::spawn(move || status_melden(quellen));
    for h in handles {
        let _ = h.join();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_errors_do_not_kill_relay() {
        use std::io::{Error, ErrorKind};
        assert!(is_transient_recv_error(&Error::from(
            ErrorKind::Interrupted
        )));
        assert!(is_transient_recv_error(&Error::from(ErrorKind::WouldBlock)));
        assert!(is_transient_recv_error(&Error::from(ErrorKind::TimedOut)));
        assert!(!is_transient_recv_error(&Error::from(
            ErrorKind::PermissionDenied
        )));
    }

    #[test]
    fn parse_ports_accepts_lists_and_rejects_garbage() {
        assert_eq!(parse_ports("7000,7005").unwrap(), vec![7000, 7005]);
        assert_eq!(parse_ports(" 7100 ").unwrap(), vec![7100]);
        assert!(parse_ports("").is_err());
        assert!(parse_ports("7000,abc").is_err());
        assert!(parse_ports("70000").is_err());
    }

    #[test]
    fn parse_targets_resolves_hostnames_and_ips() {
        let t = parse_targets("localhost:7003").unwrap();
        assert!(t.iter().any(|a| a.port() == 7003));
        let t = parse_targets("127.0.0.1:7003, 127.0.0.1:7004").unwrap();
        assert_eq!(t.len(), 2);
        assert!(parse_targets("").unwrap().is_empty());
        assert!(parse_targets("127.0.0.1:99999").is_err());
    }

    #[test]
    fn eigenes_ziel_gewinnt_vor_gemeinsamem() {
        let gemeinsam = vec![SocketAddr::from(([127, 0, 0, 1], 7003))];
        let eigenes = SocketAddr::from(([127, 0, 0, 1], 7005));
        assert_eq!(ziele_fuer(Some(eigenes), &gemeinsam), vec![eigenes]);
        assert_eq!(ziele_fuer(None, &gemeinsam), gemeinsam);
        assert!(
            ziele_fuer(None, &[]).is_empty(),
            "ohne Ziel wird nichts gesendet"
        );
    }
}
