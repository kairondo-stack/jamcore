//! Takt-Maske: Schrittweiten und Verteilungen wie die Recorder-Tastenmaske.
//!
//! Beim Desktop-Recorder bestimmen die Tastendrücke und die Zeit dazwischen,
//! wie viele Schritte zwischen zwei Noten liegen und wie sie verteilt sind
//! (gleichmäßig, beschleunigend oder bremsend – die `kdtanz`-Verteilung).
//! Hier steht die hardwarefreie Rechnung; das Touchpad liefert die Weg- und
//! Zeitproben des Wischens.

/// Wie die Zwischenschritte zwischen zwei Tasten verteilt werden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verteilung {
    /// Gleiche Abstände (annähernd linear gewischt).
    Linear,
    /// Abstände werden größer (Wisch wird schneller – beschleunigend).
    Beschleunigend,
    /// Abstände werden kleiner (Wisch wird langsamer – bremsend).
    Bremsend,
}

/// Position im Tastenfeld (0-basiert) fuer eine Nummernblock-Ziffer, wenn das
/// Feld in **Leserichtung** liegt (alte Maske: 0,1,2,… von links oben).
/// Ziffer 1..9 trifft die Positionen 0..8, die Ziffer 0 die neunte Position.
pub fn position_linear(ziffer: u8, feld: usize) -> Option<usize> {
    let index = match ziffer {
        1..=9 => (ziffer - 1) as usize,
        0 => 9,
        _ => return None,
    };
    (index < feld).then_some(index)
}

/// Position im 10er-Feld von Maske 2: das Feld liegt in **Nummernblock-
/// Anordnung** (7-8-9 / 4-5-6 / 1-2-3 / 0), die Ziffer trifft also den
/// raeumlich gleichen Knopf.
pub fn position_numpad(ziffer: u8) -> Option<usize> {
    match ziffer {
        7 => Some(0),
        8 => Some(1),
        9 => Some(2),
        4 => Some(3),
        5 => Some(4),
        6 => Some(5),
        1 => Some(6),
        2 => Some(7),
        3 => Some(8),
        0 => Some(9),
        _ => None,
    }
}

/// Positionen der Zwischenschritte zwischen zwei Tastendrücken, als Anteil
/// von 0.0 (erster Druck) bis 1.0 (zweiter Druck), ohne die Endpunkte.
///
/// - `Linear`: gleichmäßig (i/n).
/// - `Beschleunigend`: quadratisch zunehmend (i^2/n^2) - die Schritte
///   werden laenger (die Positionen liegen unter der Geraden), wie beim
///   immer schnelleren Wischen.
/// - `Bremsend`: das Spiegelbild (1 - (1 - i/n)²).
pub fn schritt_positionen(anzahl: u32, profil: Verteilung) -> Vec<f32> {
    let n = anzahl.max(1) as f32;
    (1..anzahl.max(1))
        .map(|i| {
            let t = i as f32 / n;
            match profil {
                Verteilung::Linear => t,
                Verteilung::Beschleunigend => t * t,
                Verteilung::Bremsend => 1.0 - (1.0 - t) * (1.0 - t),
            }
        })
        .collect()
}

/// Erkennt das Verteilungsprofil aus den Geschwindigkeiten der Wischproben
/// (Weg je Zeitfenster). Die zweite Hälfte wird mit der ersten verglichen;
/// ein Viertel Unterschied entscheidet, sonst gilt der Wisch als linear.
pub fn profil_aus_geschwindigkeiten(geschwindigkeiten: &[f32]) -> Verteilung {
    if geschwindigkeiten.len() < 4 {
        return Verteilung::Linear;
    }
    let mitte = geschwindigkeiten.len() / 2;
    let anfang: f32 = geschwindigkeiten[..mitte].iter().sum::<f32>() / mitte as f32;
    let ende: f32 =
        geschwindigkeiten[mitte..].iter().sum::<f32>() / (geschwindigkeiten.len() - mitte) as f32;
    if anfang <= 1e-6 {
        return Verteilung::Linear;
    }
    let verhaeltnis = ende / anfang;
    if verhaeltnis > 1.25 {
        Verteilung::Beschleunigend
    } else if verhaeltnis < 0.8 {
        Verteilung::Bremsend
    } else {
        Verteilung::Linear
    }
}

