//! Tonfolgen aus Audio: grobe Tonhöhen-Spur, wiederkehrende Motive und deren
//! x-Bereiche für den Kreis-/Bild-Scan.
//!
//! Warum eigene, kleine Schritte: Die Analyse läuft auf dem Gerät und muss
//! auch bei mehreren Minuten Audio bezahlbar bleiben. Die Tonhöhe wird
//! deshalb auf ~8 kHz heruntergetastet und nur alle 50 ms bestimmt; für
//! Melodie-Motive (60–1200 Hz) reicht das und spart den Großteil der Arbeit.

use std::collections::HashMap;

/// Fensterlänge der Tonhöhen-Spur in Sekunden.
pub const FENSTER_S: f32 = 0.05;

/// Suchbereich der Grundfrequenz.
const MIN_HZ: f32 = 60.0;
const MAX_HZ: f32 = 1200.0;

/// Ab dieser (normierten) Autokorrelation gilt ein Fenster als periodisch.
const KORR_SCHWELLE: f32 = 0.4;

/// Grobe Tonhöhen-Spur aus Mono-PCM: (Zeit in s, Frequenz in Hz) je 50-ms-
/// Fenster, per Autokorrelation (60–1200 Hz), 0.0 wenn keine Periodizität.
///
/// Die Zeit ist der Fensterbeginn. Das Signal wird vorher auf etwa 8 kHz
/// gemittelt (Tiefpass); dadurch bleiben Laufzeit und Speicher klein.
pub fn tonhoehen(pcm: &[f32], rate: u32) -> Vec<(f32, f32)> {
    if rate == 0 || pcm.is_empty() {
        return Vec::new();
    }
    // Warum mitteln statt ausdünnen: ein einfacher Tiefpass gegen Alias,
    // der bei 8 kHz noch genug Abstand zu 1200 Hz lässt.
    let faktor = ((rate as f32 / 8000.0).round() as usize).max(1);
    let mut reduz = Vec::with_capacity(pcm.len() / faktor + 1);
    for block in pcm.chunks(faktor) {
        let summe: f32 = block.iter().sum();
        reduz.push(summe / block.len() as f32);
    }
    let arate = rate as f32 / faktor as f32;
    let fenster = (arate * FENSTER_S).round().max(2.0) as usize;

    let mut spur = Vec::with_capacity(reduz.len() / fenster + 1);
    let mut pos = 0;
    while pos + fenster <= reduz.len() {
        // Den letzten halben Rest verwerfen: zu kurz für eine belastbare
        // Periode, und die Zeitachse bleibt so gleichmäßig.
        if reduz.len() - pos < fenster {
            break;
        }
        let zeit = pos as f32 / arate;
        let freq = frequenz(&reduz[pos..pos + fenster], arate);
        spur.push((zeit, freq));
        pos += fenster;
    }
    spur
}

