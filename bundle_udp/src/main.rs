use rosc::{decoder, OscType};
use std::io;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::Duration;

/// Transiente Empfangsfehler (z.B. EINTR nach Suspend/Resume) dürfen den
/// Relay nicht beenden.
fn is_transient_recv_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
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

fn relay(listen_port: u16, targets: Vec<SocketAddr>) -> io::Result<()> {
    let rx = UdpSocket::bind(("0.0.0.0", listen_port))?;
    rx.set_read_timeout(Some(Duration::from_millis(200)))?;
    let tx = UdpSocket::bind(("0.0.0.0", 0))?;

    println!("Relay läuft auf Port {listen_port} -> {} Ziele", targets.len());
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
                for t in &targets {
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
    let arg_target: Option<String> = std::env::args().nth(1);
    let target_ip = match arg_target {
        Some(ip) => ip,
        None => {
            print!("Enter the IP addressOUT: ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            line.trim().to_string()
        }
    };

    let spec = std::env::var("OSC_TARGETS").unwrap_or_else(|_| format!("{target_ip}:7003"));
    let targets = parse_targets(&spec)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let ports = parse_ports(
        &std::env::var("OSC_LISTEN_PORTS").unwrap_or_else(|_| "7000,7005".into()),
    )
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    println!(
        "OSC Relay: lauscht {}, Ziel(e): {}",
        ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(" + "),
        targets.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ")
    );
    let mut handles = Vec::new();
    for port in ports {
        let t_targets = targets.clone();
        handles.push(thread::spawn(move || {
            if let Err(e) = relay(port, t_targets) {
                eprintln!("Relay auf Port {port} beendet: {e}");
            }
        }));
    }
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
        assert!(is_transient_recv_error(&Error::from(ErrorKind::Interrupted)));
        assert!(is_transient_recv_error(&Error::from(ErrorKind::WouldBlock)));
        assert!(is_transient_recv_error(&Error::from(ErrorKind::TimedOut)));
        assert!(!is_transient_recv_error(&Error::from(ErrorKind::PermissionDenied)));
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
}