/// Anzahl der Schritte aus der Wischlänge: eine durchstrichene Taste zählt
/// als ein Schritt, mindestens einer, höchstens `max`.
pub fn schritte_aus_wisch(weg_px: f32, taste_px: f32, max: u32) -> u32 {
    if !weg_px.is_finite() || weg_px <= 0.0 {
        return 1;
    }
    let taste = taste_px.max(1.0);
    ((weg_px / taste).round() as u32).clamp(1, max.max(1))
}

/// Aus (Tonhöhe, Dauer in Schritten) die Schrittliste: jeder Ton belegt
/// `dauer` aufeinanderfolgende Schritte, Gesamtlänge = Summe.
///
/// Das ist die Aufnahme-Regel von Maske 2 (Nutzerentscheidung 2026-10-22):
/// die Wertigkeit eines Tastendrucks ist eine **Anzahl gleicher Schritte**,
/// keine Normalisierung auf die Anzahl der Drücke. 8, 4, 6 hintereinander
/// ergeben also 8 + 4 + 6 = 18 Schritte.
///
/// `dauer == 0` ist die Pause und belegt keinen Schritt. Die Schrittnummern
/// im Ergebnis sind lückenlos von 0 an.
pub fn schritte_aus_dauern(tasten: &[(i32, u32)]) -> Vec<(u32, i32)> {
    let mut raus = Vec::new();
    let mut schritt: u32 = 0;
    for &(ton, dauer) in tasten {
        for _ in 0..dauer {
            raus.push((schritt, ton));
            schritt += 1;
        }
    }
    raus
}

/// Die neuen Tastendrücke proportional auf eine Bereichszeit von `ziel`
/// Schritten legen: die Summe der neuen Dauern bestimmt die Proportionen,
/// jeder Ton bekommt mindestens einen Schritt.
///
/// Beispiel: 8/4/6 (Summe 18) auf einen Bereich von 8 Schritten ergibt
/// 3/2/3. Ist der Bereich kürzer als die Zahl der Töne, bekommen die ersten
/// Töne je einen Schritt — mehr geht nicht, ohne die Mindestdauer zu brechen.
///
/// Gerundet wird nach dem größten Rest, damit die Summe **exakt** `ziel`
/// trifft; sonst würde der Verlauf an der Bereichsgrenze verrutschen.
pub fn dauern_auf_bereich(tasten: &[(i32, u32)], ziel: u32) -> Vec<(i32, u32)> {
    let aktive: Vec<(i32, u32)> = tasten
        .iter()
        .copied()
        .filter(|(_, dauer)| *dauer > 0)
        .collect();
    if aktive.is_empty() || ziel == 0 {
        return Vec::new();
    }
    let anzahl = aktive.len() as u32;
    if ziel <= anzahl {
        // Zu wenig Platz fuer alle: die ersten Töne bekommen den einen
        // möglichen Schritt, der Rest fällt weg.
        return aktive
            .iter()
            .take(ziel as usize)
            .map(|&(ton, _)| (ton, 1))
            .collect();
    }
    let summe: u64 = aktive.iter().map(|(_, dauer)| *dauer as u64).sum();
    // Ein Schritt je Ton ist gesetzt; der Rest wird nach den Dauern verteilt.
    let rest = ziel - anzahl;
    let mut extras: Vec<u32> = aktive
        .iter()
        .map(|(_, dauer)| (rest as u64 * *dauer as u64 / summe) as u32)
        .collect();
    let vergeben: u32 = extras.iter().sum();
    let mut uebrig = rest - vergeben;
    // Groesste Reste zuerst; bei Gleichstand der frueheste Ton (stabil).
    let mut reste: Vec<(usize, u64)> = aktive
        .iter()
        .enumerate()
        .map(|(i, (_, dauer))| (i, (rest as u64 * *dauer as u64) % summe))
        .collect();
    reste.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut k = 0;
    while uebrig > 0 {
        extras[reste[k % reste.len()].0] += 1;
        uebrig -= 1;
        k += 1;
    }
    aktive
        .iter()
        .zip(extras)
        .map(|(&(ton, _), extra)| (ton, 1 + extra))
        .collect()
}

