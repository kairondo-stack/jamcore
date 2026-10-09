//! Takte-Maske für den Live-Scan: ein Wert je Spalte, wie `iialpha` im
//! Desktop-`rrn5` (Portierung von `keyEventsxyes` / `biialpha`).
//!
//! Ablage wie am Desktop: das rrn5 liest `KeyEventRender.txt` (das der
//! pattern_recorder mit `s` `0` schreibt) mit **einer Zeile je x-Spalte**
//! `x;key;iialpha;xkeyk` und lässt den Sweep nur über die Spalten laufen, deren
//! Wert ungleich 0 ist. Spalten außerhalb des Vektors sind gesperrt — genau so
//! ist es im rrn5 (`mask_alpha` gibt 0.0 zurück).
//!
//! Auflösung:
//! - [`Aufloesung::Sync`]: ein Wert je Spalte der X-Auflösung (Normalfall).
//! - [`Aufloesung::Frei`]: eigene Schrittzahl, die über die Breite gestreckt wird.

/// Wie die Spalten der Maske zu den Spalten des Sweeps passen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aufloesung {
    /// Ein Wert je Spalte der X-Auflösung.
    Sync,
    /// Eigene Schrittzahl, gleichmäßig über die Breite verteilt.
    Frei { schritte: usize },
}

/// Maske mit einem Wert (0.0 oder 1.0) je Eintrag.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanMaske {
    alpha: Vec<f32>,
    aufloesung: Aufloesung,
}

impl ScanMaske {
    /// Neue Maske, in der noch nichts gesperrt ist.
    pub fn neu(breite: usize, aufloesung: Aufloesung) -> Self {
        let n = match aufloesung {
            Aufloesung::Sync => breite,
            Aufloesung::Frei { schritte } => schritte.max(1),
        };
        Self {
            alpha: vec![1.0; n],
            aufloesung,
        }
    }

    /// Anzahl der Einträge in der Maske.
    pub fn laenge(&self) -> usize {
        self.alpha.len()
    }

    /// Amplitude der Spalte `x` bei einer X-Auflösung von `schritte` Spalten.
    /// Außerhalb der Maske und außerhalb des Sweeps gilt 0.0 (gesperrt).
    pub fn alpha(&self, x: i32, schritte: usize) -> f32 {
        if x < 0 || schritte == 0 || x as usize >= schritte {
            return 0.0;
        }
        let index = match self.aufloesung {
            Aufloesung::Sync => x as usize,
            Aufloesung::Frei { schritte: n } => {
                ((x as usize * n) / schritte).min(n.saturating_sub(1))
            }
        };
        self.alpha.get(index).copied().unwrap_or(0.0)
    }

    /// Spalte auf „durch" (1.0) oder „gesperrt" (0.0) stellen.
    pub fn setze(&mut self, x: usize, durch: bool) {
        if let Some(wert) = self.alpha.get_mut(x) {
            *wert = if durch { 1.0 } else { 0.0 };
        }
    }

    /// Länge an eine neue X-Auflösung anpassen: vorhandene Werte bleiben,
    /// neue Spalten kommen gesperrt dazu (wie im rrn5 der Vektor hinter der
    /// Datei fehlt und 0.0 gilt), beim Schrumpfen wird abgeschnitten.
    pub fn resync(&mut self, schritte: usize) {
        self.alpha.resize(schritte, 0.0);
    }

    /// Länge anpassen, dabei kommen neue Einträge **offen** dazu. Nötig, wenn
    /// nur die Anzeige breiter wird (Zoom auf dem Handy): die neuen Spalten
    /// sollen klingen, nicht wie im rrn5 hinter dem Vektor gesperrt wirken.
    pub fn resync_offen(&mut self, schritte: usize) {
        self.alpha.resize(schritte, 1.0);
    }

    /// Alle Einträge auf einmal umschalten („alles an" / „alles aus").
    pub fn setze_alle(&mut self, durch: bool) {
        let wert = if durch { 1.0 } else { 0.0 };
        for eintrag in &mut self.alpha {
            *eintrag = wert;
        }
    }

