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

// ---------------------------------------------------------------------------
// M2-Aufnahme: notes -> notes3 -> notes4
// ---------------------------------------------------------------------------
//
// Die Vorlage (`pattern_recorder/src/main.rs`, `record()` und `scale()`) macht
// aus den Tastendruecken zuerst `notes` (Wert + Zeitstempel), fuellt dann in
// `notes3` gleiche Zwischenschritte ein und legt `notes4` auf die Taktzeit.
// Hier steht dieselbe Kette ohne Bildschirm und ohne Audio.
//
// Der wichtige Unterschied zur C++-Fassung: die **0 ist keine Endtaste mehr**,
// sondern eine Pause. Das Ende kommt von aussen (`ende_s`, in der App der
// „Ende“-Knopf/Enter). Eine Pause belegt keine klingenden Schritte, zaehlt
// aber in der Schrittnummer weiter (Luecke im Verlauf).

/// Eine Rohaufnahme: (Wert, Zeitpunkt in Sekunden). Der Wert ist die Zahl
/// gleicher Schritte, die dieser Druck lang gilt; `0` ist die Pause.
pub type AufnahmeWert = (i32, f64);

/// Welche Randzeitstempel in die Schrittweiten-Rechnung eingehen.
///
/// Das sind die vier Pfeiltasten der Vorlage (K + Pfeile): links nimmt nur
/// den linken Rand, oben beide Raender, unten keinen Rand (nur die Mitte),
/// rechts nur den rechten Rand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandModus {
    /// Nur der linke (fruehe) Rand.
    Links,
    /// Linker und rechter Rand („oben“ = beide Kanten).
    Oben,
    /// Weder linker noch rechter Rand (nur die Mitte).
    Unten,
    /// Nur der rechte (spaete) Rand.
    Rechts,
}

impl RandModus {
    /// Kurzname fuer Knopf und Log.
    pub fn name(self) -> &'static str {
        match self {
            RandModus::Links => "Links",
            RandModus::Oben => "Oben",
            RandModus::Unten => "Unten",
            RandModus::Rechts => "Rechts",
        }
    }
}

/// Einstellungen der M2-Auswertung.
///
/// `step_min_ms`/`step_max_ms` sind die **von/bis**-Laengen der
/// Zwischenschritte in Millisekunden. Beim normalen Aufnehmen (`rampe ==
/// false`) wird jeder Abschnitt exakt in `Wert` gleiche Schritte zerlegt; die
/// Grenzen wirken dann nur als Anzeige/Klammer. Mit `rampe == true` laufen
/// die Schrittlaengen innerhalb eines Abschnitts von `step_min_ms` (links)
/// nach `step_max_ms` (rechts), wie `scaleToRangenum` in der Vorlage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AufnahmeOptionen {
    /// Linke/kurze Zwischenschrittlaenge in ms („von“).
    pub step_min_ms: f64,
    /// Rechte/lange Zwischenschrittlaenge in ms („bis“).
    pub step_max_ms: f64,
    /// Welche Randzeitstempel zaehlen.
    pub rand_modus: RandModus,
    /// Anteil der Gesamtdauer, der als Randzone gilt (z. B. 1/8).
    pub rand_anteil: f64,
    /// Aus Min/Max die Haelfte als Fader-Startwert nehmen (Default true).
    pub haelfte_aus_minmax: bool,
    /// Fader zwischen min und max (0..1); 0.5 ist die Haelfte.
    pub fader: f64,
    /// Rampe von `step_min_ms` nach `step_max_ms` statt gleicher Schritte.
    pub rampe: bool,
}

impl Default for AufnahmeOptionen {
    fn default() -> Self {
        Self {
            // 0 heisst „noch nicht gesetzt“: dann gilt die exakte Laenge aus
            // der Aufnahme (Abschnittsdauer / Wert). Die App fuellt die
            // Grenzen nach der ersten Aufnahme mit dem gemessenen Min/Max.
            step_min_ms: 0.0,
            step_max_ms: 0.0,
            rand_modus: RandModus::Oben,
            rand_anteil: 1.0 / 8.0,
            haelfte_aus_minmax: true,
            fader: 0.5,
            rampe: false,
        }
    }
}

/// Ein Eintrag der notes3-Kette: Zeitpunkt und Wert. `wert == 0` ist die
/// Pause; sie steht als Marke in der Liste, damit die Taktzeit-Rechnung die
/// Dauer bis zum naechsten Eintrag kennt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AufnahmeNote {
    pub zeit_s: f64,
    pub wert: i32,
}

/// Die Randzone `[von, bis]` einer Aufnahme der Laenge `ende_s`.
///
/// `rand_anteil = 1/8` und `ende_s = 3.0` ergibt `(2.625, 3.0)` — genau den
/// Bereich, den der Nutzer fuer die letzten beiden Zeitstempel nennt.
pub fn randzone(ende_s: f64, rand_anteil: f64) -> (f64, f64) {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let anteil = if rand_anteil.is_finite() {
        rand_anteil.clamp(0.0, 0.5)
    } else {
        1.0 / 8.0
    };
    (ende * (1.0 - anteil), ende)
}

/// Fader-Wert zwischen min und max in ms.
///
/// `haelfte_aus_minmax` ist der Default: dann ist der Startwert genau die
/// Haelfte (Nutzer: „Vom Min–Max die Hälfte nehmen, später mit Fader
/// einstellen“). Sobald der Fader bewegt wird, schaltet die App das Flag aus
/// und uebergibt den echten Faderwert.
pub fn fader_schritt_ms(min_ms: f64, max_ms: f64, optionen: &AufnahmeOptionen) -> f64 {
    let (min, max) = if min_ms <= max_ms {
        (min_ms, max_ms)
    } else {
        (max_ms, min_ms)
    };
    if !min.is_finite() || !max.is_finite() || max <= 0.0 {
        return 0.0;
    }
    let f = if optionen.haelfte_aus_minmax {
        0.5
    } else {
        optionen.fader.clamp(0.0, 1.0)
    };
    min + f * (max - min)
}

/// Die Schrittlaengen am linken/rechten Rand einer Zeitpunktfolge.
///
/// Die Folge muss den Anfang und das Ende enthalten (wie `notes3` inkl.
/// Endmarke). Zurueck kommen `(min_ms, max_ms)` der ausgewaehlten
/// Randintervalle:
/// - `Links`: nur das erste Intervall,
/// - `Rechts`: nur das letzte Intervall,
/// - `Oben`: erstes und letztes,
/// - `Unten`: nur die mittleren Intervalle (ohne beide Raender).
///
/// `rand_anteil` prueft, ob der letzte Zeitstempel wirklich in der rechten
/// Randzone liegt; sonst bleibt das tatsaechliche letzte Intervall stehen.
pub fn rand_schritt_ms(
    zeiten: &[f64],
    ende_s: f64,
    modus: RandModus,
    rand_anteil: f64,
) -> Option<(f64, f64)> {
    let mut z: Vec<f64> = zeiten
        .iter()
        .copied()
        .filter(|t| t.is_finite() && *t >= 0.0 && *t <= ende_s.max(0.0))
        .collect();
    z.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    z.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if z.len() < 2 {
        return None;
    }
    let erste = (z[1] - z[0]) * 1000.0;
    let mut letzte = (z[z.len() - 1] - z[z.len() - 2]) * 1000.0;
    let (zone_von, _) = randzone(ende_s, rand_anteil);
    if z[z.len() - 1] + 1e-9 < zone_von {
        // Der letzte Zeitstempel liegt nicht in der gewaehlten Randzone:
        // dann ist die Aufnahme kuerzer als die Randlaenge. Das tatsaechliche
        // letzte Intervall ist trotzdem die beste Schaetzung.
        letzte = letzte.max(0.0);
    }
    let mittel: Vec<f64> = z
        .windows(2)
        .skip(1)
        .take(z.len().saturating_sub(3))
        .map(|w| (w[1] - w[0]) * 1000.0)
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .collect();
    let (min, max) = match modus {
        RandModus::Links => (erste, erste),
        RandModus::Rechts => (letzte, letzte),
        RandModus::Oben => (erste.min(letzte), erste.max(letzte)),
        RandModus::Unten => {
            if mittel.is_empty() {
                return None;
            }
            let min = mittel.iter().copied().fold(f64::INFINITY, f64::min);
            let max = mittel.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (min, max)
        }
    };
    if !min.is_finite() || !max.is_finite() || max <= 0.0 {
        return None;
    }
    Some((min, max))
}

