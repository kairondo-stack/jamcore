//! Slicing für geladenes Audio: automatisch **gleichmäßig** oder nach
//! **Transienten/Energie**, danach von Hand nachbearbeitbar.
//!
//! Ein [`Slice`] ist ein Bereich `von` … `bis` (Sample-Indizes, `bis`
//! exklusiv). Die Liste ist immer sortiert und lückenlos; es gibt immer
//! mindestens einen Slice (leere oder zu leise Probe → ein Slice über die
//! ganze Länge), damit die App nie ohne Auswahl dasteht.
//!
//! Bei [`Schnittmodus::Transienten`] bestimmen die gefundenen Onsets die
//! **Anfänge** der Slices; der Bereich vor dem ersten Onset gehört zu keinem
//! Slice (typisches Slicer-Verhalten). Gibt es keine Onsets, entsteht ein
//! Slice über die ganze Länge.

/// Wie die automatischen Schnitte gelegt werden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schnittmodus {
    /// `anzahl` gleich lange Slices über die ganze Länge.
    Gleichmaessig,
    /// Onsets aus dem Energie-Fluss, auf `anzahl` begrenzt (die stärksten).
    Transienten,
}

/// Ein Schnitt: Sample-Index `von` bis `bis` (exklusiv).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    pub von: usize,
    pub bis: usize,
}

/// Obergrenze der Ziel-Anzahl (mehr Slices sind auf dem Handy nicht sinnvoll).
const ANZAHL_MAX: usize = 64;

/// Analyse-Fenster der Hüllkurve in Millisekunden.
const FENSTER_MS: u32 = 10;

/// So viele Fenster der Umgebung bilden das lokale Mittel der Schwelle.
const UMGEBUNG: usize = 8;

/// Ab dieser RMS gilt ein Fenster als Stille (darunter keine Onsets).
const STILLE_RMS: f32 = 1e-3;

/// Mindestabstand zweier Onsets in Fenstern.
const MIN_ABSTAND: usize = 2;

/// Ziel-Anzahl klemmen (1 … 64).
pub fn klemme_anzahl(anzahl: usize) -> usize {
    anzahl.clamp(1, ANZAHL_MAX)
}

/// Schnitte für eine Probe.
///
/// Es gibt immer mindestens einen Slice. `empfindlichkeit` (0 … 1) senkt bei
/// [`Schnittmodus::Transienten`] die Schwelle: 1 = empfindlich (Faktor 1,2),
/// 0 = streng (Faktor 4,0).
pub fn schnitte(
    proben: &[f32],
    rate: u32,
    modus: Schnittmodus,
    anzahl: usize,
    empfindlichkeit: f32,
) -> Vec<Slice> {
    let ziel = klemme_anzahl(anzahl);
    match modus {
        Schnittmodus::Gleichmaessig => gleichmaessig(proben.len(), ziel),
        Schnittmodus::Transienten => transienten(proben, rate, ziel, empfindlichkeit),
    }
}

/// `ziel` gleich lange Slices; der letzte endet am Ende der Probe. Sind
/// weniger Samples als Slices da, wird kein Slice leer.
fn gleichmaessig(len: usize, ziel: usize) -> Vec<Slice> {
    if len == 0 {
        return vec![Slice { von: 0, bis: 0 }];
    }
    let n = ziel.clamp(1, len);
    (0..n)
        .map(|i| Slice {
            von: i * len / n,
            bis: (i + 1) * len / n,
        })
        .collect()
}

/// Onsets aus dem Energie-Fluss; sie sind die Anfänge der Slices.
fn transienten(proben: &[f32], rate: u32, ziel: usize, empfindlichkeit: f32) -> Vec<Slice> {
    let gesamt = proben.len();
    if gesamt == 0 {
        return vec![Slice { von: 0, bis: 0 }];
    }
    // Ohne Abtastrate gibt es keine Zeitachse für die Fenster.
    if rate == 0 {
        return vec![Slice { von: 0, bis: gesamt }];
    }

    // Fensterlänge: FENSTER_MS Millisekunden Abtastrate.
    let fenster = (rate as usize * FENSTER_MS as usize / 1000).max(1);
    let huelle = huelle(proben, fenster);
    let onsets = onsets_aus_huelle(&huelle, ziel, empfindlichkeit);
    slices_ab_onsets(&onsets, fenster, gesamt)
}

