//! Kanalvergabe für polyphone Patterns: welche Note klingt auf welchem Kanal?
//!
//! Nutzerwunsch (2026-10-20): Das Sample soll **pro Kanal gleich bleiben** und
//! nicht bei jeder Note wechseln; eine Note bleibt in ihrem Kanal, solange dort
//! Platz ist, und schneidet das vorherige Sample nicht ab. Die hörbare
//! Wiedergabe bleibt polyphon (die App mischt alle Stimmen); die Kanäle sind
//! die Ordnung im Tracker und im `.it`-Export.
//!
//! Reine Rechnung ohne Zeit, Audio oder Android — damit die Regeln ohne Gerät
//! prüfbar sind. Die Reihenfolge der Regeln ist eine Nutzerentscheidung und
//! über [`verteile_mit`] umschaltbar („Sample zuerst“ / „frei zuerst“).

/// Eine Note für die Kanalvergabe: Startschritt, Sample/Instrument, Dauer in
/// Schritten. `dauer = 0` belegt keinen Zeitraum (die Note klingt nicht).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kandidat {
    pub start: u32,
    pub sample: u8,
    pub dauer: u32,
}

/// Verteilt Noten auf `kanaele` (1…64), Reihenfolge = Eingabe.
///
/// Regeln in dieser Reihenfolge:
/// 1. Kanal, der **dasselbe Sample** hat und zur Startzeit **frei** ist
///    (niedrigste Nummer) — so bleibt das Sample im Kanal.
/// 2. irgendein **freier** Kanal (niedrigste Nummer) — kein Übernehmen nötig.
/// 3. sonst der Kanal, dessen letzte Note **am längsten fertig** ist
///    (Ende = `start + dauer`), bei Gleichstand die niedrigste Nummer.
///
/// Ein Kanal gilt als belegt, solange eine seiner Noten klingt
/// (`start <= t < start + dauer`).
pub fn verteile(noten: &[Kandidat], kanaele: u8) -> Vec<u8> {
    verteile_mit(noten, kanaele, true)
}

/// Wie [`verteile`], aber die Reihenfolge der ersten beiden Regeln ist wählbar
/// (Nutzerwunsch „auch nachträglich“): `sample_zuerst = false` nimmt erst den
/// niedrigsten **freien** Kanal und erst danach einen Kanal mit demselben
/// Sample. Die Übernahme (Regel 3) bleibt in beiden Fällen gleich.
pub fn verteile_mit(noten: &[Kandidat], kanaele: u8, sample_zuerst: bool) -> Vec<u8> {
    // Mehr als 64 Kanäle gibt es im Tracker nicht; 0 ist unsinnig, wird aber
    // nicht bestraft (alle Noten landen dann auf 0, siehe `verteile`-Doku).
    let anzahl = usize::from(kanaele.min(64));
    if anzahl == 0 {
        return vec![0; noten.len()];
    }
    // Belegung je Kanal: (start, ende) aller Noten, die dort klingen können.
    let mut belegt: Vec<Vec<(u32, u32)>> = vec![Vec::new(); anzahl];
    // Letzte Note je Kanal: (start, ende, sample). Daraus kommt Regel 1
    // (Sample) und Regel 3 (Ende). „Letzte“ ist die Note mit dem spätesten
    // Start — die Note, die den Kanal zuletzt geprägt hat.
    let mut letzte: Vec<Option<(u32, u32, u8)>> = vec![None; anzahl];
    let mut kanaele_je_note = Vec::with_capacity(noten.len());
    for note in noten {
        let ende = note.start.saturating_add(note.dauer);
        let frei = |kanal: usize| {
            !belegt[kanal]
                .iter()
                .any(|(start, ende)| *start <= note.start && note.start < *ende)
        };
        let gleiches_sample = |kanal: usize| {
            letzte[kanal].is_some_and(|(_, _, sample)| sample == note.sample)
        };
        let freies_gleiches = (0..anzahl).find(|kanal| frei(*kanal) && gleiches_sample(*kanal));
        let freier = (0..anzahl).find(|kanal| frei(*kanal));
        let wahl = if sample_zuerst {
            freies_gleiches.or(freier)
        } else {
            freier.or(freies_gleiches)
        };
        let kanal = wahl.unwrap_or_else(|| {
            // Alle Kanäle klingen: der mit dem kleinsten Ende ist am längsten
            // fertig. `k` als zweites Schlüssel macht den Gleichstand zur
            // niedrigsten Nummer (stabil und ohne Zufall).
            (0..anzahl)
                .min_by_key(|kanal| {
                    let ende = letzte[*kanal].map(|(_, ende, _)| ende).unwrap_or(0);
                    (ende, *kanal)
                })
                .unwrap_or(0)
        });
        kanaele_je_note.push(kanal as u8);
        belegt[kanal].push((note.start, ende));
        if letzte[kanal].is_none_or(|(start, _, _)| note.start >= start) {
            letzte[kanal] = Some((note.start, ende, note.sample));
        }
    }
    kanaele_je_note
}