/// Die Schrittlaengen (ms) einer Rohaufnahme, **ohne** Pausen.
///
/// Fuer jeden klingenden Abschnitt entstehen `Wert` Laengen: bei gleichen
/// Schritten `(t1-t0)/Wert`, mit Rampe die Intervalle aus
/// [`rampe_zeitpunkte`] (inklusive des letzten Intervalls bis zum
/// Abschnittsende). Pausen tragen nichts bei — dadurch ist die letzte Laenge
/// wirklich die letzte **klingende** Schrittlaenge und nicht die Wartezeit
/// bis zum „Ende“.
pub fn schrittlaengen_aus_aufnahme(
    werte: &[AufnahmeWert],
    ende_s: f64,
    optionen: &AufnahmeOptionen,
) -> Vec<f64> {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let liste = aufnahme_sortiert(werte, ende);
    let mut raus = Vec::new();
    for (i, &(wert, t0)) in liste.iter().enumerate() {
        if wert <= 0 {
            continue;
        }
        let t1 = liste.get(i + 1).map(|(_, t)| *t).unwrap_or(ende);
        if t1 <= t0 {
            continue;
        }
        let v = wert as u32;
        if optionen.rampe && optionen.step_min_ms > 0.0 && optionen.step_max_ms > 0.0 {
            let zeiten = rampe_zeitpunkte(t0, t1, v, optionen.step_min_ms, optionen.step_max_ms);
            for paar in zeiten.windows(2) {
                raus.push((paar[1] - paar[0]) * 1000.0);
            }
        } else {
            let laenge = (t1 - t0) * 1000.0 / v as f64;
            raus.extend(std::iter::repeat_n(laenge, v as usize));
        }
    }
    raus
}

/// Randauswahl aus einer Liste von Schrittlaengen (ms).
///
/// Anders als [`rand_schritt_ms`] (das eine flache Zeitpunktfolge inkl.
/// Pausen sieht) bekommt diese Funktion nur die klingenden Laengen. Damit
/// sind „erste zwei“ und „letzte zwei“ Zeitstempel genau die erste und die
/// letzte Schrittlaenge — auch wenn hinter der letzten Note noch eine Pause
/// bis zum „Ende“ steht.
pub fn rand_schritt_ms_aus_laengen(laengen: &[f64], modus: RandModus) -> Option<(f64, f64)> {
    let l: Vec<f64> = laengen
        .iter()
        .copied()
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .collect();
    if l.is_empty() {
        return None;
    }
    let erste = l[0];
    let letzte = l[l.len() - 1];
    let mittel = if l.len() > 2 {
        &l[1..l.len() - 1]
    } else {
        &[][..]
    };
    let (min, max) = match modus {
        RandModus::Links => (erste, erste),
        RandModus::Rechts => (letzte, letzte),
        RandModus::Oben => (erste.min(letzte), erste.max(letzte)),
        RandModus::Unten => {
            if mittel.is_empty() {
                return None;
            }
            let min = mittel.iter().copied().fold(f64::INFINITY, f64::min);
            let max = mittel.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (min, max)
        }
    };
    Some((min, max))
}

/// Zeitpunkte einer Schrittweiten-Rampe wie `scaleToRangenum` in der Vorlage.
///
/// `schritte` ist die Zahl der Schritte im Abschnitt (die Wertigkeit). Zurueck
/// kommen `schritte + 1` Zeitpunkte von `start_s` bis `ende_s`:
/// - das **erste** Intervall ist `step_links_ms`,
/// - das **letzte** ist `step_rechts_ms`,
/// - die mittleren Intervalle verteilt die Vorlage mit `kd`/`sumanz`
///   (die „Annaeherung der Schrittweite wie im Code“).
///
/// Beispiel aus der Nutzervorgabe: `rampe_zeitpunkte(0, 3, 3, 750, 375)`
/// ergibt `[0, 0.75, 2.625, 3.0]`; die letzten beiden Zeitstempel liegen im
/// Bereich `3 s − 1/8·3 s … 3 s`.
pub fn rampe_zeitpunkte(
    start_s: f64,
    ende_s: f64,
    schritte: u32,
    step_links_ms: f64,
    step_rechts_ms: f64,
) -> Vec<f64> {
    let total = ende_s - start_s;
    if !total.is_finite() || total <= 0.0 || schritte == 0 {
        return Vec::new();
    }
    let n = schritte as usize;
    if n == 1 {
        // Ein Schritt: nur der Anfang; das Ende gehoert schon zum naechsten
        // Abschnitt (wie die Endmarke in notes3).
        return vec![start_s, ende_s];
    }
    let links = (step_links_ms.max(0.0) / 1000.0).min(total);
    let rechts = (step_rechts_ms.max(0.0) / 1000.0).min(total);
    if n == 2 {
        // Keine mittleren Intervalle: die beiden Randlaengen proportional
        // strecken, damit der Abschnitt exakt gefuellt ist.
        let summe = links + rechts;
        let (l, _r) = if summe > 0.0 {
            (links * total / summe, rechts * total / summe)
        } else {
            (total / 2.0, total / 2.0)
        };
        return vec![start_s, start_s + l, ende_s];
    }
    // Zu grosse Raender: dann bleibt fuer die mittleren Intervalle kein
    // Platz. Statt die Raender zu stauchen (die Mitte wuerde negativ und die
    // Zeitachse rueckwaerts laufen) nehmen wir gleichmaessige Schritte.
    if links + rechts >= total {
        let s = total / n as f64;
        return (0..=n).map(|k| start_s + s * k as f64).collect();
    }
    let mitte = total - links - rechts;
    let anzahl_mitte = (n - 2) as f64;
    let kd = if links > rechts {
        (mitte - rechts * anzahl_mitte).max(0.0)
    } else {
        (mitte - links * anzahl_mitte).max(0.0)
    };
    let sumanz = anzahl_mitte * (anzahl_mitte + 1.0) / 2.0;
    let kdtanz = if sumanz > 0.0 { kd / sumanz } else { 0.0 };
    let mut zeiten = Vec::with_capacity(n + 1);
    zeiten.push(start_s);
    let mut t = start_s + links;
    zeiten.push(t);
    for i in 1..=(n - 2) {
        let intervall = if links > rechts {
            rechts + (anzahl_mitte - i as f64 + 1.0) * kdtanz
        } else {
            links + i as f64 * kdtanz
        };
        t += intervall;
        zeiten.push(t);
    }
    // Den Endpunkt exakt setzen: die Summe der Intervalle ist durch die
    // Verteilung bereits `total`, aber Rundung darf den Takt nicht kippen.
    zeiten.push(ende_s);
    zeiten
}