    /// Zusaemliche Abschnitte gleicher Amplitude als `(von, bis, alpha)`,
    /// halb offen. Damit malt die Leiste auch bei mehreren hundert Spalten
    /// nur wenige Rechtecke.
    pub fn abschnitte(&self, schritte: usize) -> Vec<(usize, usize, f32)> {
        if schritte == 0 || self.alpha.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<(usize, usize, f32)> = Vec::new();
        for x in 0..schritte {
            let alpha = self.alpha(x as i32, schritte);
            match out.last_mut() {
                Some(abschnitt) if abschnitt.2 == alpha => abschnitt.1 = x + 1,
                _ => {
                    let von = out.last().map(|a| a.1).unwrap_or(0);
                    out.push((von, x + 1, alpha));
                }
            }
        }
        out
    }

    /// Index des Eintrags, der die Spalte `x` abdeckt — fuer das Tippen auf
    /// der Maskenleiste. `None` ausserhalb des Sweeps.
    pub fn eintrag_fuer_x(&self, x: i32, schritte: usize) -> Option<usize> {
        if x < 0 || schritte == 0 || x as usize >= schritte {
            return None;
        }
        let index = match self.aufloesung {
            Aufloesung::Sync => x as usize,
            Aufloesung::Frei { schritte: n } => {
                ((x as usize * n) / schritte).min(n.saturating_sub(1))
            }
        };
        (index < self.alpha.len()).then_some(index)
    }

    /// Als `KeyEventRender.txt` schreiben (`pattern_recorder` `s` `0`):
    /// eine Zeile je Spalte `x;key;iialpha;xkeyk`, alle drei Felder 1, wenn die
    /// Spalte durchkommt, sonst 0.
    pub fn als_keyevent(&self, schritte: usize) -> String {
        let mut out = String::new();
        for x in 0..schritte {
            if self.alpha(x as i32, schritte) > 0.0 {
                out += &format!("{x};1;1;1\n");
            } else {
                out += &format!("{x};0;0;0\n");
            }
        }
        out
    }

