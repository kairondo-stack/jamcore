use rosc::{decoder, OscType};
use std::io;
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::Duration;

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
                    tx.send_to(&buf[..n], t)?;
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

    let mut targets: Vec<SocketAddr> = Vec::new();
    for p in std::env::var("OSC_TARGETS")
        .unwrap_or_else(|_| format!("{target_ip}:7003"))
        .split(',')
        .filter(|s| !s.trim().is_empty())
    {
        let addr: SocketAddr = p
            .trim()
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("Ziel ungültig {p}: {e}")))?;
        targets.push(addr);
    }
    println!("OSC Relay: lauscht 7000 + 7005, Ziel(e): {}", targets.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", "));

    let t_targets = targets.clone();
    let t1 = thread::spawn(move || relay(7000, t_targets));

    let t_targets = targets.clone();
    let t2 = thread::spawn(move || relay(7005, t_targets));

    let _ = t1.join();
    let _ = t2.join();
    Ok(())
}