/// Ordnet eine vorhandene Kanalbelegung neu zu (Nutzer: „auch nachträglich“),
/// ohne die Notenreihenfolge zu ändern: alle 64 Kanäle stehen zur Verfügung.
pub fn neu_verteilen(noten: &[Kandidat]) -> Vec<u8> {
    verteile(noten, 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(start: u32, sample: u8, dauer: u32) -> Kandidat {
        Kandidat {
            start,
            sample,
            dauer,
        }
    }

    #[test]
    fn gleiches_sample_bleibt_im_selben_kanal() {
        // Zwei Noten desselben Samples hintereinander (nicht überlappend):
        // die zweite bleibt im Kanal der ersten, statt einen neuen zu öffnen.
        let noten = [k(0, 7, 1), k(4, 7, 1)];
        assert_eq!(verteile(&noten, 4), vec![0, 0]);
    }

    #[test]
    fn gleichzeitige_noten_gleichen_samples_verschiedene_kanaele() {
        // Überlappen sich zwei Noten desselben Samples, kann nur eine im
        // selben Kanal liegen — die zweite bekommt einen freien Kanal.
        let noten = [k(0, 7, 4), k(0, 7, 4)];
        assert_eq!(verteile(&noten, 4), vec![0, 1]);
    }

    #[test]
    fn freier_kanal_vor_uebernahme() {
        // Kanal 0 klingt noch (Sample 7, 0..8) und hat dasselbe Sample; die
        // Note bei 4 darf ihn nicht übernehmen, solange Kanal 1 frei ist.
        let noten = [k(0, 7, 8), k(4, 7, 8)];
        assert_eq!(verteile(&noten, 4), vec![0, 1]);
    }

    #[test]
    fn uebernahme_nimmt_den_am_laengsten_fertigen_kanal() {
        // Beide Kanäle sind bei Schritt 2 belegt, keiner hat das Sample der
        // neuen Note: Kanal 0 endet bei 4, Kanal 1 bei 8 — Kanal 0 ist eher
        // fertig und wird übernommen.
        let noten = [k(0, 1, 4), k(0, 2, 8), k(2, 3, 4)];
        assert_eq!(verteile(&noten, 2), vec![0, 1, 0]);
    }

    #[test]
    fn uebernahme_bei_gleichstand_niedrigster_kanal() {
        // Beide Enden gleich (8): die niedrigste Nummer gewinnt.
        let noten = [k(0, 1, 8), k(0, 2, 8), k(2, 3, 4)];
        assert_eq!(verteile(&noten, 2), vec![0, 1, 0]);
    }

    #[test]
    fn vierundsechzig_kanaele_und_grenze() {
        // 64 gleichzeitige Noten desselben Samples füllen genau 0…63.
        let noten: Vec<Kandidat> = (0..64).map(|_| k(0, 9, 8)).collect();
        assert_eq!(verteile(&noten, 64), (0..64).collect::<Vec<u8>>());
        // Mehr als 64 Kanäle werden geklemmt: nie ein Kanal über 63.
        let viele: Vec<Kandidat> = (0..80).map(|_| k(0, 9, 8)).collect();
        let kanaele = verteile(&viele, 200);
        assert_eq!(kanaele.len(), 80);
        assert!(kanaele.iter().all(|kanal| *kanal < 64), "{kanaele:?}");
        // Genau 64 Kanäle: die 65. Note verdrängt den am längsten fertigen
        // Kanal (alle Enden gleich -> Kanal 0).
        let eng = vec![k(0, 9, 8); 65];
        assert_eq!(verteile(&eng, 64)[64], 0);
    }

    #[test]
    fn leere_liste_bleibt_leer() {
        assert!(verteile(&[], 8).is_empty());
        assert!(verteile(&[], 0).is_empty());
        assert!(neu_verteilen(&[]).is_empty());
    }

    #[test]
    fn null_kanaele_panikt_nicht() {
        // kanaele = 0 ist unsinnig, darf aber nicht panisch werden: alle
        // Noten landen auf 0 (die App zeigt sie dann in der ersten Spalte).
        let noten = [k(0, 1, 1), k(4, 2, 1), k(8, 3, 1)];
        assert_eq!(verteile(&noten, 0), vec![0, 0, 0]);
        assert_eq!(verteile_mit(&noten, 0, false), vec![0, 0, 0]);
    }

    #[test]
    fn reihenfolge_sample_zuerst_und_frei_zuerst() {
        // Kanal 0 hat Sample 1 (Note 0..1), Kanal 1 hat Sample 7 (0..1). Bei
        // Schritt 8 sind beide frei; die neue Note hat Sample 7.
        let noten = [k(0, 1, 1), k(0, 7, 1), k(8, 7, 1)];
        assert_eq!(
            verteile_mit(&noten, 4, true),
            vec![0, 1, 1],
            "Sample zuerst: Kanal 1 hat dasselbe Sample"
        );
        assert_eq!(
            verteile_mit(&noten, 4, false),
            vec![0, 1, 0],
            "frei zuerst: der niedrigste freie Kanal (0) gewinnt"
        );
        // `verteile` ist die Vorgabe „Sample zuerst“.
        assert_eq!(verteile(&noten, 4), verteile_mit(&noten, 4, true));
    }

    #[test]
    fn neu_verteilen_nutzt_alle_64_kanaele() {
        // Nachträgliches Neuverteilen: gleiche Reihenfolge, aber bis zu 64
        // Kanäle (die Vorgabe von `verteile` bleibt bei der übergebenen Zahl).
        let noten = [k(0, 3, 2), k(0, 3, 2), k(0, 3, 2)];
        assert_eq!(neu_verteilen(&noten), vec![0, 1, 2]);
    }
}