    /// Aus `KeyEventRender.txt` lesen (`rrn5` `space`). Getrennt wird wie im
    /// rrn5 an `;`, die Felder sind `x`, `key`, `iialpha`, `xkeyk`; durchkommt,
    /// was `alpha` ungleich 0 oder `key` gleich 1 ist. Leere Zeilen und
    /// Zeilen mit zu wenigen Feldern werden uebersprungen.
    pub fn aus_keyevent(text: &str) -> Self {
        let mut alpha: Vec<f32> = Vec::new();
        for zeile in text.lines() {
            let felder: Vec<&str> = zeile.split(';').collect();
            if felder.len() < 3 {
                continue;
            }
            let (Ok(x), Ok(key), Ok(a)) = (
                felder[0].trim().parse::<i64>(),
                felder[1].trim().parse::<f32>(),
                felder[2].trim().parse::<f32>(),
            ) else {
                continue;
            };
            if x < 0 {
                continue;
            }
            let x = x as usize;
            if alpha.len() <= x {
                alpha.resize(x + 1, 0.0);
            }
            alpha[x] = if a != 0.0 {
                a
            } else if key == 1.0 {
                1.0
            } else {
                0.0
            };
        }
        Self {
            alpha,
            aufloesung: Aufloesung::Sync,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neue_maske_ist_alles_durch() {
        let maske = ScanMaske::neu(8, Aufloesung::Sync);
        assert_eq!(maske.laenge(), 8);
        assert_eq!(maske.alpha(0, 8), 1.0);
        assert_eq!(maske.alpha(7, 8), 1.0);
    }

    #[test]
    fn sync_ist_ein_wert_pro_spalte() {
        let mut maske = ScanMaske::neu(8, Aufloesung::Sync);
        maske.setze(3, false);
        assert_eq!(maske.alpha(3, 8), 0.0, "die gesperrte Spalte");
        assert_eq!(maske.alpha(2, 8), 1.0, "davor");
        assert_eq!(maske.alpha(4, 8), 1.0, "danach");
    }

    #[test]
    fn ausserhalb_der_maske_ist_gesperrt() {
        // rrn5: `mask_alpha` gibt 0.0 zurück, wenn die Spalte im Vektor fehlt.
        let mut maske = ScanMaske::neu(10, Aufloesung::Sync);
        for x in 0..10 {
            maske.setze(x, false);
        }
        assert_eq!(maske.alpha(9, 640), 0.0, "letzter Eintrag");
        assert_eq!(
            maske.alpha(10, 640),
            0.0,
            "Spalte hinter dem Vektor bleibt gesperrt"
        );
        assert_eq!(maske.alpha(639, 640), 0.0);
    }

    #[test]
    fn ausserhalb_der_spalten_ist_gesperrt() {
        let sync = ScanMaske::neu(64, Aufloesung::Sync);
        assert_eq!(sync.alpha(64, 64), 0.0, "genau hinter dem Sweep");
        let frei = ScanMaske::neu(0, Aufloesung::Frei { schritte: 4 });
        assert_eq!(frei.alpha(64, 64), 0.0, "frei genauso");
        assert_eq!(frei.alpha(-1, 64), 0.0, "negative Spalte");
    }

    #[test]
    fn frei_streckt_ueber_die_breite() {
        let mut maske = ScanMaske::neu(0, Aufloesung::Frei { schritte: 4 });
        maske.setze(1, false);
        assert_eq!(maske.laenge(), 4);
        for x in 0..8 {
            let erwartet = if (2..4).contains(&x) { 0.0 } else { 1.0 };
            assert_eq!(maske.alpha(x, 8), erwartet, "Spalte {x}");
        }
    }

    #[test]
    fn setzen_schaltet_ja_nein() {
        let mut maske = ScanMaske::neu(8, Aufloesung::Sync);
        maske.setze(2, false);
        assert_eq!(maske.alpha(2, 8), 0.0);
        maske.setze(2, true);
        assert_eq!(maske.alpha(2, 8), 1.0, "wieder durch");
        maske.setze(99, false);
        assert_eq!(maske.laenge(), 8, "ausserhalb bleibt die Laenge");
    }

    #[test]
    fn resync_behaelt_werte_und_sperrt_den_rest() {
        let mut maske = ScanMaske::neu(4, Aufloesung::Sync);
        maske.setze(0, false);
        maske.resync(10);
        assert_eq!(maske.laenge(), 10);
        assert_eq!(maske.alpha(0, 10), 0.0, "alter Wert bleibt");
        assert_eq!(maske.alpha(3, 10), 1.0);
        assert_eq!(maske.alpha(4, 10), 0.0, "neue Spalten sind gesperrt");
        maske.resync(2);
        assert_eq!(maske.laenge(), 2, "schrumpfen schneidet ab");
        assert_eq!(maske.alpha(0, 2), 0.0, "Wert bleibt erhalten");
    }

    #[test]
    fn resync_offen_lasst_neue_spalten_klingen() {
        let mut maske = ScanMaske::neu(4, Aufloesung::Sync);
        maske.setze(0, false);
        maske.resync_offen(8);
        assert_eq!(maske.laenge(), 8);
        assert_eq!(maske.alpha(0, 8), 0.0, "alter Wert bleibt");
        assert_eq!(maske.alpha(4, 8), 1.0, "Zoom-Fortsetzung klingt");
        assert_eq!(maske.alpha(7, 8), 1.0);
        maske.resync_offen(2);
        assert_eq!(maske.laenge(), 2, "schrumpfen schneidet ab");
    }

    #[test]
    fn alles_an_und_alles_aus() {
        let mut maske = ScanMaske::neu(8, Aufloesung::Sync);
        maske.setze(3, false);
        maske.setze_alle(false);
        assert!(
            maske.abschnitte(8) == vec![(0, 8, 0.0)],
            "alles gesperrt: {:?}",
            maske.abschnitte(8)
        );
        maske.setze_alle(true);
        assert!(
            maske.abschnitte(8) == vec![(0, 8, 1.0)],
            "alles durch: {:?}",
            maske.abschnitte(8)
        );
    }

    #[test]
    fn abschnitte_fasst_gleiche_werte_zusammen() {
        let mut maske = ScanMaske::neu(8, Aufloesung::Sync);
        for x in 2..6 {
            maske.setze(x, false);
        }
        assert_eq!(
            maske.abschnitte(8),
            vec![(0, 2, 1.0), (2, 6, 0.0), (6, 8, 1.0)],
            "drei Bloecke statt acht Streifen"
        );
    }

    #[test]
    fn eintrag_fuer_x_findet_denselben_eintrag_wie_alpha() {
        let mut maske = ScanMaske::neu(0, Aufloesung::Frei { schritte: 4 });
        maske.setze(1, false);
        for x in 0..8 {
            // Frei{4} ueber acht Spalten: zwei Spalten je Eintrag.
            let index = x as usize / 2;
            assert_eq!(maske.eintrag_fuer_x(x, 8), Some(index), "Spalte {x}");
            assert_eq!(
                maske.alpha(x, 8) == 0.0,
                index == 1,
                "gesperrt passt zum Eintrag {index} bei Spalte {x}"
            );
        }
        assert_eq!(maske.eintrag_fuer_x(8, 8), None, "ausserhalb des Sweeps");
    }

    #[test]
    fn keyevent_rundlauf() {
        // Format wie pattern_recorder `s` `0`: `x;key;iialpha;xkeyk`, 1 = kommt durch.
        let mut maske = ScanMaske::neu(24, Aufloesung::Sync);
        maske.setze(0, false);
        maske.setze(22, false);
        let text = maske.als_keyevent(24);
        assert_eq!(text.lines().count(), 24, "eine Zeile je Spalte");
        assert_eq!(text.lines().next().unwrap(), "0;0;0;0");
        assert!(text.lines().any(|z| z == "1;1;1;1"), "{text}");
        assert!(text.lines().any(|z| z == "22;0;0;0"), "{text}");
        assert!(text.lines().any(|z| z == "23;1;1;1"), "{text}");

        let zurueck = ScanMaske::aus_keyevent(&text);
        assert_eq!(zurueck.laenge(), 24);
        assert_eq!(zurueck.alpha(0, 24), 0.0, "gesperrt bleibt gesperrt");
        assert_eq!(zurueck.alpha(1, 24), 1.0);
        assert_eq!(zurueck.alpha(22, 24), 0.0);
        assert_eq!(zurueck.alpha(23, 24), 1.0);
    }

    #[test]
    fn keyevent_liest_0_als_aus() {
        // So sieht die echte Datei aus; Müllzeilen dürfen nichts kaputtmachen.
        let text = "0;0;0;0\n1;1;1;1\n\nkaputt\n2;1;0.5;1\n3;1;0;1\n";
        let maske = ScanMaske::aus_keyevent(text);
        assert_eq!(maske.laenge(), 4);
        assert_eq!(maske.alpha(0, 4), 0.0, "0 = gesperrt");
        assert_eq!(maske.alpha(1, 4), 1.0);
        assert_eq!(
            maske.alpha(2, 4),
            0.5,
            "halbe Amplitude bleibt erhalten (skaliert Farbe und Laute)"
        );
        assert_eq!(maske.alpha(3, 4), 1.0, "key=1 kommt auch ohne alpha durch");
        assert_eq!(ScanMaske::aus_keyevent("").laenge(), 0, "leere Datei");
    }

    #[test]
    fn echte_desktop_datei_wird_gelesen() {
        // Auszug aus `pattern_recorder/KeyEventRender.txt` (500 Zeilen, 26 mit
        // Note) — so liest der rrn5 die Maske nach `space`.
        let text = "0;1;1;1\n1;0;0;0\n2;0;0;0\n3;0;0;0\n4;0;0;0\n5;0;0;0\n6;0;0;0\n\
                    7;0;0;0\n8;0;0;0\n9;0;0;0\n10;0;0;0\n11;0;0;0\n12;0;0;0\n13;0;0;0\n\
                    14;0;0;0\n15;0;0;0\n16;0;0;0\n17;0;0;0\n18;0;0;0\n19;0;0;0\n20;0;0;0\n\
                    21;0;0;0\n22;1;1;1\n23;0;0;0\n";
        let maske = ScanMaske::aus_keyevent(text);
        assert_eq!(maske.laenge(), 24);
        assert_eq!(maske.alpha(0, 24), 1.0, "x=0 hat eine Note");
        assert_eq!(maske.alpha(21, 24), 0.0);
        assert_eq!(maske.alpha(22, 24), 1.0, "x=22 hat eine Note");
        assert_eq!(maske.alpha(23, 24), 0.0);
        // Und wieder zurueck: die beiden Noten bleiben, sonst alles durch.
        let zurueck = maske.als_keyevent(24);
        let neu = ScanMaske::aus_keyevent(&zurueck);
        assert_eq!(neu.alpha(0, 24), 1.0);
        assert_eq!(neu.alpha(22, 24), 1.0);
        assert_eq!(neu.alpha(5, 24), 0.0);
    }
}