/// RMS-Hüllkurve über nicht überlappende Fenster von ~10 ms.
fn huelle(proben: &[f32], fenster: usize) -> Vec<f32> {
    let mut huelle = Vec::with_capacity(proben.len().div_ceil(fenster));
    let mut von = 0;
    while von < proben.len() {
        let bis = (von + fenster).min(proben.len());
        let summe: f32 = proben[von..bis].iter().map(|s| s * s).sum();
        huelle.push((summe / (bis - von) as f32).sqrt());
        von = bis;
    }
    huelle
}

/// Fenster-Indizes der Onsets: lokale Maxima des positiven Energie-Flusses
/// über einer adaptiven Schwelle, in Zeitreihenfolge, auf `ziel` begrenzt
/// (die stärksten bleiben).
fn onsets_aus_huelle(huelle: &[f32], ziel: usize, empfindlichkeit: f32) -> Vec<usize> {
    // Empfindlichkeit 1 senkt die Schwelle (Faktor 1,2), 0 hebt sie (Faktor 4,0).
    let faktor = 1.2 + (1.0 - empfindlichkeit.clamp(0.0, 1.0)) * 2.8;

    let mut fluss = vec![0.0f32; huelle.len()];
    for i in 0..huelle.len() {
        let vorher = if i == 0 { 0.0 } else { huelle[i - 1] };
        fluss[i] = (huelle[i] - vorher).max(0.0);
    }

    let mut kandidaten: Vec<(usize, f32)> = Vec::new();
    for i in 0..fluss.len() {
        let von = i.saturating_sub(UMGEBUNG);
        let mittel = if von == i {
            0.0
        } else {
            huelle[von..i].iter().sum::<f32>() / (i - von) as f32
        };
        let schwelle = (mittel * faktor).max(STILLE_RMS);
        // Stille und kaputte Samples (NaN) bilden keine Onsets.
        if !fluss[i].is_finite() || fluss[i] <= schwelle {
            continue;
        }
        // Nur der Gipfel zählt (ein Klick kann zwei Fenster anheben).
        let links = i == 0 || fluss[i] >= fluss[i - 1];
        let rechts = i + 1 >= fluss.len() || fluss[i] > fluss[i + 1];
        if links && rechts {
            kandidaten.push((i, fluss[i]));
        }
    }

    // Dicht beieinanderliegende Kandidaten: nur den stärksten behalten.
    let mut onsets: Vec<(usize, f32)> = Vec::new();
    for (i, staerke) in kandidaten {
        let dicht = onsets
            .last()
            .map(|letzter| i - letzter.0 < MIN_ABSTAND)
            .unwrap_or(false);
        if dicht {
            let letzter = onsets.last_mut().expect("dicht ohne Vorgänger");
            if staerke > letzter.1 {
                *letzter = (i, staerke);
            }
        } else {
            onsets.push((i, staerke));
        }
    }

    // Auf die Ziel-Anzahl begrenzen: die stärksten behalten, in Zeitreihenfolge.
    if onsets.len() > ziel {
        onsets.sort_by(|a, b| b.1.total_cmp(&a.1));
        onsets.truncate(ziel);
        onsets.sort_by_key(|(i, _)| *i);
    }
    onsets.into_iter().map(|(i, _)| i).collect()
}

/// Slices ab den Onsets: jeder Onset beginnt einen Slice, der letzte endet am
/// Ende der Probe. Ohne Onsets ein Slice über die ganze Länge.
fn slices_ab_onsets(onsets: &[usize], fenster: usize, gesamt: usize) -> Vec<Slice> {
    let mut slices = Vec::with_capacity(onsets.len().max(1));
    for (k, onset) in onsets.iter().enumerate() {
        let von = (onset * fenster).min(gesamt);
        let bis = onsets
            .get(k + 1)
            .map(|naechster| naechster * fenster)
            .unwrap_or(gesamt)
            .min(gesamt);
        if bis > von {
            slices.push(Slice { von, bis });
        }
    }
    if slices.is_empty() {
        slices.push(Slice { von: 0, bis: gesamt });
    }
    slices
}

/// Schnitt (Anfang der Slice `index`) auf `grenze` setzen.
///
/// Die Liste bleibt sortiert und lückenlos: die vorherige Slice endet mit, die
/// eigene bleibt mindestens ein Sample lang. `grenze` wird auf den Bereich der
/// Nachbarn geklemmt; ein Index außerhalb ändert nichts.
// Die Signatur bleibt bewusst `&mut Vec<Slice>` (so hält die App die Liste).
#[allow(clippy::ptr_arg)]
pub fn setze(schnitte: &mut Vec<Slice>, index: usize, grenze: usize) {
    if schnitte.is_empty() {
        return;
    }
    schnitte.sort_by_key(|s| s.von);
    if index >= schnitte.len() {
        return;
    }
    let bis = schnitte[index].bis;
    let min = if index == 0 {
        0
    } else {
        schnitte[index - 1].von.saturating_add(1)
    };
    // Mindestens ein Sample muss der Slice bleiben.
    let neu = klemme(grenze, min, bis.saturating_sub(1));
    if index > 0 {
        schnitte[index - 1].bis = neu;
    }
    schnitte[index].von = neu;
    ausrichten(schnitte);
}