/// Gemessene Tippabstaende in Schritte umrechnen:
/// `schritte = (abstand_ms / step_ms).round()`.
///
/// Unbrauchbare Werte (0, negativ, NaN/∞) belegen keinen Schritt. Die
/// Mindestdauer von 1 Schritt gilt erst im Tippzeiten-Modus
/// ([`tippzeiten_als_dauern`]); hier bleibt `0 ms -> 0`, damit eine Pause
/// wirklich eine Pause ist.
pub fn schritte_aus_abstaenden(abstaende_ms: &[f64], step_ms: f64) -> Vec<u32> {
    if !step_ms.is_finite() || step_ms <= 0.0 {
        return vec![0; abstaende_ms.len()];
    }
    abstaende_ms
        .iter()
        .map(|ms| {
            if !ms.is_finite() || *ms <= 0.0 {
                0
            } else {
                // `as u32` sättigt; ein sehr langer Abstand wird nicht negativ.
                (ms / step_ms).round().max(0.0) as u32
            }
        })
        .collect()
}

/// Tippzeiten-Modus: die gemappten Abstaende sind die Blocklaengen, jeder
/// Block mindestens ein Schritt (auch der erste Druck mit Abstand 0).
pub fn tippzeiten_als_dauern(abstaende_ms: &[f64], step_ms: f64) -> Vec<u32> {
    schritte_aus_abstaenden(abstaende_ms, step_ms)
        .into_iter()
        .map(|schritte| schritte.max(1))
        .collect()
}

/// Verlauf aus Blockdauern und den Pausen **vor** jedem Block.
///
/// Die Pausen belegen leere Schritte: sie stehen nicht als Eintrag im
/// Ergebnis, sondern als Lücke in der Schrittnummer. `pausen[i]` gehört zu
/// `dauern[i]`; fehlende Angaben sind 0. Ein Block mit Dauer 0 entfällt samt
/// seiner Pause (es gibt keinen Block, vor dem sie stehen könnte).
pub fn verlauf_mit_pausen(dauern: &[(i32, u32)], pausen: &[u32]) -> Vec<(u32, i32)> {
    let mut raus = Vec::new();
    let mut schritt: u32 = 0;
    for (i, &(ton, dauer)) in dauern.iter().enumerate() {
        if dauer == 0 {
            continue;
        }
        schritt = schritt.saturating_add(pausen.get(i).copied().unwrap_or(0));
        for _ in 0..dauer {
            raus.push((schritt, ton));
            schritt += 1;
        }
    }
    raus
}

/// Gesamtlaenge eines Verlaufs in Schritten — inklusive der Lücken, die
/// Pausen hinterlassen. Ein kompakter Verlauf hat `len()` Schritte; mit
/// Pausen ist die Laenge groesser als die Zahl der klingenden Einträge.
pub fn verlauf_laenge(verlauf: &[(u32, i32)]) -> u32 {
    verlauf
        .iter()
        .map(|(schritt, _)| schritt.saturating_add(1))
        .max()
        .unwrap_or(0)
}

