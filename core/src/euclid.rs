//! Euklidisches Muster (Max-Patch `eucl-midi-in out`): `schlag` Einsen auf
//! `schritte` Zellen, möglichst gleichmäßig verteilt — z. B. 5 Schläge auf 16
//! Schritte als „1 0 0 0 1 0 0 1 0 0 1 0 0 1 0 0“. Hardwarefrei, damit Desktop
//! und Android dieselbe Verteilung sehen.

/// Obergrenze der Zellenzahl (Rhythmus-Tab und Textfeld, 1..=64).
pub const MAX_SCHRITTE: usize = 64;

/// Die häufigsten Presets als `(Schläge, Schritte)`, der Reihe nach:
/// Viertel auf 16eln (4/16), das 5/16-Muster aus dem Patch, 7/16, 3/8, 5/8,
/// 2/5, 13/16 und 9/16.
pub const PRESETS: [(usize, usize); 8] = [
    (4, 16),
    (5, 16),
    (7, 16),
    (3, 8),
    (5, 8),
    (2, 5),
    (13, 16),
    (9, 16),
];

/// Euklidisches Muster: `schritte` Zellen, `schlag` Einsen, möglichst
/// gleichmäßig verteilt.
///
/// Klassischer Bresenham/Euklid-Verteiler: Zelle `i` klingt, wenn
/// `(i * schlag) % schritte < schlag` — die Schläge liegen dadurch so weit
/// auseinander wie möglich. `schritte` wird auf 1..=[`MAX_SCHRITTE`] geklemmt,
/// `schlag` auf `0..=schritte` (0 = nur Nullen, `schritte` = nur Einsen).
/// Panik gibt es in keinem Fall, auch nicht bei 0.
pub fn muster(schritte: usize, schlag: usize) -> Vec<bool> {
    let schritte = schritte.clamp(1, MAX_SCHRITTE);
    let schlag = schlag.min(schritte);
    (0..schritte)
        .map(|i| (i * schlag) % schritte < schlag)
        .collect()
}

/// Rotation des Musters um `versatz` Zellen: ein Schlag wandert `versatz`
/// Zellen **nach rechts**, am Musterende läuft er um. `versatz` darf größer
/// als die Länge sein (ganze Umläufe fallen weg), ein leeres Muster bleibt leer.
pub fn rotiere(muster: &[bool], versatz: usize) -> Vec<bool> {
    let n = muster.len();
    if n == 0 {
        return Vec::new();
    }
    let versatz = versatz % n;
    (0..n).map(|i| muster[(i + n - versatz) % n]).collect()
}