/// Schnitt nach der Slice `index` (deren Ende) auf `grenze` setzen.
///
/// `gesamt` ist die Länge der Probe und begrenzt das Ende der letzten Slice.
/// Die Liste bleibt sortiert und lückenlos; ein Index außerhalb ändert nichts.
pub fn verschiebe(schnitte: &mut [Slice], index: usize, grenze: usize, gesamt: usize) {
    if index >= schnitte.len() {
        return;
    }
    let von = schnitte[index].von;
    let max = if index + 1 < schnitte.len() {
        schnitte[index + 1].von
    } else {
        gesamt
    }
    .min(gesamt);
    let neu = klemme(grenze, von.saturating_add(1), max);
    schnitte[index].bis = neu;
    if index + 1 < schnitte.len() {
        schnitte[index + 1].von = neu;
    }
}

/// Slice `index` löschen; ihr Bereich geht an die vorherige Slice, bei der
/// ersten an die nächste. Die letzte Slice bleibt stehen (die Probe muss
/// abgedeckt bleiben), ein Index außerhalb ändert nichts.
pub fn loesche(schnitte: &mut Vec<Slice>, index: usize) {
    if schnitte.len() <= 1 || index >= schnitte.len() {
        return;
    }
    let entfernt = schnitte.remove(index);
    if index > 0 {
        schnitte[index - 1].bis = entfernt.bis;
    } else {
        schnitte[0].von = entfernt.von;
    }
}

/// `grenze` auf `min … max` klemmen, ohne bei `min > max` zu panicken.
fn klemme(grenze: usize, min: usize, max: usize) -> usize {
    if min >= max {
        max
    } else {
        grenze.clamp(min, max)
    }
}