/// Grundfrequenz eines Fensters per normierter Autokorrelation.
fn frequenz(fenster: &[f32], rate: f32) -> f32 {
    let n = fenster.len();
    if n < 8 || rate <= 0.0 {
        return 0.0;
    }
    // Gleichanteil entfernen: sonst dominiert er die Korrelation.
    let mittel = fenster.iter().sum::<f32>() / n as f32;
    let x: Vec<f32> = fenster.iter().map(|v| v - mittel).collect();
    let energie: f32 = x.iter().map(|v| v * v).sum();
    // RMS < 1e-4: Stille (oder nur Rauschen unter der Hörschwelle).
    if !energie.is_finite() || energie / n as f32 <= 1e-8 {
        return 0.0;
    }

    let min_lag = (rate / MAX_HZ).floor().max(1.0) as usize;
    let max_lag = (rate / MIN_HZ).ceil() as usize;
    let max_lag = max_lag.min(n.saturating_sub(2));
    if min_lag >= max_lag {
        return 0.0;
    }

    // korr[lag] = normierte Kreuzkorrelation der überlappenden Teile.
    let mut korr = vec![0.0f32; max_lag + 1];
    for lag in min_lag..=max_lag {
        let mut s = 0.0f32;
        let mut ea = 0.0f32;
        let mut eb = 0.0f32;
        for i in 0..n - lag {
            let a = x[i];
            let b = x[i + lag];
            s += a * b;
            ea += a * a;
            eb += b * b;
        }
        korr[lag] = if ea > 0.0 && eb > 0.0 {
            s / (ea * eb).sqrt()
        } else {
            0.0
        };
    }

    // Das *erste* deutliche Maximum nehmen, nicht das globale: ganzzahlige
    // Vielfache der Periode sind ebenfalls Maxima und ergäben Oktavfehler
    // nach unten.
    let mut best_lag = 0usize;
    let mut best = 0.0f32;
    for lag in (min_lag + 1)..max_lag {
        if korr[lag] >= KORR_SCHWELLE && korr[lag] > korr[lag - 1] && korr[lag] >= korr[lag + 1] {
            best_lag = lag;
            best = korr[lag];
            break;
        }
    }
    if best_lag == 0 {
        // Kein sauberes lokales Maximum (z. B. sehr kurzer Ausschlag):
        // den globalen Wert nehmen, wenn er über der Schwelle liegt.
        for lag in min_lag..=max_lag {
            if korr[lag] > best {
                best = korr[lag];
                best_lag = lag;
            }
        }
    }
    if best_lag == 0 || best < KORR_SCHWELLE {
        return 0.0;
    }

    // Parabel durch die drei Nachbarn: ganzzahlige Lags sind bei 8 kHz
    // sonst bis ~3 % daneben (z. B. 440 Hz bei Lag 18).
    let (l0, l1, l2) = (korr[best_lag - 1], korr[best_lag], korr[best_lag + 1]);
    let nenner = l0 - 2.0 * l1 + l2;
    let delta = if nenner.abs() > 1e-9 {
        (0.5 * (l0 - l2) / nenner).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let lag = best_lag as f32 + delta;
    let freq = rate / lag;
    if freq.is_finite() && (MIN_HZ..=MAX_HZ).contains(&freq) {
        freq
    } else {
        0.0
    }
}

/// Frequenz → MIDI-Note (gerundet), 0 wenn Frequenz 0.
pub fn midi(freq: f32) -> i32 {
    if !freq.is_finite() || freq <= 0.0 {
        return 0;
    }
    let note = 69.0 + 12.0 * (freq / 440.0).log2();
    if !note.is_finite() {
        return 0;
    }
    // 0 ist der „keine Note"-Wert; echte Noten liegen 1..127.
    (note.round() as i32).clamp(1, 127)
}

/// Ein wiederkehrendes Motiv.
#[derive(Debug, Clone, PartialEq)]
pub struct Motiv {
    /// MIDI-Noten einer Ausführung (mindestens `laenge`).
    pub noten: Vec<i32>,
    /// Startzeiten aller Vorkommen in Sekunden, aufsteigend.
    pub starts: Vec<f32>,
    /// Dauer einer Ausführung in Sekunden (erste Note bis Ende der letzten).
    pub dauer_s: f32,
}

/// Ein Notenabschnitt (eine gehaltene Note) mit Start und Ende.
struct Abschnitt {
    start: f32,
    ende: f32,
    note: i32,
}

/// Aufeinanderfolgende gleiche Noten zu einem Abschnitt zusammenfassen.
///
/// Warum: Die Tonhöhen-Spur liefert je 50-ms-Fenster einen Wert. Eine
/// gehaltene Note wäre sonst 4–20 „Noten" lang und Motive würden nur
/// zufällig wiederkehren.
fn abschnitte(noten: &[(f32, i32)]) -> Vec<Abschnitt> {
    let mut seg: Vec<Abschnitt> = Vec::new();
    for &(t, n) in noten {
        if n <= 0 || !t.is_finite() {
            continue;
        }
        match seg.last_mut() {
            Some(letzter) if letzter.note == n => letzter.ende = t,
            _ => seg.push(Abschnitt {
                start: t,
                ende: t,
                note: n,
            }),
        }
    }
    // Ende eines Abschnitts ist der Start des nächsten; für den letzten
    // schätzen wir die Fensterlänge aus den Abständen (Fallback 50 ms).
    let mut schritt = FENSTER_S;
    if seg.len() >= 2 {
        let mut diffs: Vec<f32> = seg
            .windows(2)
            .map(|w| w[1].start - w[0].start)
            .filter(|d| *d > 0.0 && d.is_finite())
            .collect();
        if !diffs.is_empty() {
            diffs.sort_by(|a, b| a.total_cmp(b));
            schritt = diffs[diffs.len() / 2];
        }
    }
    for i in 0..seg.len() {
        let next = seg.get(i + 1).map(|s| s.start);
        seg[i].ende = next.unwrap_or(seg[i].ende + schritt);
        if seg[i].ende <= seg[i].start {
            seg[i].ende = seg[i].start + schritt;
        }
    }
    seg
}

/// Gehaltene Notenabschnitte aus PCM: (MIDI-Note, Start, Ende) in Sekunden.
///
/// Warum öffentlich: Der Kreis-Tab legt eine interne Aufnahme auch dann als
/// Preset ab, wenn sie kein wiederkehrendes Motiv enthält (z. B. ein einzelner
/// gehaltener Akkord). Dafür braucht er die Abschnitte, nicht nur Motive.
pub fn noten_segmente(pcm: &[f32], rate: u32) -> Vec<(i32, f32, f32)> {
    let spur = tonhoehen(pcm, rate);
    let noten: Vec<(f32, i32)> = spur.iter().map(|(t, f)| (*t, midi(*f))).collect();
    abschnitte(&noten)
        .into_iter()
        .map(|a| (a.note, a.start, a.ende))
        .collect()
}

/// Kleinste Periode einer Notenfolge (für „ABCABC" → 3).
fn periode(noten: &[i32]) -> usize {
    let n = noten.len();
    for p in 1..n {
        if n % p == 0 && (0..n).all(|i| noten[i] == noten[i % p]) {
            return p;
        }
    }
    n
}

/// Alle Startindizes, an denen `folge` in den Abschnitten vorkommt.
fn vorkommen(seg: &[Abschnitt], folge: &[i32]) -> Vec<usize> {
    if folge.is_empty() || folge.len() > seg.len() {
        return Vec::new();
    }
    (0..=seg.len() - folge.len())
        .filter(|&i| (0..folge.len()).all(|k| seg[i + k].note == folge[k]))
        .collect()
}

/// Wiederkehrende Motive (mindestens `laenge` Noten, mindestens 2 Vorkommen),
/// Rückgabe: Notenfolge + Startzeiten in s.
///
/// Gesucht werden die längsten gemeinsamen Anfänge zweier Startpunkte
/// (nicht überlappend). Mehrperiodige Kandidaten (ABCABC) und Drehungen
/// desselben Motivs (BCA, CAB) werden verworfen, damit die Preset-Liste
/// lesbar bleibt.
pub fn finde_motive(noten: &[(f32, i32)], laenge: usize) -> Vec<Motiv> {
    let seg = abschnitte(noten);
    let laenge = laenge.max(1);
    if seg.len() < laenge * 2 {
        return Vec::new();
    }

    // Kandidaten über Paare von Startpunkten sammeln; der Vergleich endet
    // spätestens beim zweiten Start (keine Überlappung).
    let mut kandidaten: HashMap<Vec<i32>, Vec<usize>> = HashMap::new();
    for i in 0..seg.len() {
        for j in (i + 1)..seg.len() {
            let max_l = (j - i).min(seg.len() - j);
            if max_l < laenge {
                continue;
            }
            let mut l = 0;
            while l < max_l && seg[i + l].note == seg[j + l].note {
                l += 1;
            }
            if l < laenge {
                continue;
            }
            let folge: Vec<i32> = seg[i..i + l].iter().map(|s| s.note).collect();
            // Mehrperiodige Folgen sind nur Verlängerungen eines kürzeren
            // Motivs (ABCABC → ABC).
            if periode(&folge) != folge.len() {
                continue;
            }
            if !kandidaten.contains_key(&folge) {
                let starts = vorkommen(&seg, &folge);
                if starts.len() >= 2 {
                    kandidaten.insert(folge, starts);
                }
            }
        }
    }

    let mut liste: Vec<(Motiv, Vec<usize>)> = kandidaten
        .into_iter()
        .map(|(folge, starts)| {
            let erster = starts[0];
            let letzter = erster + folge.len() - 1;
            let dauer = (seg[letzter].ende - seg[erster].start).max(0.0);
            (
                Motiv {
                    noten: folge,
                    starts: starts.iter().map(|&i| seg[i].start).collect(),
                    dauer_s: dauer,
                },
                starts,
            )
        })
        .collect();

    // Häufigste Motive zuerst: dann gewinnt bei Drehungen die vollständige
    // Ausführung (A E A mit 3×) gegen die am Dateiende abgeschnittene (E A A
    // mit 2×).
    liste.sort_by(|(a, a_starts), (b, b_starts)| {
        b_starts
            .len()
            .cmp(&a_starts.len())
            .then(a_starts[0].cmp(&b_starts[0]))
            .then(b.noten.len().cmp(&a.noten.len()))
    });

    // Drehungen (gleiche Länge, um einen festen Versatz verschoben) sind
    // dasselbe Motiv. Die jüngere darf weniger Vorkommen haben: am Dateiende
    // fehlt die letzte Runde, die Startpunkte liegen aber passend verschoben.
    let mut behalten: Vec<(Motiv, Vec<usize>)> = Vec::new();
    for (motiv, starts) in liste.drain(..) {
        let drehung = behalten.iter().any(|(anderes, andere_starts)| {
            let n = motiv.noten.len();
            if anderes.noten.len() != n {
                return false;
            }
            let versatz =
                (0..n).find(|&o| (0..n).all(|k| motiv.noten[k] == anderes.noten[(k + o) % n]));
            let Some(o) = versatz else {
                return false;
            };
            starts
                .iter()
                .all(|&s| s >= o && andere_starts.contains(&(s - o)))
        });
        if !drehung {
            behalten.push((motiv, starts));
        }
    }

    // Kürzere Motive, die vollständig in einem längeren Vorkommen liegen
    // (z. B. BCD in ABCD), weglassen.
    let mut ergebnis: Vec<Motiv> = Vec::new();
    for (i, (motiv, starts)) in behalten.iter().enumerate() {
        let enthalten = behalten
            .iter()
            .enumerate()
            .any(|(j, (lang, lange_starts))| {
                if i == j || lang.noten.len() <= motiv.noten.len() {
                    return false;
                }
                // Kommt das kürzere Motiv als Fenster im längeren vor?
                let fenster = lang
                    .noten
                    .windows(motiv.noten.len())
                    .position(|w| w == motiv.noten.as_slice());
                let Some(offset) = fenster else {
                    return false;
                };
                // ... und liegen alle Vorkommen in einem langen Vorkommen?
                starts
                    .iter()
                    .all(|&s| lange_starts.iter().any(|&l| l + offset == s))
            });
        if !enthalten {
            ergebnis.push(motiv.clone());
        }
    }

    ergebnis.sort_by(|a, b| {
        a.starts[0]
            .total_cmp(&b.starts[0])
            .then(b.starts.len().cmp(&a.starts.len()))
            .then(b.noten.len().cmp(&a.noten.len()))
    });
    ergebnis
}

/// Cent-Schritt der Thai-7-Ton-Gleichschwebung: sieben gleiche Schritte je
/// Oktave. Die klassische Thai-Musik nutzt diese Stimmung mit pentatonischen
/// Ausschnitten; 171 Cent liegt zwischen einem Halbton und einem
/// Ganztonschritt, deshalb klingt die 12-TET-Näherung absichtlich „schwebend“.
pub const THAI_CENT: f32 = 1200.0 / 7.0;

/// Grundton der eingebauten Thai-Motive (MIDI-Näherung der 7-TET-Stufe 0).
pub const THAI_GRUNDTON: i32 = 60;

/// Namen der sechs eingebauten Thai-Motive, gleiche Reihenfolge wie
/// [`thai_motive`]. Warum getrennt: `Motiv` bleibt reine Daten für die
/// Analyse; die Anzeige braucht keine zusätzlichen Felder.
pub const THAI_NAMEN: [&str; 6] = [
    "aufwärts 3",
    "abwärts 3",
    "3+1 auf",
    "3+1 ab",
    "5-Ton auf",
    "5-Ton ab",
];

/// 7-TET-Stufe → MIDI-Note (gerundet). Nur für Anzeige/Vergleich; die
/// eigentliche Stimmung liegt in [`thai_frequenz`].
pub fn thai_midi(stufe: i32, grundton: i32) -> i32 {
    (grundton as f32 + stufe as f32 * 12.0 / 7.0)
        .round()
        .clamp(0.0, 127.0) as i32
}

/// Frequenz einer 7-TET-Stufe über `grundton` (MIDI). Damit kann die App die
/// Thai-Motive auch ohne Audiodatei hörbar machen, ohne die 171-Cent-Schritte
/// auf 12-TET zu verbiegen.
pub fn thai_frequenz(stufe: i32, grundton: i32) -> f32 {
    let basis = 440.0 * 2f32.powf((grundton.clamp(0, 127) as f32 - 69.0) / 12.0);
    basis * 2f32.powf(stufe as f32 / 7.0)
}

/// Sechs feste Thai-Motive: drei- und fünftönige Auf-/Absteiger sowie je ein
/// „3+1“-Motiv mit Sprung. Sie brauchen keine Audiodatei — der Kreis-Tab
/// synthetisiert daraus PCM (7-TET) und bietet sie wie analysierte Motive an.
pub fn thai_motive() -> Vec<Motiv> {
    // Pentatonische Ausschnitte aus den sieben Stufen (0..7) plus die Oktave.
    // Auf-/Absteiger stehen paarweise, damit die Umkehrung offensichtlich ist.
    const STUFEN: [&[i32]; 6] = [
        &[0, 2, 4],
        &[4, 2, 0],
        &[0, 2, 4, 7],
        &[7, 4, 2, 0],
        &[0, 2, 4, 6, 8],
        &[8, 6, 4, 2, 0],
    ];
    STUFEN
        .iter()
        .map(|stufen| Motiv {
            noten: stufen
                .iter()
                .map(|&stufe| thai_midi(stufe, THAI_GRUNDTON))
                .collect(),
            starts: vec![0.0],
            // 0,25 s je Ton: die x-Bereiche werden dadurch gleich breit.
            dauer_s: stufen.len() as f32 * 0.25,
        })
        .collect()
}

/// Gleich breite x-Bereiche für `anzahl` Noten (0..1). Für Presets ohne
/// Einzeldauern (Thai, Akkorde) und als Rückfall, wenn eine Bearbeitung keine
/// brauchbaren Zeitmarken mehr übrig lässt.
pub fn gleich_bereiche(anzahl: usize) -> Vec<(f32, f32)> {
    if anzahl == 0 {
        return Vec::new();
    }
    let schritt = 1.0 / anzahl as f32;
    (0..anzahl)
        .map(|i| (i as f32 * schritt, (i + 1) as f32 * schritt))
        .collect()
}

/// Motiv → x-Bereiche (0..1) je Note für den Kreis-Tab.
///
/// Warum gleichmäßig: Das Motiv kennt nur seine Gesamtdauer, keine
/// Einzeldauern. Für den Scan ist die Aufteilung in gleich breite Streifen
/// ausreichend; die erste Ausführung bestimmt die Lage im Stück.
pub fn x_bereiche(motiv: &Motiv, gesamt_s: f32) -> Vec<(f32, f32)> {
    if motiv.noten.is_empty() || !gesamt_s.is_finite() || gesamt_s <= 0.0 {
        return Vec::new();
    }
    let start = motiv.starts.first().copied().unwrap_or(0.0);
    let dauer = motiv.dauer_s.max(0.0);
    let schritt = dauer / motiv.noten.len() as f32;
    (0..motiv.noten.len())
        .map(|i| {
            let von = ((start + i as f32 * schritt) / gesamt_s).clamp(0.0, 1.0);
            let bis = ((start + (i + 1) as f32 * schritt) / gesamt_s).clamp(0.0, 1.0);
            (von.min(bis), von.max(bis))
        })
        .collect()
}

/// Notenabschnitte → x-Bereiche (0..1), ohne gleichmäßige Aufteilung: die
/// erkannten Start-/Endzeiten bestimmen die Breite. Wird für den
/// Aufnahme-Fallback gebraucht (eine Aufnahme ohne wiederkehrendes Motiv).
pub fn x_bereiche_aus_segmenten(
    segmente: &[(i32, f32, f32)],
    gesamt_s: f32,
) -> Vec<(f32, f32)> {
    if !gesamt_s.is_finite() || gesamt_s <= 0.0 {
        return Vec::new();
    }
    segmente
        .iter()
        .filter_map(|&(_, von, bis)| {
            let von = (von / gesamt_s).clamp(0.0, 1.0);
            let bis = (bis / gesamt_s).clamp(0.0, 1.0);
            (bis > von).then_some((von, bis))
        })
        .collect()
}

/// x-Bereiche umkehren: Notenreihenfolge und Lage spiegeln.
///
/// Rückgabe mit dem alten Index, damit der Aufrufer `noten` und `x_bereiche`
/// synchron halten kann (der Editor im Kreis-Tab braucht beides).
pub fn bereiche_umkehren(bereiche: &[(f32, f32)]) -> Vec<(usize, (f32, f32))> {
    bereiche
        .iter()
        .enumerate()
        .rev()
        .map(|(i, &(a, b))| {
            let a = a.clamp(0.0, 1.0);
            let b = b.clamp(0.0, 1.0);
            let (a, b) = (a.min(b), a.max(b));
            (i, ((1.0 - b).clamp(0.0, 1.0), (1.0 - a).clamp(0.0, 1.0)))
        })
        .collect()
}

/// x-Bereiche auf `[von, bis]` zuschneiden und auf 0..1 normieren.
///
/// Angeschnittene Bereiche werden geklemmt, ganz außerhalb liegende fallen
/// weg. So bleiben Noten und x-Bereiche nach `audioedit::zuschneiden` synchron.
pub fn bereiche_zuschneiden(
    bereiche: &[(f32, f32)],
    von: f32,
    bis: f32,
) -> Vec<(usize, (f32, f32))> {
    let von = von.clamp(0.0, 1.0);
    let bis = bis.clamp(0.0, 1.0);
    if !(bis > von) {
        return Vec::new();
    }
    let spanne = bis - von;
    bereiche
        .iter()
        .enumerate()
        .filter_map(|(i, &(a, b))| {
            let a = a.clamp(0.0, 1.0);
            let b = b.clamp(0.0, 1.0);
            let (a, b) = (a.min(b), a.max(b));
            let a = a.max(von);
            let b = b.min(bis);
            (b > a).then_some((i, ((a - von) / spanne, (b - von) / spanne)))
        })
        .collect()
}

/// x-Bereiche nach dem Löschen von `[von, bis)` neu verteilen.
///
/// Noten im Loch fallen weg, Bereiche davor/dahinter rücken zusammen; ein
/// Bereich, der das Loch überlappt, wird zu einem Bereich (die zwei übrigen
/// Teile stoßen nach dem Löschen aneinander). Rückgabe mit altem Index.
pub fn bereiche_loeschen(
    bereiche: &[(f32, f32)],
    von: f32,
    bis: f32,
) -> Vec<(usize, (f32, f32))> {
    let von = von.clamp(0.0, 1.0);
    let bis = bis.clamp(0.0, 1.0);
    if !(bis > von) {
        return bereiche.iter().copied().enumerate().collect();
    }
    let loch = bis - von;
    let rest = 1.0 - loch;
    if rest <= 1e-6 {
        return Vec::new();
    }
    let mut ergebnis = Vec::new();
    for (i, &(a, b)) in bereiche.iter().enumerate() {
        let a = a.clamp(0.0, 1.0);
        let b = b.clamp(0.0, 1.0);
        let (a, b) = (a.min(b), a.max(b));
        if b <= a {
            continue;
        }
        let neu = if b <= von {
            // Ganz vor dem Loch: nur die kürzere Gesamtdauer staucht.
            Some((a / rest, b / rest))
        } else if a >= bis {
            // Ganz dahinter: um die Lochbreite nach vorn.
            Some(((a - loch) / rest, (b - loch) / rest))
        } else if a < von && b > bis {
            // Beide Seiten bleiben und stoßen zusammen.
            Some((a / rest, (b - loch) / rest))
        } else if a < von {
            // Ende liegt im Loch.
            Some((a / rest, von / rest))
        } else if b > bis {
            // Anfang liegt im Loch.
            Some((von / rest, (b - loch) / rest))
        } else {
            None // vollständig im Loch
        };
        if let Some((na, nb)) = neu {
            let na = na.clamp(0.0, 1.0);
            let nb = nb.clamp(0.0, 1.0);
            if nb > na {
                ergebnis.push((i, (na, nb)));
            }
        }
    }
    ergebnis
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 8_000;

    /// Sinusfolge: (Frequenz in Hz, Dauer in s), ohne Pausen aneinander.
    fn sinusfolge(noten: &[(f32, f32)]) -> Vec<f32> {
        let mut pcm = Vec::new();
        for &(freq, dauer) in noten {
            let n = (dauer * RATE as f32) as usize;
            for i in 0..n {
                pcm.push((2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32).sin() * 0.6);
            }
        }
        pcm
    }

    #[test]
    fn tonhoehen_findet_drei_toene() {
        let pcm = sinusfolge(&[(440.0, 0.2), (494.0, 0.2), (523.0, 0.2)]);
        let spur = tonhoehen(&pcm, RATE);
        assert_eq!(spur.len(), 12, "0,6 s / 50 ms = 12 Fenster");
        for (i, (_, freq)) in spur.iter().enumerate() {
            let soll = match i / 4 {
                0 => 440.0,
                1 => 494.0,
                _ => 523.0,
            };
            assert!(
                (freq - soll).abs() < 12.0,
                "Fenster {i}: {freq:.1} Hz statt {soll:.1} Hz"
            );
        }
        // Zeiten sind aufsteigend und beginnen bei 0.
        assert_eq!(spur[0].0, 0.0);
        assert!(spur.windows(2).all(|w| w[1].0 > w[0].0));
    }

    #[test]
    fn stille_ergibt_keine_traege_und_keine_motive() {
        let pcm = vec![0.0f32; RATE as usize];
        let spur = tonhoehen(&pcm, RATE);
        assert!(!spur.is_empty());
        assert!(spur.iter().all(|(_, f)| *f == 0.0), "{spur:?}");
        let noten: Vec<(f32, i32)> = spur.iter().map(|(t, f)| (*t, midi(*f))).collect();
        assert!(finde_motive(&noten, 3).is_empty());
    }

    #[test]
    fn midi_rundet_auf_die_naechste_note() {
        assert_eq!(midi(0.0), 0);
        assert_eq!(midi(-5.0), 0);
        assert_eq!(midi(440.0), 69);
        assert_eq!(midi(494.0), 71);
        assert_eq!(midi(523.0), 72);
        assert_eq!(midi(880.0), 81);
    }

    #[test]
    fn motiv_wird_wiedererkannt() {
        let pcm = sinusfolge(&[
            (440.0, 0.2),
            (494.0, 0.2),
            (523.0, 0.2),
            (440.0, 0.2),
            (494.0, 0.2),
            (523.0, 0.2),
        ]);
        let spur = tonhoehen(&pcm, RATE);
        let noten: Vec<(f32, i32)> = spur.iter().map(|(t, f)| (*t, midi(*f))).collect();
        let motive = finde_motive(&noten, 3);
        assert_eq!(
            motive.len(),
            1,
            "genau ein Motiv, keine Drehungen: {motive:?}"
        );
        let motiv = &motive[0];
        assert_eq!(motiv.noten, vec![69, 71, 72]);
        assert_eq!(motiv.starts.len(), 2);
        assert!((motiv.starts[0] - 0.0).abs() < 0.06);
        assert!((motiv.starts[1] - 0.6).abs() < 0.1);
        assert!((motiv.dauer_s - 0.6).abs() < 0.1, "dauer {}", motiv.dauer_s);
    }

    #[test]
    fn dreifache_wiederholung_ist_ein_motiv() {
        let mut noten: Vec<(f32, i32)> = Vec::new();
        for runde in 0..3 {
            for (k, note) in [69, 76, 81].iter().enumerate() {
                noten.push(((runde * 3 + k) as f32 * 0.2, *note));
            }
        }
        let motive = finde_motive(&noten, 3);
        assert_eq!(motive.len(), 1, "keine abgeschnittene Drehung: {motive:?}");
        assert_eq!(motive[0].noten, vec![69, 76, 81]);
        assert_eq!(motive[0].starts.len(), 3);
    }

    #[test]
    fn einmalige_folge_ist_kein_motiv() {
        let noten: Vec<(f32, i32)> = (0..6).map(|i| (i as f32 * 0.2, 60 + i * 2)).collect();
        assert!(finde_motive(&noten, 3).is_empty());
        // Zwei Vorkommen derselben dreinotigen Folge reichen.
        let mut doppelt = noten.clone();
        doppelt.extend((0..3).map(|i| (1.2 + i as f32 * 0.2, 60 + i * 2)));
        let motive = finde_motive(&doppelt, 3);
        assert_eq!(motive.len(), 1, "{motive:?}");
        assert_eq!(motive[0].noten, vec![60, 62, 64]);
        assert_eq!(motive[0].starts.len(), 2);
    }

    #[test]
    fn x_bereiche_sind_aufsteigend_und_im_bereich() {
        let motiv = Motiv {
            noten: vec![60, 64, 67],
            starts: vec![1.0, 4.0],
            dauer_s: 0.6,
        };
        let bereiche = x_bereiche(&motiv, 6.0);
        assert_eq!(bereiche.len(), 3);
        assert!((bereiche[0].0 - 1.0 / 6.0).abs() < 1e-5, "{bereiche:?}");
        assert!((bereiche[2].1 - 1.6 / 6.0).abs() < 1e-5, "{bereiche:?}");
        for (von, bis) in &bereiche {
            assert!((0.0..=1.0).contains(von) && (0.0..=1.0).contains(bis));
            assert!(von <= bis);
        }
        for w in bereiche.windows(2) {
            assert!(w[0].1 <= w[1].0 + 1e-6, "nicht aufsteigend: {bereiche:?}");
        }
        // Leere/ungültige Gesamtdauer ergibt keine Bereiche.
        assert!(x_bereiche(&motiv, 0.0).is_empty());
    }

    #[test]
    fn x_bereiche_klemmen_ans_ende() {
        let motiv = Motiv {
            noten: vec![60, 64, 67],
            starts: vec![5.9],
            dauer_s: 0.6,
        };
        let bereiche = x_bereiche(&motiv, 6.0);
        assert_eq!(bereiche.len(), 3);
        assert!((bereiche[2].1 - 1.0).abs() < 1e-6);
        assert!(bereiche.iter().all(|(a, b)| a <= b));
    }

    #[test]
    fn thai_motive_sind_sieben_ton_gleichschwebung() {
        let motive = thai_motive();
        assert_eq!(motive.len(), 6);
        assert_eq!(motive.len(), THAI_NAMEN.len());
        let laengen: Vec<usize> = motive.iter().map(|m| m.noten.len()).collect();
        assert_eq!(laengen, vec![3, 3, 4, 4, 5, 5]);

        // Auf- und Absteiger sind exakte Umkehrungen.
        for paar in motive.chunks(2) {
            let mut umgekehrt = paar[0].noten.clone();
            umgekehrt.reverse();
            assert_eq!(umgekehrt, paar[1].noten, "kein Umkehrer: {paar:?}");
        }

        // 7-TET: eine Stufe ist 1200/7 Cent; die MIDI-Näherung darf davon
        // höchstens einen halben Halbton abweichen.
        assert!((THAI_CENT - 171.428_57).abs() < 1e-3, "{THAI_CENT}");
        assert_eq!(thai_midi(0, THAI_GRUNDTON), 60);
        assert_eq!(thai_midi(2, THAI_GRUNDTON), 63);
        assert_eq!(thai_midi(7, THAI_GRUNDTON), 72);
        let verhaeltnis = thai_frequenz(1, THAI_GRUNDTON) / thai_frequenz(0, THAI_GRUNDTON);
        let cent = 1200.0 * verhaeltnis.log2();
        assert!((cent - THAI_CENT).abs() < 0.01, "{cent} Cent");

        // „3+1“ endet mit einem Sprung (mindestens vier Halbtöne).
        for motiv in [&motive[2], &motive[3]] {
            assert!(
                motiv
                    .noten
                    .windows(2)
                    .any(|w| (w[1] - w[0]).abs() >= 4),
                "kein Sprung: {:?}",
                motiv.noten
            );
        }
    }

    #[test]
    fn thai_motive_x_bereiche_sind_aufsteigend_im_bereich() {
        for motiv in thai_motive() {
            let bereiche = x_bereiche(&motiv, motiv.dauer_s);
            assert_eq!(bereiche.len(), motiv.noten.len());
            assert!((bereiche[0].0 - 0.0).abs() < 1e-6, "{bereiche:?}");
            assert!(
                (bereiche.last().unwrap().1 - 1.0).abs() < 1e-6,
                "{bereiche:?}"
            );
            for (von, bis) in &bereiche {
                assert!((0.0..=1.0).contains(von) && (0.0..=1.0).contains(bis));
                assert!(von < bis, "leerer Bereich: {bereiche:?}");
            }
            for w in bereiche.windows(2) {
                assert!(w[0].1 <= w[1].0 + 1e-6, "nicht aufsteigend: {bereiche:?}");
            }
        }
    }

    #[test]
    fn noten_segmente_liefern_start_und_ende() {
        let pcm = sinusfolge(&[(440.0, 0.2), (494.0, 0.2), (523.0, 0.2)]);
        let segmente = noten_segmente(&pcm, RATE);
        assert_eq!(segmente.len(), 3, "{segmente:?}");
        assert_eq!(
            segmente.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
            vec![69, 71, 72]
        );
        assert!((segmente[0].1 - 0.0).abs() < 0.06);
        // Das Ende der letzten Note schätzt `abschnitte` aus dem Median-
        // abstand; es liegt deshalb eher zu weit hinten als zu kurz.
        assert!(segmente[2].2 > segmente[2].1);
        let bereiche = x_bereiche_aus_segmenten(&segmente, 0.6);
        assert_eq!(bereiche.len(), 3);
        assert!((bereiche[0].0 - 0.0).abs() < 1e-6);
        assert!((bereiche[2].1 - 1.0).abs() < 1e-6);
        assert!(bereiche.windows(2).all(|w| w[0].1 <= w[1].0 + 1e-6));
    }

    #[test]
    fn bereiche_zuschneiden_normiert_und_behaelt_indizes() {
        let bereiche = [
            (0.0, 0.2),
            (0.2, 0.4),
            (0.4, 0.6),
            (0.6, 0.8),
            (0.8, 1.0),
        ];
        let neu = bereiche_zuschneiden(&bereiche, 0.2, 0.7);
        assert_eq!(neu.len(), 3, "{neu:?}");
        assert_eq!(neu[0].0, 1);
        assert!(((neu[0].1).0 - 0.0).abs() < 1e-6);
        assert!(((neu[0].1).1 - 0.4).abs() < 1e-6);
        assert_eq!(neu[1].0, 2);
        assert!(((neu[1].1).0 - 0.4).abs() < 1e-6);
        assert!(((neu[1].1).1 - 0.8).abs() < 1e-6);
        assert_eq!(neu[2].0, 3);
        assert!(((neu[2].1).0 - 0.8).abs() < 1e-6);
        assert!(((neu[2].1).1 - 1.0).abs() < 1e-6);
        // Ungültiger/leerer Bereich ändert nichts.
        assert!(bereiche_zuschneiden(&bereiche, 0.5, 0.5).is_empty());
    }

    #[test]
    fn bereiche_loeschen_rueckt_zusammen() {
        let bereiche = [
            (0.0, 0.2),
            (0.2, 0.4),
            (0.4, 0.6),
            (0.6, 0.8),
            (0.8, 1.0),
        ];
        let neu = bereiche_loeschen(&bereiche, 0.4, 0.6);
        assert_eq!(neu.len(), 4, "{neu:?}");
        // 0,2 / 0,8 = 0,25 usw.; der geloeschte Bereich faellt weg.
        assert_eq!(neu.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 1, 3, 4]);
        assert!(((neu[1].1).1 - 0.5).abs() < 1e-6, "{neu:?}");
        assert!(((neu[2].1).0 - 0.5).abs() < 1e-6, "{neu:?}");
        assert!(((neu[3].1).1 - 1.0).abs() < 1e-6, "{neu:?}");

        // Ein Bereich, der das Loch überlappt, wird zusammengezogen.
        let neu = bereiche_loeschen(&[(0.3, 0.7)], 0.4, 0.6);
        assert_eq!(neu.len(), 1, "{neu:?}");
        assert!(((neu[0].1).0 - 0.375).abs() < 1e-6, "{neu:?}");
        assert!(((neu[0].1).1 - 0.625).abs() < 1e-6, "{neu:?}");

        // Alles löschen ergibt keine Bereiche; leerer Bereich ist ein No-op.
        assert!(bereiche_loeschen(&bereiche, 0.0, 1.0).is_empty());
        assert_eq!(bereiche_loeschen(&bereiche, 0.3, 0.3).len(), 5);
    }

    #[test]
    fn bereiche_umkehren_spiegelt_und_dreht() {
        let bereiche = [(0.0, 0.2), (0.3, 0.5), (0.8, 1.0)];
        let neu = bereiche_umkehren(&bereiche);
        assert_eq!(
            neu.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![2, 1, 0]
        );
        assert!(((neu[0].1).0 - 0.0).abs() < 1e-6, "{neu:?}");
        assert!(((neu[0].1).1 - 0.2).abs() < 1e-6, "{neu:?}");
        assert!(((neu[1].1).0 - 0.5).abs() < 1e-6, "{neu:?}");
        assert!(((neu[1].1).1 - 0.7).abs() < 1e-6, "{neu:?}");
        assert!(((neu[2].1).0 - 0.8).abs() < 1e-6, "{neu:?}");
        assert!(((neu[2].1).1 - 1.0).abs() < 1e-6, "{neu:?}");
    }
}
