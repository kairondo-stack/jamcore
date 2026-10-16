//! OSC-Relay-Logik: bekannte Handys (Quellen) zählen und ihre Ziele wählen.
//!
//! Der Relay in `bundle_udp` lauscht auf den JamCore-OSC-Ports (7000/7005) und
//! gibt die Pakete an Renoise weiter. Bis zu [`MAX_QUELLEN`] Handys werden als
//! Quellen gezählt; ist die Liste voll, verdrängt ein neues Handy das
//! **älteste** (wie beim Stimmen-Mixer in [`crate::klang`]). Jedes Handy kann
//! ein eigenes Ziel (Instrument/Port) haben; ohne eigenes Ziel gilt das
//! gemeinsame Ziel, ohne beides wird nichts gesendet.

use std::net::{IpAddr, SocketAddr};

/// Bekannte OSC-Quellen (Handys), höchstens `MAX_QUELLEN`.
pub const MAX_QUELLEN: usize = 10;

/// Bekannte Handys und ihre Ziele (gemeinsam oder je Handy eigenes).
#[derive(Clone, Debug, Default)]
pub struct Quellen {
    /// Quell-IPs in Reihenfolge der ersten Meldung.
    liste: Vec<IpAddr>,
    /// Zuordnung „IP = host:port" je Handy (z. B. aus `OSC_MAP`).
    karte: Vec<(IpAddr, SocketAddr)>,
    /// Ziel für alle Handys ohne eigenen Eintrag.
    gemeinsam: Option<SocketAddr>,
}

impl Quellen {
    /// Leere Quellenliste, kein Ziel.
    pub fn neu() -> Self {
        Quellen::default()
    }

    /// Gemeinsames Ziel für Handys ohne eigenen Eintrag.
    pub fn setze_gemeinsames_ziel(&mut self, ziel: Option<SocketAddr>) {
        self.gemeinsam = ziel;
    }

    /// Zuordnung aus [`karte_aus_text`] übernehmen.
    pub fn setze_karte(&mut self, karte: Vec<(IpAddr, SocketAddr)>) {
        self.karte = karte;
    }

    /// Quelle melden; gibt true zurück, wenn sie neu war. Ist die Liste voll,
    /// verdrängt die neue Quelle die **älteste** (wie der Stimmen-Mixer).
    pub fn melden(&mut self, ip: IpAddr) -> bool {
        if self.liste.contains(&ip) {
            return false;
        }
        if self.liste.len() >= MAX_QUELLEN {
            self.liste.remove(0);
        }
        self.liste.push(ip);
        true
    }

    /// Angemeldete Quellen in Reihenfolge der ersten Meldung.
    pub fn liste(&self) -> Vec<IpAddr> {
        self.liste.clone()
    }

    /// So viele Handys sind bekannt (höchstens [`MAX_QUELLEN`]).
    pub fn anzahl(&self) -> usize {
        self.liste.len()
    }

    /// Nur das eigene Ziel aus der Karte (ohne gemeinsames).
    pub fn eigenes_ziel(&self, ip: IpAddr) -> Option<SocketAddr> {
        self.karte
            .iter()
            .find(|(quelle, _)| *quelle == ip)
            .map(|(_, ziel)| *ziel)
    }

    /// Ziel für eine Quelle: eigenes Ziel, sonst das gemeinsame, sonst None.
    pub fn ziel(&self, ip: IpAddr) -> Option<SocketAddr> {
        self.eigenes_ziel(ip).or(self.gemeinsam)
    }
}

/// Zuordnung „IP=host:port" je Handy (z. B. aus `OSC_MAP`), sonst leer.
/// Leere und kaputte Einträge werden übersprungen (kein Panic); rechts muss
/// eine `IP:port` stehen (Hostnamen kennt die Zuordnung nicht).
pub fn karte_aus_text(text: &str) -> Vec<(IpAddr, SocketAddr)> {
    let mut karte = Vec::new();
    for eintrag in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((links, rechts)) = eintrag.split_once('=') else {
            continue;
        };
        let (Ok(ip), Ok(ziel)) = (
            links.trim().parse::<IpAddr>(),
            rechts.trim().parse::<SocketAddr>(),
        ) else {
            continue;
        };
        karte.push((ip, ziel));
    }
    karte
}