/// Liste lückenlos und aufsteigend machen (falls sie es nicht schon ist).
fn ausrichten(schnitte: &mut [Slice]) {
    for i in 0..schnitte.len() {
        if schnitte[i].bis < schnitte[i].von {
            schnitte[i].bis = schnitte[i].von;
        }
        if i + 1 < schnitte.len() && schnitte[i + 1].von < schnitte[i].bis {
            schnitte[i + 1].von = schnitte[i].bis;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Klicks in Stille: je `(Zeit in Sekunden, Stärke)` ein 5-ms-Ausschlag.
    fn klicks(rate: u32, dauer_s: f32, positionen: &[(f32, f32)]) -> Vec<f32> {
        let n = (rate as f32 * dauer_s) as usize;
        let mut v = vec![0.0f32; n];
        let laenge = (rate as f32 * 0.005) as usize;
        for &(t, staerke) in positionen {
            let start = (t * rate as f32) as usize;
            for k in 0..laenge {
                if start + k < n {
                    v[start + k] = staerke;
                }
            }
        }
        v
    }

    #[test]
    fn gleichmaessig_teilt_in_n() {
        let proben = vec![0.0f32; 4000];
        let s = schnitte(&proben, 16000, Schnittmodus::Gleichmaessig, 4, 0.5);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0], Slice { von: 0, bis: 1000 });
        assert_eq!(s[3], Slice { von: 3000, bis: 4000 });
        for (i, w) in s.windows(2).enumerate() {
            assert_eq!(w[0].bis, w[1].von, "lückenlos bei {i}");
            assert_eq!(
                w[1].bis - w[1].von,
                w[0].bis - w[0].von,
                "gleich lang bei {i}"
            );
        }
        assert!(s.iter().all(|x| x.bis > x.von), "keine leeren Slices");
    }

    #[test]
    fn gleichmaessig_letzter_geht_bis_ende() {
        // Länge nicht durch die Anzahl teilbar.
        let proben = vec![0.0f32; 4001];
        let s = schnitte(&proben, 16000, Schnittmodus::Gleichmaessig, 3, 0.5);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].von, 0, "Anfang");
        assert_eq!(s.last().unwrap().bis, proben.len(), "letzter bis Ende");
        assert_eq!(s[2].bis, 4001);
        // Mehr Slices als Samples: trotzdem lückenlos und nicht leer.
        let kurz = vec![0.0f32; 3];
        let s = schnitte(&kurz, 16000, Schnittmodus::Gleichmaessig, 8, 0.5);
        assert_eq!(s.len(), 3, "mehr Slices als Samples ergeben keine leeren");
        assert_eq!(s.last().unwrap().bis, 3);
    }

    #[test]
    fn transienten_findet_klicks() {
        let rate = 16000u32;
        let positionen = [(0.2f32, 0.9f32), (0.6, 0.9), (1.0, 0.9), (1.4, 0.9)];
        let proben = klicks(rate, 2.0, &positionen);
        let s = schnitte(&proben, rate, Schnittmodus::Transienten, 8, 0.5);
        assert_eq!(s.len(), 4, "vier Klicks → vier Slices: {s:?}");
        for (i, (t, _)) in positionen.iter().enumerate() {
            let start = (t * rate as f32) as usize;
            assert!(
                s[i].von <= start && start < s[i].bis,
                "Klick {i} liegt in Slice {i}: {:?}",
                s[i]
            );
        }
        assert_eq!(s.last().unwrap().bis, proben.len(), "letzter bis Ende");
    }

    #[test]
    fn transienten_auf_anzahl_begrenzt() {
        let rate = 16000u32;
        let stark = [0.2f32, 0.5, 0.8, 1.1];
        let schwach = [0.35f32, 0.65, 0.95, 1.25];
        let mut positionen: Vec<(f32, f32)> = stark.iter().map(|p| (*p, 0.9)).collect();
        positionen.extend(schwach.iter().map(|p| (*p, 0.25)));
        let proben = klicks(rate, 1.6, &positionen);

        let s = schnitte(&proben, rate, Schnittmodus::Transienten, 4, 0.5);
        assert_eq!(s.len(), 4, "acht Klicks, Ziel 4 → vier Slices: {s:?}");
        for slice in &s {
            let t = slice.von as f32 / rate as f32;
            assert!(
                stark.iter().any(|p| (t - p).abs() < 0.03),
                "behalten werden die stärksten Klicks, Slice beginnt bei {t}"
            );
        }
    }

    #[test]
    fn leises_signal_bleibt_ein_slice() {
        let rate = 16000u32;
        // Sehr leises Signal (unter der Stille-Schwelle) mit leisem Klick.
        let n = rate as usize;
        let mut proben: Vec<f32> = (0..n)
            .map(|i| 0.0008 * (i as f32 / rate as f32 * 880.0 * 2.0 * std::f32::consts::PI).sin())
            .collect();
        let start = (0.5 * rate as f32) as usize;
        for k in 0..(rate as usize / 200) {
            if start + k < n {
                proben[start + k] += 0.0006;
            }
        }
        for modus in [Schnittmodus::Gleichmaessig, Schnittmodus::Transienten] {
            let s = schnitte(&proben, rate, modus, 1, 0.5);
            assert_eq!(s.len(), 1, "{modus:?}: leises Signal bleibt ein Slice");
            assert_eq!(s[0], Slice { von: 0, bis: proben.len() });
        }
        // Kürzer als ein Analyse-Fenster: auch ein Slice.
        let kurz = vec![0.5f32; 5];
        let s = schnitte(&kurz, rate, Schnittmodus::Transienten, 4, 0.5);
        assert_eq!(s, vec![Slice { von: 0, bis: 5 }]);
    }

    #[test]
    fn anzahl_wird_geklemmt() {
        assert_eq!(klemme_anzahl(0), 1);
        assert_eq!(klemme_anzahl(1), 1);
        assert_eq!(klemme_anzahl(32), 32);
        assert_eq!(klemme_anzahl(64), 64);
        assert_eq!(klemme_anzahl(65), 64);
        assert_eq!(klemme_anzahl(usize::MAX), 64);
        let proben = vec![0.0f32; 6400];
        assert_eq!(
            schnitte(&proben, 16000, Schnittmodus::Gleichmaessig, 0, 0.5).len(),
            1
        );
        assert_eq!(
            schnitte(&proben, 16000, Schnittmodus::Gleichmaessig, 1000, 0.5).len(),
            64
        );
        assert_eq!(
            schnitte(&proben, 16000, Schnittmodus::Transienten, 1000, 0.5).len(),
            1,
            "Stille bleibt ein Slice"
        );
    }

    #[test]
    fn leere_probe_und_rate_null() {
        for modus in [Schnittmodus::Gleichmaessig, Schnittmodus::Transienten] {
            assert_eq!(
                schnitte(&[], 44100, modus, 8, 0.5),
                vec![Slice { von: 0, bis: 0 }],
                "{modus:?}: leere Probe"
            );
        }
        // Ohne Abtastrate kann nur gleichmäßig geteilt werden.
        let proben = vec![0.5f32; 100];
        assert_eq!(
            schnitte(&proben, 0, Schnittmodus::Gleichmaessig, 2, 0.5).len(),
            2
        );
        assert_eq!(
            schnitte(&proben, 0, Schnittmodus::Transienten, 8, 0.5),
            vec![Slice { von: 0, bis: 100 }],
            "rate 0 → ein Slice"
        );
    }

    #[test]
    fn hand_schnitt_sortiert() {
        // Unsortierte Liste: `setze` sortiert, schließt Lücken und klemmt.
        let mut s = vec![
            Slice {
                von: 300,
                bis: 400,
            },
            Slice { von: 0, bis: 100 },
            Slice {
                von: 100,
                bis: 300,
            },
        ];
        setze(&mut s, 2, 250);
        assert_eq!(
            s,
            vec![
                Slice { von: 0, bis: 100 },
                Slice {
                    von: 100,
                    bis: 250
                },
                Slice {
                    von: 250,
                    bis: 400
                },
            ]
        );
        // Klemmen: mindestens ein Sample Abstand zum Vorgänger …
        setze(&mut s, 1, 0);
        assert_eq!(s[1].von, 1, "nicht auf den Vorgänger");
        // … und höchstens bis ans eigene Ende (ein Sample bleibt übrig).
        setze(&mut s, 1, 9999);
        assert_eq!(s[1].von, 249);
        assert!(s[1].bis > s[1].von, "Slice bleibt nicht leer");
        // Anfang der ersten Slice (Trimmen) und Index außerhalb.
        setze(&mut s, 0, 40);
        assert_eq!(
            s[0],
            Slice {
                von: 40,
                bis: 249
            }
        );
        setze(&mut s, 7, 5);
        assert_eq!(s[0].von, 40, "Index außerhalb ändert nichts");
        let mut leer: Vec<Slice> = Vec::new();
        setze(&mut leer, 0, 5);
        assert!(leer.is_empty(), "leere Liste bleibt leer");
    }

    #[test]
    fn schnitt_loeschen_und_verschieben() {
        let mut s = vec![
            Slice {
                von: 0,
                bis: 1000,
            },
            Slice {
                von: 1000,
                bis: 2000,
            },
            Slice {
                von: 2000,
                bis: 3000,
            },
            Slice {
                von: 3000,
                bis: 4000,
            },
        ];
        // Löschen: der Bereich geht an die vorherige Slice.
        loesche(&mut s, 1);
        assert_eq!(
            s,
            vec![
                Slice {
                    von: 0,
                    bis: 2000
                },
                Slice {
                    von: 2000,
                    bis: 3000
                },
                Slice {
                    von: 3000,
                    bis: 4000
                },
            ]
        );
        // Erste löschen: die nächste übernimmt den Anfang.
        loesche(&mut s, 0);
        assert_eq!(
            s,
            vec![
                Slice {
                    von: 0,
                    bis: 3000
                },
                Slice {
                    von: 3000,
                    bis: 4000
                },
            ]
        );
        // Verschieben: Schnitt nach Slice 0 auf 2500.
        verschiebe(&mut s, 0, 2500, 4000);
        assert_eq!(
            s,
            vec![
                Slice {
                    von: 0,
                    bis: 2500
                },
                Slice {
                    von: 2500,
                    bis: 4000
                },
            ]
        );
        // Klemmen: nicht über das Gesamt-Ende …
        verschiebe(&mut s, 1, 9999, 4000);
        assert_eq!(s[1].bis, 4000);
        // … und nicht auf den eigenen Anfang.
        verschiebe(&mut s, 0, 0, 4000);
        assert_eq!(
            s[0],
            Slice {
                von: 0,
                bis: 1
            }
        );
        // Die letzte Slice bleibt stehen, unbekannte Indizes tun nichts.
        loesche(&mut s, 0);
        assert_eq!(s.len(), 1);
        loesche(&mut s, 0);
        assert_eq!(s.len(), 1, "die letzte Slice bleibt");
        verschiebe(&mut s, 5, 10, 4000);
        loesche(&mut s, 9);
        setze(&mut s, 9, 10);
        assert_eq!(s.len(), 1);
    }
}
