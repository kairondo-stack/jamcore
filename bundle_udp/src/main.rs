use rosc::{decoder, OscType};
use std::io;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::Duration;

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
                    Ok((_, _)) => eprintln!("[7000] Bundle erhalten; wird roh weitergeleitet"),
                    Err(_) => eprintln!("[{listen_port}] Nicht-OSC-Datagramm von {src}"),
                }
                for t in &targets {
                    if let Err(e) = tx.send_to(&buf[..n], t) {
                        eprintln!("Senden an {t}: {e}");
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {}
            Err(e) => return Err(e),
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
    println!("OSC Relay: lauscht 7000 + 7005, Ziel(e): {}", targets.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", "));

    let t_targets = targets.clone();
    let t1 = thread::spawn(move || relay(7000, t_targets));

    let t_targets = targets.clone();
    let t2 = thread::spawn(move || relay(7005, t_targets));

    let _ = t1.join();
    let _ = t2.join();
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

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