/// Blockdauern und Pausen einer neuen Aufnahme proportional auf eine
/// Bereichszeit von `ziel` Schritten legen.
///
/// Anders als [`dauern_auf_bereich`] zaehlen hier auch die Pausen als
/// Zeitanteil. Jeder Block bekommt mindestens einen Schritt; der Rest wird
/// nach dem groessten Rest auf **alle** Segmente (Bloecke und Pausen)
/// verteilt, damit die Summe exakt `ziel` trifft.
///
/// Rueckgabe: `(blockdauern, pausen)` mit gleicher Laenge; die Pausen stehen
/// wieder vor ihrem Block.
pub fn struktur_auf_bereich(
    dauern: &[(i32, u32)],
    pausen: &[u32],
    ziel: u32,
) -> (Vec<(i32, u32)>, Vec<u32>) {
    // Nur echte Bloecke; ihre Pause gehoert dazu.
    let aktiv: Vec<(i32, u32, u32)> = dauern
        .iter()
        .enumerate()
        .filter(|(_, (_, dauer))| *dauer > 0)
        .map(|(i, &(ton, dauer))| (ton, dauer, pausen.get(i).copied().unwrap_or(0)))
        .collect();
    if aktiv.is_empty() || ziel == 0 {
        return (Vec::new(), Vec::new());
    }
    let anzahl = aktiv.len() as u32;
    if ziel <= anzahl {
        // Zu wenig Platz: die ersten Bloecke bekommen je einen Schritt.
        return (
            aktiv
                .iter()
                .take(ziel as usize)
                .map(|&(ton, _, _)| (ton, 1))
                .collect(),
            vec![0; ziel as usize],
        );
    }
    let rest = ziel - anzahl;
    // Segmente in Reihenfolge: Block, Pause, Block, Pause … Nur Segmente mit
    // Gewicht zaehlen; die Basis 1 je Block ist bereits abgezogen.
    enum Art {
        Block(usize),
        Pause(usize),
    }
    let mut segmente: Vec<(Art, u64)> = Vec::new();
    for (k, &(_, dauer, pause)) in aktiv.iter().enumerate() {
        segmente.push((Art::Block(k), u64::from(dauer)));
        if pause > 0 {
            segmente.push((Art::Pause(k), u64::from(pause)));
        }
    }
    let summe: u64 = segmente.iter().map(|(_, gewicht)| *gewicht).sum();
    let mut extra = vec![0u32; segmente.len()];
    for (j, (_, gewicht)) in segmente.iter().enumerate() {
        extra[j] = (u64::from(rest) * *gewicht / summe) as u32;
    }
    let vergeben: u32 = extra.iter().sum();
    let mut uebrig = rest - vergeben;
    let mut reste: Vec<(usize, u64)> = segmente
        .iter()
        .enumerate()
        .map(|(j, (_, gewicht))| (j, (u64::from(rest) * *gewicht) % summe))
        .collect();
    reste.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut k = 0;
    while uebrig > 0 {
        extra[reste[k % reste.len()].0] += 1;
        uebrig -= 1;
        k += 1;
    }
    let mut block_extra = vec![0u32; aktiv.len()];
    let mut pause_neu = vec![0u32; aktiv.len()];
    for (j, (art, _)) in segmente.iter().enumerate() {
        match art {
            Art::Block(k) => block_extra[*k] = extra[j],
            Art::Pause(k) => pause_neu[*k] = extra[j],
        }
    }
    (
        aktiv
            .iter()
            .enumerate()
            .map(|(k, &(ton, _, _))| (ton, 1 + block_extra[k]))
            .collect(),
        pause_neu,
    )
}

/// Ergebnis von [`bereich_ersetzen`] fuer Status und Log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BereichErsetzt {
    /// Laenge des Bereichs in Schritten (und damit die Zahl der neuen
    /// Schritte, denn die Bereichszeit wird exakt gefuellt).
    pub schritte: u32,
    /// Summe der neuen Dauern (die „Gesamtzeit“ der neuen Aufnahme), inklusive
    /// der Pausen.
    pub dauer: u32,
    /// Gesamtlaenge des Verlaufs danach (inklusive Pausen-Luecken).
    pub gesamt: u32,
}

/// Einen lokalen Verlauf `[0, bereich)` in den Bereich `[von, bis)` setzen.
/// Ausserhalb bleibt alles stehen; die Schrittnummern sind bereits absolut
/// und bleiben es (die Bereichslaenge aendert sich nicht).
fn ersetze_bereich(
    verlauf: &mut Vec<(u32, i32)>,
    von: u32,
    bis: u32,
    lokal: &[(u32, i32)],
    dauer: u32,
) -> BereichErsetzt {
    verlauf.retain(|(schritt, _)| *schritt < von || *schritt >= bis);
    verlauf.extend(lokal.iter().map(|(schritt, ton)| (von + *schritt, *ton)));
    verlauf.sort_by_key(|(schritt, _)| *schritt);
    BereichErsetzt {
        schritte: bis - von,
        dauer,
        gesamt: verlauf_laenge(verlauf),
    }
}

/// Den Bereich `[von, bis)` im Verlauf durch eine neue M2-Aufnahme ersetzen.
///
/// Die neue Gesamtzeit ([`dauern_auf_bereich`]) wird auf die Bereichszeit
/// gemappt; außerhalb des Bereichs bleibt der Verlauf unangetastet.
///
/// `None`, wenn der Bereich leer/ungueltig ist oder die neue Aufnahme keine
/// Schritte hat — dann darf ein versehentlicher Tastendruck nichts löschen.
pub fn bereich_ersetzen(
    verlauf: &mut Vec<(u32, i32)>,
    von: u32,
    bis: u32,
    tasten: &[(i32, u32)],
) -> Option<BereichErsetzt> {
    let laenge = verlauf_laenge(verlauf);
    let von = von.min(laenge);
    let bis = bis.min(laenge);
    if von >= bis {
        return None;
    }
    let dauer: u32 = tasten.iter().map(|(_, d)| *d).sum();
    if dauer == 0 {
        return None;
    }
    let gemappt = dauern_auf_bereich(tasten, bis - von);
    let lokal = schritte_aus_dauern(&gemappt);
    Some(ersetze_bereich(verlauf, von, bis, &lokal, dauer))
}