/// Rohaufnahme sortieren und auf `[0, ende_s]` klemmen.
fn aufnahme_sortiert(werte: &[AufnahmeWert], ende_s: f64) -> Vec<AufnahmeWert> {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let mut v: Vec<AufnahmeWert> = werte
        .iter()
        .copied()
        .filter(|(_, t)| t.is_finite())
        .map(|(wert, t)| (wert, t.clamp(0.0, ende)))
        .collect();
    v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Median einer Zahlenfolge (leer -> None). Wird fuer die Pausenlaenge
/// gebraucht, wenn keine feste Schrittweite eingestellt ist.
fn median(mut v: Vec<f64>) -> Option<f64> {
    v.retain(|x| x.is_finite() && *x > 0.0);
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len() / 2;
    if v.len() % 2 == 0 {
        Some((v[m - 1] + v[m]) / 2.0)
    } else {
        Some(v[m])
    }
}

/// notes -> notes3: jeden Abschnitt in `Wert` gleiche Schritte zerlegen.
///
/// Der Wert gilt vom eigenen Zeitstempel bis zum naechsten (bzw. bis
/// `ende_s`). `0` legt eine Pausenmarke an; sie belegt keinen klingenden
/// Schritt, ihre Dauer wird aber in [`auf_taktzeit_mappen_notes4`] als Luecke
/// gezaehlt.
///
/// Mit `optionen.rampe` laufen die Schrittlaengen von `step_min_ms` (links)
/// nach `step_max_ms` (rechts) statt gleich zu sein. Die Rampe wird auf die
/// Abschnittsdauer gestreckt, damit der naechste Druck exakt am Ende steht.
pub fn zwischenschritte_notes3(
    werte: &[AufnahmeWert],
    ende_s: f64,
    optionen: &AufnahmeOptionen,
) -> Vec<AufnahmeNote> {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let liste = aufnahme_sortiert(werte, ende);
    let mut raus = Vec::new();
    for (i, &(wert, t0)) in liste.iter().enumerate() {
        let t1 = liste.get(i + 1).map(|(_, t)| *t).unwrap_or(ende);
        if wert <= 0 {
            // Pause: Marke setzen, kein klingender Schritt.
            raus.push(AufnahmeNote {
                zeit_s: t0,
                wert: 0,
            });
            continue;
        }
        if t1 <= t0 {
            // Null-Dauer (z. B. zwei Tasten im selben Frame): ein Schritt am
            // Anfang, damit der Wert nicht verloren geht.
            raus.push(AufnahmeNote { zeit_s: t0, wert });
            continue;
        }
        let v = wert as u32;
        let zeiten: Vec<f64> =
            if optionen.rampe && optionen.step_min_ms > 0.0 && optionen.step_max_ms > 0.0 {
                rampe_zeitpunkte(t0, t1, v, optionen.step_min_ms, optionen.step_max_ms)
            } else {
                let schritt_s = (t1 - t0) / v as f64;
                (0..v).map(|k| t0 + schritt_s * k as f64).collect()
            };
        for zeit_s in zeiten.into_iter().take(v as usize) {
            raus.push(AufnahmeNote { zeit_s, wert });
        }
    }
    raus
}

/// notes3 -> notes4: die Schrittfolge auf die Taktzeit legen.
///
/// Ein klingender notes3-Eintrag wird **ein** Takt-Schritt; eine Pause
/// rueckt den Schrittzaehler um ihre Dauer in Schritten weiter (Luecke im
/// Verlauf). Die Pausenlaenge kommt aus dem Fader (`step_min_ms`/
/// `step_max_ms`); ohne feste Grenzen aus dem Median der klingenden
/// Schrittlaengen. So bleibt „0 = Pause“ sichtbar, ohne die Taktzeit zu
/// verbiegen.
pub fn auf_taktzeit_mappen_notes4(
    notes3: &[AufnahmeNote],
    ende_s: f64,
    optionen: &AufnahmeOptionen,
) -> Vec<(u32, i32)> {
    let mut schritt_ms = 0.0;
    if optionen.step_min_ms > 0.0 && optionen.step_max_ms > 0.0 {
        schritt_ms = fader_schritt_ms(optionen.step_min_ms, optionen.step_max_ms, optionen);
    }
    if schritt_ms <= 0.0 {
        // Fallback: Median der klingenden Schrittlaengen.
        let mut laengen = Vec::new();
        for paar in notes3.windows(2) {
            if paar[0].wert > 0 && paar[1].wert > 0 {
                let d = paar[1].zeit_s - paar[0].zeit_s;
                if d > 0.0 {
                    laengen.push(d * 1000.0);
                }
            }
        }
        schritt_ms = median(laengen).unwrap_or(0.0);
    }
    let mut raus = Vec::new();
    let mut schritt: u32 = 0;
    for (i, note) in notes3.iter().enumerate() {
        if note.wert > 0 {
            raus.push((schritt, note.wert));
            schritt = schritt.saturating_add(1);
        } else {
            let bis = notes3
                .get(i + 1)
                .map(|n| n.zeit_s)
                .unwrap_or(ende_s.max(note.zeit_s));
            let dauer_ms = ((bis - note.zeit_s).max(0.0)) * 1000.0;
            let pause = if schritt_ms > 0.0 {
                (dauer_ms / schritt_ms).round().max(0.0) as u32
            } else {
                0
            };
            schritt = schritt.saturating_add(pause);
        }
    }
    raus
}

/// Die komplette Kette notes -> notes3 -> notes4.
///
/// Das ist die Funktion, die Maske 2 beim „Ende“ aufruft: Wert gilt von
/// seinem Zeitstempel bis zum naechsten (bzw. bis `ende_s`), in gleiche
/// Schritte zerlegt; `0` ist die Pause. Zurueck kommt der Verlauf als
/// `(Schrittnummer, Wert)` mit Luecken fuer Pausen.
pub fn schritte_aus_aufnahme(
    werte: &[AufnahmeWert],
    ende_s: f64,
    optionen: &AufnahmeOptionen,
) -> Vec<(u32, i32)> {
    let notes3 = zwischenschritte_notes3(werte, ende_s, optionen);
    auf_taktzeit_mappen_notes4(&notes3, ende_s, optionen)
}

/// Blockdauern und Pausen einer Rohaufnahme fuer die Maske-1-Vorschau und
/// das Bereichs-Mappen.
///
/// `dauern` ist `(Wert, Wert)` je klingendem Druck (der Wert ist die Dauer
/// in Schritten), `pausen` die Pausen **vor** dem jeweiligen Block. Die
/// Pausenlaenge wird mit derselben Schrittweite wie in
/// [`auf_taktzeit_mappen_notes4`] gerechnet; ohne feste Grenzen mit dem
/// Median der Abschnitts-Schrittlaengen.
pub fn aufnahme_blockstruktur(
    werte: &[AufnahmeWert],
    ende_s: f64,
    optionen: &AufnahmeOptionen,
) -> (Vec<(i32, u32)>, Vec<u32>) {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let liste = aufnahme_sortiert(werte, ende);
    let mut schritt_ms = 0.0;
    if optionen.step_min_ms > 0.0 && optionen.step_max_ms > 0.0 {
        schritt_ms = fader_schritt_ms(optionen.step_min_ms, optionen.step_max_ms, optionen);
    }
    if schritt_ms <= 0.0 {
        let mut laengen = Vec::new();
        for (i, &(wert, t0)) in liste.iter().enumerate() {
            if wert <= 0 {
                continue;
            }
            let t1 = liste.get(i + 1).map(|(_, t)| *t).unwrap_or(ende);
            if t1 > t0 {
                laengen.push((t1 - t0) * 1000.0 / wert as f64);
            }
        }
        schritt_ms = median(laengen).unwrap_or(0.0);
    }
    let mut dauern = Vec::new();
    let mut pausen = Vec::new();
    let mut pause_steps: u32 = 0;
    for (i, &(wert, t0)) in liste.iter().enumerate() {
        let t1 = liste.get(i + 1).map(|(_, t)| *t).unwrap_or(ende);
        if wert <= 0 {
            let dauer_ms = ((t1 - t0).max(0.0)) * 1000.0;
            if schritt_ms > 0.0 {
                pause_steps =
                    pause_steps.saturating_add((dauer_ms / schritt_ms).round().max(0.0) as u32);
            }
            continue;
        }
        dauern.push((wert, wert.max(0) as u32));
        pausen.push(pause_steps);
        pause_steps = 0;
    }
    (dauern, pausen)
}

// ---------------------------------------------------------------------------
// Tasten `-` / `--`: Schrittweiten-Bereiche und Handbearbeitung
// ---------------------------------------------------------------------------
//
// Die Vorlage kennt fuer Schrittweiten die K+Eingaben (startIndexnum,
// endIndexnum, schrittweitanz). Hier ersetzen zwei echte Tasten das:
// - `-`  setzt die Weite fuer die **laufende Wertphase** auf 1/4 ihrer Dauer,
// - `--` setzt eine **globale** Weite: 1/4 ab dem Tastendruck bis zum Ende.
//
// Die Weite wird roh gespeichert (keine stille Begrenzung). Die Anzahl der
// Schritte ist `round(Dauer/Weite)` und darf **0** sein — dann ist der
// Abschnitt eine Pause, genau wie Taste 0.

/// Woher die Schrittweite eines Bereichs kommt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeiteArt {
    /// Keine `-`-Taste: der getippte Wert zaehlt die Schritte.
    Wert,
    /// `-`: 1/4 der laufenden Wertphase.
    Lokal,
    /// `--`: 1/4 ab dem Tastendruck bis zum Ende.
    Global,
}

impl WeiteArt {
    /// Kurzname fuer Anzeige und Log.
    pub fn name(self) -> &'static str {
        match self {
            WeiteArt::Wert => "Wert",
            WeiteArt::Lokal => "lokal",
            WeiteArt::Global => "global",
        }
    }
}

/// Eine `-`/`--`-Taste mit relativem Zeitpunkt (Sekunden ab Aufnahmebeginn).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeitenEreignis {
    pub art: WeiteArt,
    pub zeit_s: f64,
}