/// Text „1 0 1 0 0“ ⇄ Muster (für die Textfeld-Eingabe wie im Patch).
///
/// Trenner sind Leerzeichen, Kommas, Semikolons und Zeilenumbrüche; als Eins
/// zählen `1`, `x` und `X`, als Null `0` und `.` — auch als Block („1010“).
/// Ein unbekanntes Zeichen oder ein leerer Text ergibt `None`; über
/// [`MAX_SCHRITTE`] Zellen wird gekappt.
pub fn aus_text(text: &str) -> Option<Vec<bool>> {
    let mut out = Vec::new();
    'token: for token in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        for zeichen in token.chars() {
            if out.len() >= MAX_SCHRITTE {
                break 'token;
            }
            match zeichen {
                '1' | 'x' | 'X' => out.push(true),
                '0' | '.' => out.push(false),
                _ => return None,
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Muster als Text „1 0 1 0 0“ (leeres Muster = leerer Text).
pub fn als_text(muster: &[bool]) -> String {
    muster
        .iter()
        .map(|zelle| if *zelle { "1" } else { "0" })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Häufigste Presets: (Schläge, Schritte) → Text.
/// Das `n`-te Preset aus [`PRESETS`]; `None` hinter dem Listenende.
pub fn preset(n: usize) -> Option<(usize, usize)> {
    PRESETS.get(n).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zyklische Abstände zwischen den Einsen (für die Gleichmäßigkeit).
    fn abstaende(muster: &[bool]) -> Vec<usize> {
        let einsen: Vec<usize> = muster
            .iter()
            .enumerate()
            .filter(|(_, zelle)| **zelle)
            .map(|(i, _)| i)
            .collect();
        let mut out: Vec<usize> = einsen.windows(2).map(|p| p[1] - p[0]).collect();
        if let (Some(erste), Some(letzte)) = (einsen.first(), einsen.last()) {
            out.push(muster.len() - letzte + erste);
        }
        out
    }

    #[test]
    fn fuenf_auf_sechzehn() {
        // Das Muster aus dem Max-Patch: 5 Schläge auf 16 Schritte.
        let m = muster(16, 5);
        assert_eq!(m.len(), 16);
        assert_eq!(m.iter().filter(|zelle| **zelle).count(), 5);
        assert_eq!(als_text(&m), "1 0 0 0 1 0 0 1 0 0 1 0 0 1 0 0");
        // Gleichmäßig: der längste Abstand ist höchstens eins mehr als der
        // kürzeste (hier 3 und 4 Schritte, kein 5er-Pulk am Anfang).
        let abstaende = abstaende(&m);
        let min = *abstaende.iter().min().unwrap();
        let max = *abstaende.iter().max().unwrap();
        assert!(max - min <= 1, "Abstände {abstaende:?}");
    }

    #[test]
    fn alle_schritte_ein() {
        assert_eq!(muster(4, 4), vec![true; 4]);
        // Mehr Schläge als Zellen werden auf „alle“ geklemmt.
        assert_eq!(muster(4, 99), vec![true; 4]);
        assert_eq!(als_text(&muster(3, 3)), "1 1 1");
    }

    #[test]
    fn kein_schlag() {
        let m = muster(16, 0);
        assert_eq!(m.len(), 16);
        assert!(m.iter().all(|zelle| !*zelle));
        assert_eq!(als_text(&m), "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0");
    }

    #[test]
    fn schritte_geklemmt() {
        // 0 Zellen gibt es nicht: eine Zelle.
        assert_eq!(muster(0, 0), vec![false], "eine Zelle, kein Schlag");
        assert_eq!(muster(0, 5), vec![true], "eine Zelle, Schlag geklemmt");
        // Obergrenze 64: mehr Zellen werden geklemmt, die Schläge bleiben
        // verteilt.
        let m = muster(200, 5);
        assert_eq!(m.len(), MAX_SCHRITTE);
        assert_eq!(m.iter().filter(|zelle| **zelle).count(), 5);
    }

    #[test]
    fn rotiert() {
        let m = [true, false, false, true];
        assert_eq!(
            rotiere(&m, 1),
            vec![true, true, false, false],
            "ein Schlag wandert eine Zelle nach rechts"
        );
        assert_eq!(rotiere(&m, 0), m.to_vec());
        assert_eq!(rotiere(&m, 4), m.to_vec(), "ganze Umläufe ändern nichts");
        assert_eq!(rotiere(&m, 5), rotiere(&m, 1), "Umlauf + 1");
        assert!(rotiere(&[], 3).is_empty(), "leeres Muster bleibt leer");
    }

    #[test]
    fn text_hin_und_zurueck() {
        let m = aus_text("1 0 1 0 0").expect("lesbar");
        assert_eq!(m, vec![true, false, true, false, false]);
        assert_eq!(als_text(&m), "1 0 1 0 0");
        // Weitere Schreibweisen und Trenner.
        assert_eq!(aus_text("1010"), Some(vec![true, false, true, false]));
        assert_eq!(
            aus_text("x . x . , 1"),
            Some(vec![true, false, true, false, true])
        );
        assert_eq!(als_text(&[]), "", "leeres Muster = leerer Text");
        assert_eq!(aus_text(""), None);
        assert_eq!(aus_text("1 2 0"), None, "unbekanntes Zeichen");
        // Über der Obergrenze wird gekappt statt zu panicken.
        let lang = aus_text(&vec!["1"; 100].join(" ")).expect("lesbar");
        assert_eq!(lang.len(), MAX_SCHRITTE);
    }

    #[test]
    fn presets() {
        assert_eq!(preset(0), Some((4, 16)));
        assert_eq!(preset(1), Some((5, 16)), "das Muster aus dem Patch");
        assert_eq!(preset(usize::MAX), None);
        for (n, (schlag, schritte)) in PRESETS.iter().enumerate() {
            assert_eq!(preset(n), Some((*schlag, *schritte)), "Preset {n}");
            let m = muster(*schritte, *schlag);
            assert_eq!(m.len(), *schritte, "Preset {n}");
            assert_eq!(
                m.iter().filter(|zelle| **zelle).count(),
                *schlag,
                "Preset {n}"
            );
            assert!(
                *schlag > 0 && *schlag <= *schritte && *schritte <= MAX_SCHRITTE,
                "Preset {n} unbrauchbar"
            );
        }
    }
}