/// Statuszeile „N/10 Handys" (mehr als [`MAX_QUELLEN`] wird nicht angezeigt).
pub fn kurztext(anzahl: usize) -> String {
    format!("{}/{} Handys", anzahl.min(MAX_QUELLEN), MAX_QUELLEN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    fn ziel(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn max_zehn_quellen() {
        let mut q = Quellen::neu();
        for i in 1..=10 {
            assert!(q.melden(ip(10, 0, 0, i)), "Handy {i} ist neu");
        }
        assert_eq!(q.anzahl(), MAX_QUELLEN);
        assert_eq!(q.liste().len(), MAX_QUELLEN);
        // Doppelte Meldungen zählen nicht noch einmal.
        assert!(!q.melden(ip(10, 0, 0, 3)), "bekanntes Handy ist nicht neu");
        assert_eq!(q.anzahl(), MAX_QUELLEN);
    }

    #[test]
    fn aelteste_faellt_raus() {
        let mut q = Quellen::neu();
        for i in 1..=10 {
            q.melden(ip(10, 0, 0, i));
        }
        // Das elfte Handy verdrängt das erste — auch bei voller Liste zählt Neues.
        assert!(q.melden(ip(10, 0, 0, 11)));
        assert_eq!(q.anzahl(), MAX_QUELLEN, "mehr als zehn gibt es nicht");
        let liste = q.liste();
        assert!(!liste.contains(&ip(10, 0, 0, 1)), "das älteste ist weg");
        assert_eq!(liste.first(), Some(&ip(10, 0, 0, 2)));
        assert_eq!(liste.last(), Some(&ip(10, 0, 0, 11)));
        // Bekannte Handys behalten ihren Platz (Reihenfolge der ersten Meldung).
        assert!(!q.melden(ip(10, 0, 0, 5)));
        assert_eq!(q.liste().first(), Some(&ip(10, 0, 0, 2)));
    }

    #[test]
    fn gemeinsames_ziel() {
        let mut q = Quellen::neu();
        q.setze_gemeinsames_ziel(Some(ziel(7003)));
        q.melden(ip(10, 0, 0, 7));
        assert_eq!(q.ziel(ip(10, 0, 0, 7)), Some(ziel(7003)));
        // Auch eine noch nie gemeldete Quelle bekommt das gemeinsame Ziel.
        assert_eq!(q.ziel(ip(10, 0, 0, 9)), Some(ziel(7003)));
        assert_eq!(
            q.eigenes_ziel(ip(10, 0, 0, 7)),
            None,
            "kein eigener Eintrag"
        );
    }

    #[test]
    fn je_quelle_ein_ziel() {
        let mut q = Quellen::neu();
        q.setze_gemeinsames_ziel(Some(ziel(7003)));
        q.setze_karte(karte_aus_text(
            "10.179.32.50=127.0.0.1:7005,10.179.32.51=127.0.0.1:7006",
        ));
        assert_eq!(q.eigenes_ziel(ip(10, 179, 32, 50)), Some(ziel(7005)));
        assert_eq!(
            q.ziel(ip(10, 179, 32, 50)),
            Some(ziel(7005)),
            "eigenes Ziel gewinnt"
        );
        assert_eq!(q.ziel(ip(10, 179, 32, 51)), Some(ziel(7006)));
        // Ohne eigenen Eintrag gilt das gemeinsame Ziel.
        assert_eq!(q.ziel(ip(10, 179, 32, 52)), Some(ziel(7003)));
    }

    #[test]
    fn ziel_fuer_unbekannte_quelle() {
        let mut q = Quellen::neu();
        // Weder eigenes noch gemeinsames Ziel: nichts senden.
        assert_eq!(q.ziel(ip(10, 0, 0, 7)), None);
        // Nur ein eigenes Ziel (kein gemeinsames) — die Zuordnung gilt auch
        // für eine Quelle, die sich noch nicht gemeldet hat.
        q.setze_karte(karte_aus_text("10.0.0.7=127.0.0.1:7010"));
        assert_eq!(q.ziel(ip(10, 0, 0, 7)), Some(ziel(7010)));
        assert_eq!(q.ziel(ip(10, 0, 0, 8)), None);
    }

    #[test]
    fn karte_aus_text_liest_zuordnungen() {
        let k = karte_aus_text(" 10.179.32.50=127.0.0.1:7005 ,10.179.32.51=127.0.0.1:7006 ");
        assert_eq!(k.len(), 2);
        assert_eq!(k[0], (ip(10, 179, 32, 50), ziel(7005)));
        assert_eq!(k[1], (ip(10, 179, 32, 51), ziel(7006)));
        // Leer, kaputt oder ohne Zuweisung: kein Panic, nur nichts.
        assert!(karte_aus_text("").is_empty());
        assert!(karte_aus_text("   ").is_empty());
        assert_eq!(karte_aus_text("10.0.0.1=127.0.0.1:7005,kein=paar").len(), 1);
        assert!(karte_aus_text("10.0.0.1=,=127.0.0.1:7005").is_empty());
        assert!(
            karte_aus_text("10.0.0.1=127.0.0.1").is_empty(),
            "Port fehlt"
        );
    }

    #[test]
    fn kurztext() {
        // Über `super::` rufen, weil dieser Test selbst `kurztext` heißt.
        assert_eq!(super::kurztext(0), "0/10 Handys");
        assert_eq!(super::kurztext(1), "1/10 Handys");
        assert_eq!(super::kurztext(10), "10/10 Handys");
        assert_eq!(
            super::kurztext(12),
            "10/10 Handys",
            "mehr als MAX wird nicht angezeigt"
        );
    }
}