/// Ein bearbeitbarer Bereich der Aufnahme.
///
/// `weite_s` und `schritte` sind **roh** aus der Aufnahme gerechnet. Die
/// Handbearbeitung aendert nur diese Kopie; `roh_bereiche` bleibt daneben
/// unangetastet (eigene Ebene).
#[derive(Clone, Debug, PartialEq)]
pub struct Bereich {
    pub art: WeiteArt,
    /// Getippter Wert/Ton (fuer Farbe und Blockdauer).
    pub wert: i32,
    pub start_s: f64,
    pub ende_s: f64,
    /// Rohe Schrittweite in Sekunden (nicht geklemmt).
    pub weite_s: f64,
    /// Rohe Schrittzahl; 0 ist eine Pause.
    pub schritte: u32,
    /// Erster/letzter Schritt im Gesamtverlauf (einschliesslich).
    pub start_schritt: u32,
    pub ende_schritt: u32,
    /// Fadergrenzen und -stellung fuer die Handbearbeitung (ms).
    pub min_ms: f64,
    pub max_ms: f64,
    pub fader: f64,
}

/// 1/4 der laufenden Wertphase.
pub fn weite_lokal_s(start_s: f64, ende_s: f64) -> f64 {
    ((ende_s - start_s).max(0.0)) / 4.0
}

/// 1/4 ab dem Tastendruck bis zum Aufnahmeende.
pub fn weite_global_s(taste_s: f64, ende_s: f64) -> f64 {
    ((ende_s - taste_s).max(0.0)) / 4.0
}

/// Rohrechnung Dauer/Weite, **ohne Minimum** und ohne Obergrenze.
///
/// `round` ist die kaufmaennische Annaeherung; bei einer Weite groesser als
/// die doppelte Dauer kommt 0 heraus (Pause). Eine technische Saettigung bei
/// `u32::MAX` verhindert nur einen Ueberlauf, keine musikalische Begrenzung.
pub fn schritte_aus_weite(dauer_s: f64, weite_s: f64) -> u32 {
    if !dauer_s.is_finite() || dauer_s <= 0.0 || !weite_s.is_finite() || weite_s <= 0.0 {
        return 0;
    }
    let roh = (dauer_s / weite_s).round();
    if !roh.is_finite() || roh <= 0.0 {
        0
    } else if roh >= u32::MAX as f64 {
        u32::MAX
    } else {
        roh as u32
    }
}

/// Nur ein Hinweis, keine Klemmung: ist die rohe Weite groesser als die
/// Pattern-/Taktzeit? Die App meldet das im Status/Log und laesst den Wert
/// unveraendert.
pub fn weite_ueber_pattern(weite_s: f64, pattern_s: f64) -> bool {
    pattern_s.is_finite() && pattern_s > 0.0 && weite_s.is_finite() && weite_s > pattern_s
}

/// Wertphasen: aufeinanderfolgende gleiche Werte bilden eine Phase.
/// Eine Phase endet am naechsten **anderen** Wert oder am Aufnahmeende.
fn phasen_aus_aufnahme(werte: &[AufnahmeWert], ende_s: f64) -> Vec<(i32, f64, f64)> {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let liste = aufnahme_sortiert(werte, ende);
    let mut raus = Vec::new();
    let mut i = 0;
    while i < liste.len() {
        let (wert, start) = liste[i];
        let mut j = i + 1;
        while j < liste.len() && liste[j].0 == wert {
            j += 1;
        }
        let phase_ende = if j < liste.len() { liste[j].1 } else { ende };
        raus.push((wert, start, phase_ende.max(start)));
        i = j;
    }
    raus
}

/// Die passende Weite fuer eine Phase: die **spaeteste** Taste, die auf die
/// Phase wirkt. Ein globales `--` gilt ab seinem Zeitpunkt auch fuer spaetere
/// Phasen; ein lokales `-` nur fuer die Phase, in der es gedrueckt wurde.
/// Eine Taste genau am Phasenende gehoert zur naechsten Phase.
fn weite_fuer_phase(
    start_s: f64,
    ende_s: f64,
    weiten: &[WeitenEreignis],
    aufnahme_ende_s: f64,
) -> (WeiteArt, f64) {
    let mut best: Option<WeitenEreignis> = None;
    for e in weiten {
        if !e.zeit_s.is_finite() || e.art == WeiteArt::Wert {
            continue;
        }
        let applies = match e.art {
            WeiteArt::Lokal => e.zeit_s >= start_s - 1e-9 && e.zeit_s < ende_s - 1e-9,
            WeiteArt::Global => e.zeit_s < ende_s - 1e-9,
            WeiteArt::Wert => false,
        };
        if !applies {
            continue;
        }
        let besser = match best {
            None => true,
            Some(b) => {
                e.zeit_s > b.zeit_s + 1e-9
                    || ((e.zeit_s - b.zeit_s).abs() <= 1e-9
                        && e.art == WeiteArt::Global
                        && b.art == WeiteArt::Lokal)
            }
        };
        if besser {
            best = Some(*e);
        }
    }
    match best {
        Some(e) if e.art == WeiteArt::Lokal => (WeiteArt::Lokal, weite_lokal_s(start_s, ende_s)),
        Some(e) => (WeiteArt::Global, weite_global_s(e.zeit_s, aufnahme_ende_s)),
        None => (WeiteArt::Wert, 0.0),
    }
}

/// Bereiche aus Rohaufnahme und `-`/`--`-Tasten. Die Schrittzahlen sind roh;
/// die Spannen (`start_schritt`/`ende_schritt`) fuellt
/// [`bereich_schrittspannen`], weil dafuer die Pausenweite gebraucht wird.
pub fn bereiche_aus_aufnahme(
    werte: &[AufnahmeWert],
    ende_s: f64,
    weiten: &[WeitenEreignis],
) -> Vec<Bereich> {
    let ende = if ende_s.is_finite() {
        ende_s.max(0.0)
    } else {
        0.0
    };
    let phasen = phasen_aus_aufnahme(werte, ende);
    let mut raus = Vec::new();
    for (wert, start, phase_ende) in phasen {
        let (art, weite) = weite_fuer_phase(start, phase_ende, weiten, ende);
        let dauer = (phase_ende - start).max(0.0);
        // Eine 0-Phase bleibt immer Pause — auch wenn eine `-`/`--`-Taste
        // daneben liegt.
        let (weite_s, schritte) = if wert <= 0 {
            (0.0, 0)
        } else {
            match art {
                WeiteArt::Wert => (dauer / wert as f64, wert as u32),
                _ => {
                    let w = weite.max(0.0);
                    (w, schritte_aus_weite(dauer, w))
                }
            }
        };
        let weite_ms = weite_s * 1000.0;
        raus.push(Bereich {
            art,
            wert,
            start_s: start,
            ende_s: phase_ende,
            weite_s,
            schritte,
            start_schritt: 0,
            ende_schritt: 0,
            // Der Fader startet in der Mitte der rohen Weite; der Nutzer
            // kann Min/Max und damit den Bereich selbst setzen.
            min_ms: weite_ms * 0.5,
            max_ms: weite_ms * 1.5,
            fader: 0.5,
        });
    }
    raus
}

/// Die Schrittweite fuer Pausen: Fader aus den Optionen, sonst Median der
/// rohen Bereichsweiten. 0 heisst „keine Pausenweite bekannt“.
pub fn bereich_schritt_ms(bereiche: &[Bereich], optionen: &AufnahmeOptionen) -> f64 {
    if optionen.step_min_ms > 0.0 && optionen.step_max_ms > 0.0 {
        return fader_schritt_ms(optionen.step_min_ms, optionen.step_max_ms, optionen);
    }
    let laengen: Vec<f64> = bereiche
        .iter()
        .filter(|b| b.schritte > 0 && b.weite_s > 0.0)
        .map(|b| b.weite_s * 1000.0)
        .collect();
    median(laengen).unwrap_or(0.0)
}

/// Die absoluten Schrittspannen je Bereich (einschliesslich) fuellen.
/// Klingende Bereiche belegen `schritte`; 0-Schritt-Bereiche (Pausen)
/// ruecken um ihre Dauer in Pausenschritten weiter.
pub fn bereich_schrittspannen(bereiche: &mut [Bereich], schritt_ms: f64) {
    let mut schritt: u32 = 0;
    for b in bereiche.iter_mut() {
        let dauer = (b.ende_s - b.start_s).max(0.0);
        if b.schritte > 0 {
            b.start_schritt = schritt;
            b.ende_schritt = schritt.saturating_add(b.schritte - 1);
            schritt = schritt.saturating_add(b.schritte);
        } else {
            b.start_schritt = schritt;
            let pause = if schritt_ms.is_finite() && schritt_ms > 0.0 {
                (dauer * 1000.0 / schritt_ms).round().max(0.0) as u32
            } else {
                0
            };
            b.ende_schritt = schritt.saturating_add(pause.saturating_sub(1));
            schritt = schritt.saturating_add(pause);
        }
    }
}