/// Wie [`bereich_ersetzen`], aber mit Pausen vor den Blocken.
///
/// Die neue Aufnahme wird mit ihrer **gesamten** Zeit (Bloecke + Pausen) auf
/// die Bereichszeit gemappt; die Pausen bleiben als Luecken erhalten.
pub fn bereich_ersetzen_mit_pausen(
    verlauf: &mut Vec<(u32, i32)>,
    von: u32,
    bis: u32,
    dauern: &[(i32, u32)],
    pausen: &[u32],
) -> Option<BereichErsetzt> {
    let laenge = verlauf_laenge(verlauf);
    let von = von.min(laenge);
    let bis = bis.min(laenge);
    if von >= bis {
        return None;
    }
    let dauer: u32 =
        dauern.iter().map(|(_, d)| *d).sum::<u32>() + pausen.iter().take(dauern.len()).sum::<u32>();
    if dauer == 0 {
        return None;
    }
    let (dauern_neu, pausen_neu) = struktur_auf_bereich(dauern, pausen, bis - von);
    let lokal = verlauf_mit_pausen(&dauern_neu, &pausen_neu);
    Some(ersetze_bereich(verlauf, von, bis, &lokal, dauer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_verteilt_gleichmaessig() {
        let p = schritt_positionen(4, Verteilung::Linear);
        assert_eq!(p.len(), 3, "drei Zwischenschritte bei vier Schritten");
        assert!((p[0] - 0.25).abs() < 1e-6);
        assert!((p[1] - 0.50).abs() < 1e-6);
        assert!((p[2] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn beschleunigend_macht_die_schritte_laenger() {
        let p = schritt_positionen(4, Verteilung::Beschleunigend);
        let linear = schritt_positionen(4, Verteilung::Linear);
        assert!(p[0] < linear[0], "erster Zwischenschritt kommt früher");
        assert!(p.windows(2).all(|w| w[1] > w[0]), "monoton");
        // Abstände zwischen 0, den Zwischenschritten und 1 wachsen.
        let mut punkte = vec![0.0];
        punkte.extend(p);
        punkte.push(1.0);
        let abstaende: Vec<f32> = punkte.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(
            abstaende.windows(2).all(|w| w[1] > w[0]),
            "Schritte werden länger: {abstaende:?}"
        );
    }

    #[test]
    fn bremsend_ist_das_spiegelbild() {
        let p = schritt_positionen(4, Verteilung::Bremsend);
        let b = schritt_positionen(4, Verteilung::Beschleunigend);
        for i in 0..p.len() {
            assert!(
                (p[i] + b[b.len() - 1 - i] - 1.0).abs() < 1e-6,
                "Bremsend spiegelt Beschleunigend: {} {}",
                p[i],
                b[b.len() - 1 - i]
            );
        }
    }

    #[test]
    fn randfaelle_der_positionen() {
        assert!(schritt_positionen(1, Verteilung::Linear).is_empty());
        assert_eq!(schritt_positionen(2, Verteilung::Linear).len(), 1);
        let p = schritt_positionen(3, Verteilung::Linear);
        assert!(p.iter().all(|t| (0.0..1.0).contains(t)));
    }

    #[test]
    fn profil_erkennt_die_wischart() {
        assert_eq!(
            profil_aus_geschwindigkeiten(&[1.0, 1.0, 1.0, 1.0]),
            Verteilung::Linear
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[1.0, 1.0, 2.0, 2.0]),
            Verteilung::Beschleunigend
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[2.0, 2.0, 1.0, 1.0]),
            Verteilung::Bremsend
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[1.0, 1.1, 1.0, 1.05]),
            Verteilung::Linear,
            "kleine Schwankungen bleiben linear"
        );
        assert_eq!(
            profil_aus_geschwindigkeiten(&[]),
            Verteilung::Linear,
            "zu wenig Proben"
        );
    }

    #[test]
    fn schritte_aus_der_wischlaenge() {
        assert_eq!(schritte_aus_wisch(300.0, 60.0, 32), 5);
        assert_eq!(schritte_aus_wisch(10.0, 60.0, 32), 1, "mindestens einer");
        assert_eq!(schritte_aus_wisch(10_000.0, 60.0, 32), 32, "gedeckelt");
        assert_eq!(schritte_aus_wisch(0.0, 60.0, 32), 1);
        assert_eq!(schritte_aus_wisch(f32::NAN, 60.0, 32), 1);
    }

    #[test]
    fn ziffer_trifft_position() {
        // Nutzerwunsch 2026-10-14: der USB-Nummernblock waehlt die **Position**
        // im Tastenfeld, nicht den Wert: Ziffer 1 = erste Taste, Ziffer 0 =
        // zehnte.
        assert_eq!(position_linear(1, 10), Some(0));
        assert_eq!(position_linear(5, 10), Some(4));
        assert_eq!(position_linear(9, 10), Some(8));
        assert_eq!(position_linear(0, 10), Some(9), "die Null ist die zehnte");
        assert_eq!(position_linear(5, 4), None, "das Feld hat nur vier Tasten");
        assert_eq!(position_linear(4, 4), Some(3), "die letzte passt noch");
    }

    #[test]
    fn ziffer_trifft_den_raeumlich_gleichen_knopf() {
        // Maske 2: Feld in Nummernblock-Anordnung (7-8-9 / 4-5-6 / 1-2-3 / 0).
        assert_eq!(position_numpad(7), Some(0), "oben links");
        assert_eq!(position_numpad(9), Some(2), "oben rechts");
        assert_eq!(position_numpad(4), Some(3), "mitte links");
        assert_eq!(position_numpad(5), Some(4), "Mitte");
        assert_eq!(position_numpad(1), Some(6), "unten links");
        assert_eq!(position_numpad(0), Some(9), "die Null ganz unten");
    }

    #[test]
    fn dauern_haengen_sich_aneinander() {
        // Nutzerbefund 2026-10-22: die Wertigkeit ist die **Dauer** eines
        // Tons, nicht ein Anteil an der Gesamtzahl der Druecke. 8 + 4 + 6
        // ergibt 18 Schritte, nicht 3 oder 9.
        let schritte = schritte_aus_dauern(&[(60, 8), (62, 4), (64, 6)]);
        assert_eq!(schritte.len(), 18, "8+4+6");
        assert_eq!(
            schritte.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            (0..18).collect::<Vec<_>>(),
            "lueckenlose Schrittnummern"
        );
        assert!(
            schritte[..8].iter().all(|(_, ton)| *ton == 60),
            "0-7: Ton 60"
        );
        assert!(
            schritte[8..12].iter().all(|(_, ton)| *ton == 62),
            "8-11: Ton 62"
        );
        assert!(
            schritte[12..].iter().all(|(_, ton)| *ton == 64),
            "12-17: Ton 64"
        );
    }

    #[test]
    fn dauer_null_und_leere_folge() {
        // Die Null ist die Pause: sie belegt keinen Schritt.
        assert!(schritte_aus_dauern(&[]).is_empty());
        let schritte = schritte_aus_dauern(&[(60, 0), (62, 2), (64, 0)]);
        assert_eq!(schritte.len(), 2, "nur der Ton mit Dauer");
        assert!(schritte.iter().all(|(_, ton)| *ton == 62));
        assert_eq!(schritte[0].0, 0, "Schrittnummern bleiben lueckenlos");
        assert_eq!(schritte[1].0, 1);
    }

    #[test]
    fn bereichszeit_mappt_die_neue_gesamtdauer() {
        // 18 Schritte neue Aufnahme auf einen 8-Schritte-Bereich: die
        // Proportionen bleiben, jeder Ton bekommt mindestens einen Schritt.
        let gemappt = dauern_auf_bereich(&[(60, 8), (62, 4), (64, 6)], 8);
        assert_eq!(gemappt, vec![(60, 3), (62, 2), (64, 3)]);
        assert_eq!(
            gemappt.iter().map(|(_, d)| *d).sum::<u32>(),
            8,
            "Bereich genau gefuellt"
        );
        // 12 Schritte: 8/4/6 -> 5/3/4, ebenfalls exakt.
        let gemappt = dauern_auf_bereich(&[(60, 8), (62, 4), (64, 6)], 12);
        assert_eq!(gemappt, vec![(60, 5), (62, 3), (64, 4)]);
    }

    #[test]
    fn bereich_mindestens_ein_schritt_je_ton() {
        // Auch bei kleinem Bereich bekommt jeder Ton einen Schritt; ist der
        // Bereich kleiner als die Toene, bleiben die ersten uebrig.
        let gemappt = dauern_auf_bereich(&[(60, 8), (62, 4), (64, 6)], 3);
        assert_eq!(gemappt, vec![(60, 1), (62, 1), (64, 1)]);
        let gemappt = dauern_auf_bereich(&[(60, 8), (62, 4), (64, 6)], 2);
        assert_eq!(gemappt, vec![(60, 1), (62, 1)]);
        assert!(dauern_auf_bereich(&[], 8).is_empty());
        assert!(dauern_auf_bereich(&[(60, 8)], 0).is_empty());
    }

    #[test]
    fn bereich_ersetzen_laesst_draussen_alles_stehen() {
        // Maske 1 waehlt 4..12; in M2 kommen 18 Schritte neu. Der Bereich
        // wird auf seine 8 Schritte gemappt, davor und danach bleibt alles.
        let alt: Vec<(u32, i32)> = (0..20).map(|i| (i, 40 + i as i32)).collect();
        let mut verlauf = alt.clone();
        let meldung = bereich_ersetzen(&mut verlauf, 4, 12, &[(60, 8), (62, 4), (64, 6)])
            .expect("Bereich ist gueltig");
        assert_eq!(meldung.schritte, 8, "Bereichslaenge");
        assert_eq!(meldung.dauer, 18, "Summe der neuen Dauern");
        assert_eq!(meldung.gesamt, 20, "Gesamtlaenge bleibt");
        assert_eq!(&verlauf[..4], &alt[..4], "davor unveraendert");
        assert_eq!(&verlauf[12..], &alt[12..], "danach unveraendert");
        assert_eq!(
            &verlauf[4..12],
            &[
                (4, 60),
                (5, 60),
                (6, 60),
                (7, 62),
                (8, 62),
                (9, 64),
                (10, 64),
                (11, 64)
            ],
            "der neue Bereich"
        );
        assert!(
            verlauf.iter().enumerate().all(|(i, (s, _))| *s == i as u32),
            "Schrittnummern neu durchnummeriert"
        );
    }

    #[test]
    fn bereich_ersetzen_ohne_neue_toene_aendert_nichts() {
        let mut verlauf: Vec<(u32, i32)> = (0..8).map(|i| (i, 60)).collect();
        let alt = verlauf.clone();
        assert!(
            bereich_ersetzen(&mut verlauf, 2, 5, &[(60, 0)]).is_none(),
            "ohne Dauer gibt es nichts zu ersetzen"
        );
        assert_eq!(verlauf, alt);
        assert!(bereich_ersetzen(&mut verlauf, 5, 5, &[(60, 2)]).is_none());
    }

    #[test]
    fn abstaende_werden_zu_schritten() {
        // 240 ms bei 125 ms je Schritt sind genau zwei Schritte.
        assert_eq!(schritte_aus_abstaenden(&[240.0], 125.0), vec![2]);
        // 187 ms/125 = 1,496 -> 1; 188 ms/125 = 1,504 -> 2.
        assert_eq!(
            schritte_aus_abstaenden(&[187.0, 188.0], 125.0),
            vec![1, 2],
            "kaufmaennisch runden"
        );
        assert_eq!(
            schritte_aus_abstaenden(&[62.0], 125.0),
            vec![0],
            "unter einem halben Schritt"
        );
        assert_eq!(
            schritte_aus_abstaenden(&[0.0, -10.0, f64::NAN, f64::INFINITY], 125.0),
            vec![0, 0, 0, 0],
            "unbrauchbare Abstaende belegen keinen Schritt"
        );
        assert_eq!(
            schritte_aus_abstaenden(&[240.0], 0.0),
            vec![0],
            "ohne Schrittlaenge keine Rechnung"
        );
        assert!(schritte_aus_abstaenden(&[], 125.0).is_empty());
    }

    #[test]
    fn tippzeiten_werden_mindestens_ein_schritt() {
        // Tippzeiten-Modus: der gemappte Abstand ist die Blocklaenge, auch
        // beim ersten Druck (Abstand 0) mindestens ein Schritt.
        assert_eq!(
            tippzeiten_als_dauern(&[0.0, 240.0, 360.0], 120.0),
            vec![1, 2, 3]
        );
        assert_eq!(tippzeiten_als_dauern(&[0.0], 120.0), vec![1]);
        assert!(tippzeiten_als_dauern(&[], 120.0).is_empty());
    }

    #[test]
    fn pausen_stehen_vor_ihrem_block() {
        // 8/4/6 mit 2 und 3 Schritten Pause vor dem zweiten/dritten Block:
        // 18 Block-Schritte plus 5 Pausen = 23 Schritte.
        let verlauf = verlauf_mit_pausen(&[(60, 8), (62, 4), (64, 6)], &[0, 2, 3]);
        assert_eq!(verlauf.len(), 18, "18 klingende Schritte");
        assert_eq!(verlauf_laenge(&verlauf), 23, "inklusive Pausen");
        assert_eq!(verlauf[8].0, 10, "zweiter Block nach 2 Pausen");
        assert_eq!(verlauf[12].0, 17, "dritter Block nach weiteren 3 Pausen");
        assert!(!verlauf.iter().any(|(s, _)| (8..10).contains(s)));
        assert!(!verlauf.iter().any(|(s, _)| (14..17).contains(s)));
        // Fehlende Pausenangaben sind 0, ein Block mit Dauer 0 entfaellt
        // samt seiner Pause.
        let knapp = verlauf_mit_pausen(&[(60, 2), (62, 2)], &[0]);
        assert_eq!(verlauf_laenge(&knapp), 4);
        assert_eq!(
            verlauf_mit_pausen(&[(60, 0), (62, 2)], &[3, 0]),
            vec![(0, 62), (1, 62)]
        );
        assert_eq!(verlauf_laenge(&[]), 0);
    }

    #[test]
    fn bereich_mappt_auch_pausen() {
        // 8/4/6 mit 2+3 Pausen (23 Schritte) auf 8 Bereichsschritte: die
        // Bloecke schrumpfen auf 3/2/2, eine Pause bleibt als Luecke.
        let (dauern, pausen) = struktur_auf_bereich(&[(60, 8), (62, 4), (64, 6)], &[0, 2, 3], 8);
        assert_eq!(dauern, vec![(60, 3), (62, 2), (64, 2)]);
        assert_eq!(pausen, vec![0, 0, 1]);
        assert_eq!(
            dauern.iter().map(|(_, d)| *d).sum::<u32>() + pausen.iter().sum::<u32>(),
            8,
            "Bereich exakt gefuellt"
        );
        // Auf die volle Laenge gemappt kommt die Aufnahme unveraendert zurueck.
        let (dauern, pausen) = struktur_auf_bereich(&[(60, 8), (62, 4), (64, 6)], &[0, 2, 3], 23);
        assert_eq!(dauern, vec![(60, 8), (62, 4), (64, 6)]);
        assert_eq!(pausen, vec![0, 2, 3]);
    }

    #[test]
    fn bereich_ersetzen_mit_pausen_haelt_die_luecke() {
        let alt: Vec<(u32, i32)> = (0..20).map(|i| (i, 40 + i as i32)).collect();
        let mut verlauf = alt.clone();
        let meldung = bereich_ersetzen_mit_pausen(
            &mut verlauf,
            4,
            12,
            &[(60, 8), (62, 4), (64, 6)],
            &[0, 2, 3],
        )
        .expect("Bereich ist gueltig");
        assert_eq!(meldung.schritte, 8);
        assert_eq!(meldung.dauer, 23, "Pausen zaehlen zur neuen Gesamtzeit");
        assert_eq!(meldung.gesamt, 20, "Gesamtlaenge bleibt");
        assert_eq!(&verlauf[..4], &alt[..4], "davor unveraendert");
        assert_eq!(&verlauf[11..], &alt[12..], "danach unveraendert");
        assert_eq!(
            &verlauf[4..11],
            &[
                (4, 60),
                (5, 60),
                (6, 60),
                (7, 62),
                (8, 62),
                (10, 64),
                (11, 64)
            ],
            "gemappter Bereich mit Pause bei Schritt 9"
        );
        assert!(
            !verlauf.iter().any(|(s, _)| *s == 9),
            "die Pause ist eine Luecke"
        );
        assert_eq!(verlauf_laenge(&verlauf), 20);
    }
}