/// notes3 aus fertigen Bereichen. Mit `optionen.rampe` laeuft die
/// Schrittweite je Bereich von `step_min_ms` nach `step_max_ms` (wie die
/// Vorlage); sonst werden die Schritte gleichmaessig ueber den Bereich
/// verteilt. `schritte == 0` wird eine Pausenmarke.
pub fn notes3_aus_bereichen(
    bereiche: &[Bereich],
    optionen: &AufnahmeOptionen,
) -> Vec<AufnahmeNote> {
    let mut raus = Vec::new();
    for b in bereiche {
        if b.schritte == 0 {
            raus.push(AufnahmeNote {
                zeit_s: b.start_s,
                wert: 0,
            });
            continue;
        }
        let dauer = (b.ende_s - b.start_s).max(0.0);
        if optionen.rampe && optionen.step_min_ms > 0.0 && optionen.step_max_ms > 0.0 {
            let zeiten = rampe_zeitpunkte(
                b.start_s,
                b.ende_s,
                b.schritte,
                optionen.step_min_ms,
                optionen.step_max_ms,
            );
            for zeit_s in zeiten.into_iter().take(b.schritte as usize) {
                raus.push(AufnahmeNote {
                    zeit_s,
                    wert: b.wert,
                });
            }
        } else {
            for k in 0..b.schritte {
                raus.push(AufnahmeNote {
                    zeit_s: b.start_s + dauer * k as f64 / b.schritte as f64,
                    wert: b.wert,
                });
            }
        }
    }
    raus
}

/// Blockdauern/Pausen aus fertigen Bereichen fuer die Bereichs-Ersetzung.
/// 0-Schritt-Bereiche sind Pausen und werden vor dem naechsten Block
/// gesammelt.
pub fn bereich_blockstruktur(bereiche: &[Bereich], schritt_ms: f64) -> (Vec<(i32, u32)>, Vec<u32>) {
    let mut dauern = Vec::new();
    let mut pausen = Vec::new();
    let mut pause_steps: u32 = 0;
    for b in bereiche {
        if b.schritte > 0 {
            dauern.push((b.wert, b.schritte));
            pausen.push(pause_steps);
            pause_steps = 0;
        } else if schritt_ms.is_finite() && schritt_ms > 0.0 {
            let dauer = (b.ende_s - b.start_s).max(0.0);
            pause_steps =
                pause_steps.saturating_add((dauer * 1000.0 / schritt_ms).round().max(0.0) as u32);
        }
    }
    (dauern, pausen)
}

/// Bereich mit einer anderen rohen Weite (Handbearbeitung). Keine Klemmung
/// an Min/Max; `0` ergibt 0 Schritte, negative Eingaben werden als 0
/// behandelt.
pub fn bereich_mit_weite(bereich: &Bereich, weite_s: f64) -> Bereich {
    let mut neu = bereich.clone();
    neu.weite_s = if weite_s.is_finite() && weite_s > 0.0 {
        weite_s
    } else {
        0.0
    };
    neu.schritte = schritte_aus_weite((neu.ende_s - neu.start_s).max(0.0), neu.weite_s);
    if neu.max_ms > neu.min_ms {
        neu.fader =
            ((neu.weite_s * 1000.0 - neu.min_ms) / (neu.max_ms - neu.min_ms)).clamp(0.0, 1.0);
    }
    neu
}

/// Fader zwischen den Min/Max-Grenzen eines Bereichs.
pub fn bereich_mit_fader(bereich: &Bereich, fader: f64) -> Bereich {
    let f = if fader.is_finite() {
        fader.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let weite_ms = if bereich.max_ms > bereich.min_ms {
        bereich.min_ms + f * (bereich.max_ms - bereich.min_ms)
    } else {
        bereich.weite_s * 1000.0
    };
    let mut neu = bereich_mit_weite(bereich, weite_ms / 1000.0);
    neu.fader = f;
    neu
}

/// Bereich zeitlich setzen (Start/Ende). Die Weite bleibt; die Schrittzahl
/// wird roh neu gerechnet.
pub fn bereich_mit_zeit(bereich: &Bereich, start_s: f64, ende_s: f64) -> Bereich {
    let mut neu = bereich.clone();
    let start = if start_s.is_finite() {
        start_s.max(0.0)
    } else {
        0.0
    };
    let ende = if ende_s.is_finite() {
        ende_s.max(start)
    } else {
        start
    };
    neu.start_s = start;
    neu.ende_s = ende;
    neu.schritte = schritte_aus_weite((ende - start).max(0.0), neu.weite_s);
    neu
}

/// Bereich verschieben (Dauer bleibt) innerhalb der Aufnahme.
pub fn bereich_verschieben(bereich: &Bereich, delta_s: f64, aufnahme_ende_s: f64) -> Bereich {
    let dauer = (bereich.ende_s - bereich.start_s).max(0.0);
    let max_start = (aufnahme_ende_s - dauer).max(0.0);
    let start = (bereich.start_s + delta_s).clamp(0.0, max_start);
    bereich_mit_zeit(bereich, start, start + dauer)
}

/// Bereich am Ende kuerzen (negative Werte verlaengern bis zum Aufnahmeende).
pub fn bereich_kuerzen(bereich: &Bereich, delta_s: f64, aufnahme_ende_s: f64) -> Bereich {
    let start = bereich.start_s.clamp(0.0, aufnahme_ende_s.max(0.0));
    let ende = (bereich.ende_s - delta_s).clamp(start, aufnahme_ende_s.max(start));
    bereich_mit_zeit(bereich, start, ende)
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

    // ------------------------------------------------------------------
    // M2-Aufnahme: notes -> notes3 -> notes4
    // ------------------------------------------------------------------

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn zeiten_von(notes: &[AufnahmeNote]) -> Vec<f64> {
        notes
            .iter()
            .filter(|n| n.wert > 0)
            .map(|n| n.zeit_s)
            .collect()
    }

    #[test]
    fn aufnahme_2450_ergibt_die_drei_bloecke() {
        // Nutzervorgabe: 2 4 5 0 mit 1 s / 2 s / 4 s Abstand = 7 s.
        // 0 ist die letzte Taste und zugleich das Ende (Dauer 0).
        let werte = [(2, 0.0), (4, 1.0), (5, 3.0), (0, 7.0)];
        let opt = AufnahmeOptionen::default();
        let notes3 = zwischenschritte_notes3(&werte, 7.0, &opt);
        let zeiten = zeiten_von(&notes3);
        assert_eq!(zeiten.len(), 11, "2 + 4 + 5 gleiche Schritte");
        let erwartet = [
            0.0, 0.5, // 0-1 s, Wert 2: zwei gleiche Schritte
            1.0, 1.5, 2.0, 2.5, // 1-3 s, Wert 4: vier gleiche Schritte
            3.0, 3.8, 4.6, 5.4, 6.2, // 3-7 s, Wert 5: fuenf gleiche Schritte
        ];
        for (i, (ist, soll)) in zeiten.iter().zip(erwartet).enumerate() {
            assert!(nah(*ist, soll), "Zeit {i}: {ist} statt {soll}");
        }
        let verlauf = schritte_aus_aufnahme(&werte, 7.0, &opt);
        assert_eq!(verlauf.len(), 11);
        assert_eq!(&verlauf[0..2], &[(0, 2), (1, 2)], "0-1 s: Wert 2");
        assert_eq!(
            &verlauf[2..6],
            &[(2, 4), (3, 4), (4, 4), (5, 4)],
            "1-3 s: Wert 4"
        );
        assert_eq!(
            &verlauf[6..11],
            &[(6, 5), (7, 5), (8, 5), (9, 5), (10, 5)],
            "3-7 s: Wert 5"
        );
        assert_eq!(verlauf_laenge(&verlauf), 11);
    }

    #[test]
    fn null_ist_pause_und_zaehlt_schritte() {
        // 2 Schritte, dann 3 s Pause, dann 3 Schritte. Die Pause belegt keine
        // klingenden Schritte, ruckt die Schrittnummer aber weiter.
        let werte = [(2, 0.0), (0, 1.0), (3, 4.0)];
        let mut opt = AufnahmeOptionen::default();
        opt.step_min_ms = 500.0;
        opt.step_max_ms = 500.0;
        let verlauf = schritte_aus_aufnahme(&werte, 5.0, &opt);
        assert_eq!(
            verlauf,
            vec![(0, 2), (1, 2), (8, 3), (9, 3), (10, 3)],
            "3 s Pause bei 500 ms = 6 Schritte Luecke"
        );
        assert_eq!(verlauf_laenge(&verlauf), 11);
        assert!(
            !verlauf.iter().any(|(s, _)| (2..8).contains(s)),
            "die Pause ist eine Luecke"
        );
    }

    #[test]
    fn ende_kommt_von_aussen_nicht_von_der_null() {
        // Nur ein Druck (4), Ende nach 2 s: der Wert gilt bis zum Ende.
        let werte = [(4, 0.0)];
        let verlauf = schritte_aus_aufnahme(&werte, 2.0, &AufnahmeOptionen::default());
        assert_eq!(verlauf, vec![(0, 4), (1, 4), (2, 4), (3, 4)]);
        // Die Null ist eine Pause, kein Ende: sie belegt keinen klingenden
        // Schritt, das Ende setzt `ende_s`.
        let werte = [(2, 0.0), (0, 1.0)];
        let verlauf = schritte_aus_aufnahme(&werte, 2.0, &AufnahmeOptionen::default());
        assert_eq!(verlauf, vec![(0, 2), (1, 2)]);
    }

    #[test]
    fn rampe_beispiel_4_8_1_ergibt_die_randrechnung() {
        // Nutzerbeispiel: 4, -, 8, 1, / oder Enter; 4->1 = 2 s, 1->Ende =
        // 3 s. Der wirksame Bereich ist 3 s lang; die Rampe laeuft von
        // 1/4*3 s (750 ms) nach 1/8*3 s (375 ms).
        let zeiten = rampe_zeitpunkte(0.0, 3.0, 3, 750.0, 375.0);
        assert_eq!(zeiten.len(), 4);
        for (ist, soll) in zeiten.iter().zip([0.0, 0.75, 2.625, 3.0]) {
            assert!(nah(*ist, soll), "Rampe: {ist} statt {soll}");
        }
        let (zone_von, zone_bis) = randzone(3.0, 1.0 / 8.0);
        assert!(nah(zone_von, 2.625));
        assert!(nah(zone_bis, 3.0));
        let letzte_zwei = &zeiten[2..];
        assert!(
            letzte_zwei.iter().all(|t| *t + 1e-9 >= zone_von),
            "die letzten 2 Zeitstempel liegen im Randbereich"
        );
        assert!(
            nah((letzte_zwei[1] - letzte_zwei[0]) * 1000.0, 375.0),
            "letzter Schritt = 1/8 * 3 s"
        );
        let (min, max) =
            rand_schritt_ms(&zeiten, 3.0, RandModus::Oben, 1.0 / 8.0).expect("zwei Raender");
        assert!(nah(min, 375.0), "Min: {min}");
        assert!(nah(max, 750.0), "Max: {max}");
        let mitte = fader_schritt_ms(min, max, &AufnahmeOptionen::default());
        assert!(nah(mitte, 562.5), "Haelfte aus Min/Max: {mitte}");
    }

    #[test]
    fn alle_vier_pfeil_modi() {
        // Dieselbe Rampe: 750 ms links, 375 ms rechts, dazwischen 1875 ms.
        let zeiten = [0.0, 0.75, 2.625, 3.0];
        let links =
            rand_schritt_ms(&zeiten, 3.0, RandModus::Links, 1.0 / 8.0).expect("linker Rand");
        let oben =
            rand_schritt_ms(&zeiten, 3.0, RandModus::Oben, 1.0 / 8.0).expect("beide Raender");
        let unten = rand_schritt_ms(&zeiten, 3.0, RandModus::Unten, 1.0 / 8.0).expect("Mitte");
        let rechts =
            rand_schritt_ms(&zeiten, 3.0, RandModus::Rechts, 1.0 / 8.0).expect("rechter Rand");
        assert!(
            nah(links.0, 750.0) && nah(links.1, 750.0),
            "links: {links:?}"
        );
        assert!(
            nah(oben.0, 375.0) && nah(oben.1, 750.0),
            "oben (beide): {oben:?}"
        );
        assert!(
            nah(unten.0, 1875.0) && nah(unten.1, 1875.0),
            "unten (ohne Raender): {unten:?}"
        );
        assert!(
            nah(rechts.0, 375.0) && nah(rechts.1, 375.0),
            "rechts: {rechts:?}"
        );
    }

    #[test]
    fn rampe_ist_monoton_und_fuellt_die_zeit() {
        for schritte in 1..=6 {
            for (links, rechts) in [
                (750.0, 375.0),
                (375.0, 750.0),
                (500.0, 500.0),
                (0.0, 0.0),
                (2000.0, 2000.0),
            ] {
                let z = rampe_zeitpunkte(1.0, 4.0, schritte, links, rechts);
                assert_eq!(z.len(), schritte as usize + 1, "n={schritte}");
                assert!(nah(z[0], 1.0));
                assert!(nah(z[z.len() - 1], 4.0));
                assert!(z.windows(2).all(|w| w[1] + 1e-9 >= w[0]), "monoton: {z:?}");
                let summe: f64 = z.windows(2).map(|w| w[1] - w[0]).sum();
                assert!(nah(summe, 3.0), "Summe {summe} bei n={schritte}");
            }
        }
    }

    #[test]
    fn rampe_im_notes3_und_notes4() {
        // Ein Abschnitt mit Wert 3 ueber 3 s, Rampe 750 -> 375 ms: genau das
        // Nutzerbeispiel, nur als Aufnahme statt als Tastenkürzel.
        let mut opt = AufnahmeOptionen::default();
        opt.rampe = true;
        opt.step_min_ms = 750.0;
        opt.step_max_ms = 375.0;
        let werte = [(3, 0.0)];
        let notes3 = zwischenschritte_notes3(&werte, 3.0, &opt);
        let zeiten = zeiten_von(&notes3);
        assert_eq!(zeiten.len(), 3);
        for (ist, soll) in zeiten.iter().zip([0.0, 0.75, 2.625]) {
            assert!(nah(*ist, soll), "notes3: {ist} statt {soll}");
        }
        let verlauf = schritte_aus_aufnahme(&werte, 3.0, &opt);
        assert_eq!(verlauf, vec![(0, 3), (1, 3), (2, 3)]);
    }

    #[test]
    fn fader_haelfte_und_freier_fader() {
        let mut opt = AufnahmeOptionen::default();
        assert!(nah(fader_schritt_ms(375.0, 750.0, &opt), 562.5));
        assert!(
            nah(fader_schritt_ms(750.0, 375.0, &opt), 562.5),
            "Reihenfolge egal"
        );
        opt.haelfte_aus_minmax = false;
        for (f, soll) in [(0.0, 375.0), (0.25, 468.75), (0.5, 562.5), (1.0, 750.0)] {
            opt.fader = f;
            assert!(nah(fader_schritt_ms(375.0, 750.0, &opt), soll), "Fader {f}");
        }
        assert!(nah(fader_schritt_ms(0.0, 0.0, &opt), 0.0));
    }

    #[test]
    fn randzone_und_leere_aufnahme() {
        assert_eq!(randzone(3.0, 0.0), (3.0, 3.0));
        assert_eq!(randzone(3.0, 1.0 / 8.0), (2.625, 3.0));
        assert_eq!(randzone(3.0, 0.5), (1.5, 3.0));
        assert_eq!(randzone(3.0, 2.0), (1.5, 3.0), "auf 1/2 begrenzt");
        assert!(schritte_aus_aufnahme(&[], 2.0, &AufnahmeOptionen::default()).is_empty());
        assert!(zwischenschritte_notes3(&[], 2.0, &AufnahmeOptionen::default()).is_empty());
        assert!(auf_taktzeit_mappen_notes4(&[], 2.0, &AufnahmeOptionen::default()).is_empty());
        assert!(rand_schritt_ms(&[], 3.0, RandModus::Oben, 0.125).is_none());
        assert!(rand_schritt_ms(&[1.0], 3.0, RandModus::Oben, 0.125).is_none());
    }

    #[test]
    fn unordentliche_aufnahme_wird_sortiert_und_geklemmt() {
        // Zeitstempel ausser der Reihe und hinter dem Ende: die Aufnahme
        // wird sortiert und auf [0, ende] geklemmt, damit die Schritte
        // monoton bleiben.
        let werte = [(5, 3.0), (2, 0.0)];
        let verlauf = schritte_aus_aufnahme(&werte, 5.0, &AufnahmeOptionen::default());
        assert_eq!(&verlauf[0..2], &[(0, 2), (1, 2)]);
        assert_eq!(&verlauf[2..], &[(2, 5), (3, 5), (4, 5), (5, 5), (6, 5)]);
    }

    #[test]
    fn schrittlaengen_ohne_pausen_und_rand_modi() {
        // 2/4/5/0: die Pause hinter der 5 darf die letzte Schrittlaenge
        // nicht auf die Wartezeit bis „Ende“ verkuerzen.
        let werte = [(2, 0.0), (4, 1.0), (5, 3.0), (0, 7.0)];
        let laengen = schrittlaengen_aus_aufnahme(&werte, 7.0, &AufnahmeOptionen::default());
        assert_eq!(laengen.len(), 11);
        assert!(laengen[..6].iter().all(|ms| nah(*ms, 500.0)));
        assert!(laengen[6..].iter().all(|ms| nah(*ms, 800.0)));
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Links),
            Some((500.0, 500.0))
        );
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Rechts),
            Some((800.0, 800.0))
        );
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Oben),
            Some((500.0, 800.0))
        );
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Unten),
            Some((500.0, 800.0))
        );
        // Ohne Pause in der Mitte: die 0 hinter der 5 wird ignoriert.
        let ohne_pause =
            schrittlaengen_aus_aufnahme(&werte[..3], 7.0, &AufnahmeOptionen::default());
        assert_eq!(ohne_pause.len(), 11);
    }

    #[test]
    fn schrittlaengen_der_rampe() {
        let mut opt = AufnahmeOptionen::default();
        opt.rampe = true;
        opt.step_min_ms = 750.0;
        opt.step_max_ms = 375.0;
        let laengen = schrittlaengen_aus_aufnahme(&[(3, 0.0)], 3.0, &opt);
        assert_eq!(laengen.len(), 3);
        for (ist, soll) in laengen.iter().zip([750.0, 1875.0, 375.0]) {
            assert!(nah(*ist, soll), "Rampe: {ist} statt {soll}");
        }
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Links),
            Some((750.0, 750.0))
        );
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Rechts),
            Some((375.0, 375.0))
        );
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Oben),
            Some((375.0, 750.0))
        );
        assert_eq!(
            rand_schritt_ms_aus_laengen(&laengen, RandModus::Unten),
            Some((1875.0, 1875.0))
        );
    }

    #[test]
    fn blockstruktur_fuer_die_vorschau() {
        // 2 Schritte, 1 s Pause, 4 Schritte; 500 ms je Schritt.
        let mut opt = AufnahmeOptionen::default();
        opt.step_min_ms = 500.0;
        opt.step_max_ms = 500.0;
        let (dauern, pausen) = aufnahme_blockstruktur(&[(2, 0.0), (0, 1.0), (4, 2.0)], 3.0, &opt);
        assert_eq!(dauern, vec![(2, 2), (4, 4)]);
        assert_eq!(pausen, vec![0, 2], "1 s Pause = 2 Schritte");
    }

    // ------------------------------------------------------------------
    // `-` / `--`: Schrittweiten-Bereiche, 0 Schritte, Handbearbeitung
    // ------------------------------------------------------------------

    fn bereiche_mit_spannen(
        werte: &[AufnahmeWert],
        ende_s: f64,
        weiten: &[WeitenEreignis],
    ) -> Vec<Bereich> {
        let mut b = bereiche_aus_aufnahme(werte, ende_s, weiten);
        let opt = AufnahmeOptionen::default();
        let ms = bereich_schritt_ms(&b, &opt);
        bereich_schrittspannen(&mut b, ms);
        b
    }

    #[test]
    fn lokale_weite_ist_ein_viertel_der_phase() {
        // Wert 4 von 0 bis 2 s, dann 8 (2-3 s) und 1 (3-5 s).
        let werte = [(4, 0.0), (8, 2.0), (1, 3.0)];
        let weiten = [WeitenEreignis {
            art: WeiteArt::Lokal,
            zeit_s: 0.0,
        }];
        let b = bereiche_mit_spannen(&werte, 5.0, &weiten);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].art, WeiteArt::Lokal);
        assert!(nah(b[0].weite_s, 0.5), "2 s / 4 = 0,5 s");
        assert_eq!(b[0].schritte, 4, "2 s / 0,5 s");
        // Die folgenden Phasen bleiben beim Wert.
        assert_eq!(b[1].art, WeiteArt::Wert);
        assert!(nah(b[1].weite_s, 1.0 / 8.0));
        assert_eq!(b[1].schritte, 8);
        assert_eq!(b[2].art, WeiteArt::Wert);
        assert_eq!(b[2].schritte, 1);
    }

    #[test]
    fn globale_weite_ist_ein_viertel_bis_zum_ende() {
        // `--` bei 2 s, Ende 5 s: Weite 0,75 s; die Phase davor bleibt Wert.
        let werte = [(4, 0.0), (8, 2.0), (1, 3.0)];
        let weiten = [WeitenEreignis {
            art: WeiteArt::Global,
            zeit_s: 2.0,
        }];
        let b = bereiche_mit_spannen(&werte, 5.0, &weiten);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].art, WeiteArt::Wert, "vor dem -- unveraendert");
        assert_eq!(b[0].schritte, 4);
        assert_eq!(b[1].art, WeiteArt::Global);
        assert!(nah(b[1].weite_s, 0.75), "3 s / 4 = 0,75 s");
        assert_eq!(b[1].schritte, 1, "1 s / 0,75 s gerundet 1");
        assert_eq!(b[2].art, WeiteArt::Global);
        assert!(nah(b[2].weite_s, 0.75));
        assert_eq!(b[2].schritte, 3, "2 s / 0,75 s gerundet 3");
    }

    #[test]
    fn minus_und_minusminus_wirken_verschieden() {
        // Gleiche Phase 0-2 s (Wert 4): lokal 0,5 s -> 4 Schritte,
        // global ab 0 s bis 5 s: 1,25 s -> 2 Schritte.
        let werte = [(4, 0.0), (8, 2.0), (1, 3.0)];
        let lokal = bereiche_mit_spannen(
            &werte,
            5.0,
            &[WeitenEreignis {
                art: WeiteArt::Lokal,
                zeit_s: 0.0,
            }],
        );
        let global = bereiche_mit_spannen(
            &werte,
            5.0,
            &[WeitenEreignis {
                art: WeiteArt::Global,
                zeit_s: 0.0,
            }],
        );
        assert_ne!(lokal[0].schritte, global[0].schritte);
        assert!(nah(lokal[0].weite_s, 0.5));
        assert!(nah(global[0].weite_s, 1.25));
        assert_eq!(lokal[0].schritte, 4);
        assert_eq!(global[0].schritte, 2);
    }

    #[test]
    fn weite_ohne_klemmung_und_null_schritte() {
        // 1 s Dauer, 2,4 s Weite: roh gespeichert, 0 Schritte (Pause).
        let b = Bereich {
            art: WeiteArt::Global,
            wert: 4,
            start_s: 0.0,
            ende_s: 1.0,
            weite_s: 2.4,
            schritte: schritte_aus_weite(1.0, 2.4),
            start_schritt: 0,
            ende_schritt: 0,
            min_ms: 1000.0,
            max_ms: 3000.0,
            fader: 0.5,
        };
        assert_eq!(b.schritte, 0);
        assert!(nah(b.weite_s, 2.4), "keine stille Begrenzung");
        // 0,1 s Weite auf 2 s: 20 Schritte, nicht auf ein Maximum geklemmt.
        assert_eq!(schritte_aus_weite(2.0, 0.1), 20);
        let fein = bereich_mit_weite(&b, 0.1);
        assert_eq!(fein.schritte, 10, "1 s / 0,1 s (Bereich ist 1 s)");
        // Fader darf die rohe Weite direkt auf 2,4 s setzen.
        let mut b2 = b.clone();
        b2.min_ms = 1000.0;
        b2.max_ms = 3000.0;
        let gefadert = bereich_mit_fader(&b2, 0.7);
        assert!(nah(gefadert.weite_s, 2.4));
        assert_eq!(gefadert.schritte, 0);
    }

    #[test]
    fn null_schritte_aus_globaler_weite_ist_pause() {
        // Global bei 0 s bis 10 s => 2,5 s Weite. Die 1-s-Phase 4 bekommt
        // 0 Schritte; die 9-s-Phase 8 bekommt 4 Schritte.
        let werte = [(4, 0.0), (8, 1.0)];
        let weiten = [WeitenEreignis {
            art: WeiteArt::Global,
            zeit_s: 0.0,
        }];
        let b = bereiche_mit_spannen(&werte, 10.0, &weiten);
        assert_eq!(b[0].schritte, 0, "1 s / 2,5 s = 0,4 -> 0");
        assert_eq!(b[1].schritte, 4, "9 s / 2,5 s = 3,6 -> 4");
        let opt = AufnahmeOptionen::default();
        let n3 = notes3_aus_bereichen(&b, &opt);
        assert_eq!(n3[0].wert, 0, "0 Schritte werden zur Pause");
        assert!(n3.iter().any(|n| n.wert == 8));
    }

    #[test]
    fn handaenderung_ist_eigene_ebene() {
        let roh = Bereich {
            art: WeiteArt::Lokal,
            wert: 4,
            start_s: 1.0,
            ende_s: 3.0,
            weite_s: 0.5,
            schritte: 4,
            start_schritt: 0,
            ende_schritt: 3,
            min_ms: 250.0,
            max_ms: 750.0,
            fader: 0.5,
        };
        let original = roh.clone();
        // Weite direkt aendern.
        let breit = bereich_mit_weite(&roh, 1.0);
        assert_eq!(breit.schritte, 2);
        assert_eq!(roh, original, "Rohwert bleibt unangetastet");
        // Fader aendert nur die Kopie.
        let gefadert = bereich_mit_fader(&roh, 0.0);
        assert!(nah(gefadert.weite_s, 0.25));
        assert_eq!(roh, original);
        // Verschieben und Kuerzen.
        let verschoben = bereich_verschieben(&roh, 0.5, 5.0);
        assert!(nah(verschoben.start_s, 1.5));
        assert!(nah(verschoben.ende_s, 3.5));
        let gekuerzt = bereich_kuerzen(&roh, 0.5, 5.0);
        assert!(nah(gekuerzt.ende_s, 2.5));
        assert_eq!(gekuerzt.schritte, 3, "1,5 s / 0,5 s");
        assert_eq!(roh, original);
    }

    #[test]
    fn bereich_blockstruktur_und_spannen() {
        // 4 Schritte, Pause 1 s (500 ms => 2 Schritte), 2 Schritte.
        let b = vec![
            Bereich {
                art: WeiteArt::Wert,
                wert: 4,
                start_s: 0.0,
                ende_s: 2.0,
                weite_s: 0.5,
                schritte: 4,
                start_schritt: 0,
                ende_schritt: 0,
                min_ms: 250.0,
                max_ms: 750.0,
                fader: 0.5,
            },
            Bereich {
                art: WeiteArt::Wert,
                wert: 0,
                start_s: 2.0,
                ende_s: 3.0,
                weite_s: 0.0,
                schritte: 0,
                start_schritt: 0,
                ende_schritt: 0,
                min_ms: 0.0,
                max_ms: 0.0,
                fader: 0.5,
            },
            Bereich {
                art: WeiteArt::Wert,
                wert: 8,
                start_s: 3.0,
                ende_s: 4.0,
                weite_s: 0.5,
                schritte: 2,
                start_schritt: 0,
                ende_schritt: 0,
                min_ms: 250.0,
                max_ms: 750.0,
                fader: 0.5,
            },
        ];
        let (dauern, pausen) = bereich_blockstruktur(&b, 500.0);
        assert_eq!(dauern, vec![(4, 4), (8, 2)]);
        assert_eq!(pausen, vec![0, 2]);
        let mut b2 = b.clone();
        bereich_schrittspannen(&mut b2, 500.0);
        assert_eq!((b2[0].start_schritt, b2[0].ende_schritt), (0, 3));
        assert_eq!((b2[1].start_schritt, b2[1].ende_schritt), (4, 5));
        assert_eq!((b2[2].start_schritt, b2[2].ende_schritt), (6, 7));
    }

    #[test]
    fn bereiche_mappen_in_bereich_mit_pausen() {
        // 4/2 Schritte, 1 s Pause, 8/3 Schritte; der Bereich 2-8 wird ersetzt.
        let b = vec![
            Bereich {
                art: WeiteArt::Wert,
                wert: 4,
                start_s: 0.0,
                ende_s: 1.0,
                weite_s: 0.5,
                schritte: 2,
                start_schritt: 0,
                ende_schritt: 1,
                min_ms: 250.0,
                max_ms: 750.0,
                fader: 0.5,
            },
            Bereich {
                art: WeiteArt::Wert,
                wert: 0,
                start_s: 1.0,
                ende_s: 2.0,
                weite_s: 0.0,
                schritte: 0,
                start_schritt: 2,
                ende_schritt: 3,
                min_ms: 0.0,
                max_ms: 0.0,
                fader: 0.5,
            },
            Bereich {
                art: WeiteArt::Wert,
                wert: 8,
                start_s: 2.0,
                ende_s: 3.0,
                weite_s: 1.0 / 3.0,
                schritte: 3,
                start_schritt: 4,
                ende_schritt: 6,
                min_ms: 166.0,
                max_ms: 500.0,
                fader: 0.5,
            },
        ];
        let (dauern, pausen) = bereich_blockstruktur(&b, 500.0);
        assert_eq!(dauern, vec![(4, 2), (8, 3)]);
        assert_eq!(pausen, vec![0, 2]);
        let alt: Vec<(u32, i32)> = (0..10).map(|i| (i, 60)).collect();
        let mut verlauf = alt.clone();
        let meldung = bereich_ersetzen_mit_pausen(&mut verlauf, 2, 8, &dauern, &pausen)
            .expect("Bereich 2-8 ist gueltig");
        assert_eq!(meldung.schritte, 6);
        assert_eq!(meldung.dauer, 7, "2 + 3 Bloecke + 2 Pausen");
        assert_eq!(meldung.gesamt, 10);
        assert_eq!(&verlauf[..2], &alt[..2], "davor unveraendert");
        assert_eq!(
            &verlauf[7..],
            &alt[8..],
            "danach unveraendert (Schritte 8, 9)"
        );
        assert!(
            verlauf[2..4].iter().all(|(_, ton)| *ton == 4),
            "4er-Block am Bereichsanfang"
        );
        assert!(
            !verlauf.iter().any(|(s, _)| *s == 4),
            "die Pause bleibt eine Luecke"
        );
        assert!(
            verlauf[4..7].iter().all(|(_, ton)| *ton == 8),
            "8er-Block nach der Pause (Schritte 5-7)"
        );
    }

    #[test]
    fn weite_ueber_pattern_ist_nur_ein_hinweis() {
        assert!(weite_ueber_pattern(2.4, 2.0));
        assert!(!weite_ueber_pattern(2.0, 2.0));
        assert!(!weite_ueber_pattern(1.0, 2.0));
        assert!(!weite_ueber_pattern(f64::NAN, 2.0));
        assert!(!weite_ueber_pattern(2.4, 0.0));
    }

    #[test]
    fn bereiche_ohne_tasten_entsprechen_der_alten_kette() {
        // 2/4/5/0: ohne -/-- muss die Bereichskette dieselben 11 Schritte
        // und dieselben Zeiten wie notes3 liefern.
        let werte = [(2, 0.0), (4, 1.0), (5, 3.0), (0, 7.0)];
        let b = bereiche_mit_spannen(&werte, 7.0, &[]);
        let opt = AufnahmeOptionen::default();
        let n3 = notes3_aus_bereichen(&b, &opt);
        let zeiten: Vec<f64> = n3.iter().filter(|n| n.wert > 0).map(|n| n.zeit_s).collect();
        let erwartet = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.8, 4.6, 5.4, 6.2];
        assert_eq!(zeiten.len(), erwartet.len());
        for (ist, soll) in zeiten.iter().zip(erwartet) {
            assert!(nah(*ist, soll), "Zeit {ist} statt {soll}");
        }
    }
}
