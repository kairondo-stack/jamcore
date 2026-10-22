//! Audio-Bearbeitung für den Slicer: Umkehren, Löschen, Zuschneiden,
//! Ein-/Ausblenden und Normalisieren auf Mono-PCM.
//!
//! Alle Funktionen arbeiten auf Sample-Indizes `[von, bis)` und klemmen `bis`
//! auf die Pufferlänge. Ein leerer oder umgekehrter Bereich (`von >= bis`)
//! ändert nichts — so kann die Oberfläche einen Bereich ohne Vorprüfung
//! übergeben, ohne dass eine Bearbeitung daneben geht.

/// Bereich auf `[0, len)` klemmen; `None`, wenn danach nichts übrig bleibt.
///
/// Warum zentral: alle Werkzeuge sollen dieselben Grenzen haben (leer,
/// umgekehrt, über das Ende hinaus) — so kann die Oberfläche rohe
/// Fingerkoordinaten übergeben, ohne selbst zu klemmen.
fn klemme(len: usize, von: usize, bis: usize) -> Option<(usize, usize)> {
    let bis = bis.min(len);
    let von = von.min(len);
    (bis > von).then_some((von, bis))
}

/// Kehrt die Proben im Bereich `[von, bis)` um.
pub fn umkehren(pcm: &mut [f32], von: usize, bis: usize) {
    if let Some((von, bis)) = klemme(pcm.len(), von, bis) {
        pcm[von..bis].reverse();
    }
}

/// Entfernt `[von, bis)` und liefert die neue Länge.
///
/// Ein leerer/umgekehrter Bereich löscht nichts — anders als beim Zuschneiden
/// ist „nichts ausgewählt“ hier kein gültiger Wunsch, sondern ein No-op.
pub fn bereich_loeschen(pcm: &mut Vec<f32>, von: usize, bis: usize) -> usize {
    let Some((von, bis)) = klemme(pcm.len(), von, bis) else {
        return pcm.len();
    };
    pcm.drain(von..bis);
    pcm.len()
}

/// Behält nur `[von, bis)` und liefert die neue Länge.
///
/// `von == bis` ergibt einen leeren Puffer (nichts behalten); ein umgekehrter
/// Bereich oder ein Anfang hinter dem Ende lässt alles stehen, damit ein
/// verrutschter Finger nicht versehentlich das ganze Sample leert.
pub fn zuschneiden(pcm: &mut Vec<f32>, von: usize, bis: usize) -> usize {
    let len = pcm.len();
    if von > bis || von > len {
        return len;
    }
    let bis = bis.min(len);
    let mut rest = pcm.split_off(von.min(len));
    rest.truncate(bis.saturating_sub(von));
    *pcm = rest;
    pcm.len()
}

/// Lineare Einblendung über `dauer` Proben ab `von` (Faktor 1.0 am Ende).
///
/// Die Rampe läuft über höchstens den ganzen Bereich; `dauer == 0` ist ein
/// No-op (sonst gäbe es eine Division durch null).
pub fn einblenden(pcm: &mut [f32], von: usize, bis: usize, dauer: usize) {
    let Some((von, bis)) = klemme(pcm.len(), von, bis) else {
        return;
    };
    if dauer == 0 {
        return;
    }
    let d = dauer.min(bis - von);
    for k in 0..d {
        pcm[von + k] *= (k + 1) as f32 / d as f32;
    }
}

/// Lineare Ausblendung über `dauer` Proben bis `bis` (Faktor 1.0 am Anfang,
/// `1/dauer` an der letzten Probe).
///
/// Spiegelbild der Einblendung, damit sich beide bei umgekehrtem Material
/// exakt entsprechen; `dauer == 0` ist ein No-op.
pub fn ausblenden(pcm: &mut [f32], von: usize, bis: usize, dauer: usize) {
    let Some((_, bis)) = klemme(pcm.len(), von, bis) else {
        return;
    };
    if dauer == 0 {
        return;
    }
    let d = dauer.min(bis - von);
    let start = bis - d;
    for k in 0..d {
        pcm[start + k] *= (d - k) as f32 / d as f32;
    }
}

/// Hebt den Bereich auf den Spitzenwert 1.0 und liefert den Faktor.
///
/// Stille (Spitze 0) und nicht-endliche Proben bleiben unverändert; der
/// Rückgabewert ist dann 1.0. So kann die Oberfläche den Faktor loggen, ohne
/// eine Division durch null befürchten zu müssen.
pub fn normalisieren(pcm: &mut [f32], von: usize, bis: usize) -> f32 {
    let Some((von, bis)) = klemme(pcm.len(), von, bis) else {
        return 1.0;
    };
    let mut spitze = 0.0f32;
    for s in &pcm[von..bis] {
        if s.is_finite() {
            spitze = spitze.max(s.abs());
        }
    }
    if spitze <= 0.0 {
        return 1.0;
    }
    let faktor = 1.0 / spitze;
    for s in &mut pcm[von..bis] {
        if s.is_finite() {
            *s *= faktor;
        }
    }
    faktor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nah(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn umkehren_exakt_und_teilbereich() {
        let mut pcm = [1.0f32, 2.0, 3.0, 4.0];
        umkehren(&mut pcm, 0, 4);
        assert_eq!(pcm, [4.0, 3.0, 2.0, 1.0]);

        let mut pcm = [1.0f32, 2.0, 3.0, 4.0, 5.0];
        umkehren(&mut pcm, 1, 4);
        assert_eq!(pcm, [1.0, 4.0, 3.0, 2.0, 5.0]);
    }

    #[test]
    fn umkehren_grenzen() {
        let mut leer: [f32; 0] = [];
        umkehren(&mut leer, 0, 10);

        let mut pcm = [1.0f32, 2.0, 3.0];
        umkehren(&mut pcm, 1, 1);
        assert_eq!(pcm, [1.0, 2.0, 3.0], "von == bis ändert nichts");

        umkehren(&mut pcm, 2, 1);
        assert_eq!(pcm, [1.0, 2.0, 3.0], "von > bis ändert nichts");

        umkehren(&mut pcm, 1, 99);
        assert_eq!(pcm, [1.0, 3.0, 2.0], "bis > len wird geklemmt");

        umkehren(&mut pcm, 99, 100);
        assert_eq!(pcm, [1.0, 3.0, 2.0], "von > len ändert nichts");
    }

    #[test]
    fn bereich_loeschen_und_laenge() {
        let mut pcm = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(bereich_loeschen(&mut pcm, 1, 3), 3);
        assert_eq!(pcm, [1.0, 4.0, 5.0]);

        assert_eq!(bereich_loeschen(&mut pcm, 0, 3), 0);
        assert!(pcm.is_empty(), "alles gelöscht");

        let mut leer: Vec<f32> = Vec::new();
        assert_eq!(bereich_loeschen(&mut leer, 0, 10), 0);

        let mut pcm = vec![1.0f32, 2.0, 3.0, 4.0];
        assert_eq!(bereich_loeschen(&mut pcm, 2, 2), 4);
        assert_eq!(pcm, [1.0, 2.0, 3.0, 4.0], "von == bis ändert nichts");
        assert_eq!(bereich_loeschen(&mut pcm, 3, 1), 4);
        assert_eq!(pcm, [1.0, 2.0, 3.0, 4.0], "von > bis ändert nichts");
        assert_eq!(
            bereich_loeschen(&mut pcm, 9, 10),
            4,
            "von > len ändert nichts"
        );
        assert_eq!(
            bereich_loeschen(&mut pcm, 2, 99),
            2,
            "bis > len kürzt das Ende"
        );
        assert_eq!(pcm, [1.0, 2.0]);
    }

    #[test]
    fn zuschneiden_behaelt_bereich() {
        let mut pcm = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(zuschneiden(&mut pcm, 1, 3), 2);
        assert_eq!(pcm, [2.0, 3.0]);

        let mut pcm = vec![1.0f32, 2.0, 3.0];
        assert_eq!(zuschneiden(&mut pcm, 0, 3), 3, "ganzer Bereich bleibt");
        assert_eq!(zuschneiden(&mut pcm, 1, 99), 2, "bis > len wird geklemmt");
        assert_eq!(pcm, [2.0, 3.0]);

        let mut pcm = vec![1.0f32, 2.0, 3.0];
        assert_eq!(zuschneiden(&mut pcm, 2, 2), 0, "von == bis ergibt leer");
        assert!(pcm.is_empty());

        let mut pcm = vec![1.0f32, 2.0, 3.0];
        assert_eq!(zuschneiden(&mut pcm, 2, 1), 3, "von > bis ändert nichts");
        assert_eq!(pcm, [1.0, 2.0, 3.0]);
        assert_eq!(zuschneiden(&mut pcm, 9, 10), 3, "von > len ändert nichts");
        assert_eq!(pcm, [1.0, 2.0, 3.0]);

        let mut leer: Vec<f32> = Vec::new();
        assert_eq!(zuschneiden(&mut leer, 0, 5), 0);
    }

    #[test]
    fn einblenden_linear() {
        let mut pcm = [1.0f32; 4];
        einblenden(&mut pcm, 0, 4, 4);
        assert_eq!(pcm, [0.25, 0.5, 0.75, 1.0], "Rampe bis 1.0 am Ende");

        let mut pcm = [1.0f32, 2.0, 3.0, 4.0];
        einblenden(&mut pcm, 1, 4, 2);
        assert_eq!(pcm, [1.0, 1.0, 3.0, 4.0], "nur die ersten 2 Proben");

        let mut pcm = [1.0f32, 2.0, 3.0];
        einblenden(&mut pcm, 1, 3, 0);
        assert_eq!(pcm, [1.0, 2.0, 3.0], "Dauer 0 ändert nichts");

        let mut pcm = [1.0f32; 3];
        einblenden(&mut pcm, 0, 3, 99);
        assert_eq!(
            pcm,
            [1.0 / 3.0, 2.0 / 3.0, 1.0],
            "Dauer > Bereich füllt ihn"
        );

        let mut pcm = [1.0f32, 2.0, 3.0];
        einblenden(&mut pcm, 2, 1, 1);
        assert_eq!(pcm, [1.0, 2.0, 3.0], "von > bis ändert nichts");
    }

    #[test]
    fn ausblenden_linear() {
        let mut pcm = [1.0f32; 4];
        ausblenden(&mut pcm, 0, 4, 4);
        assert_eq!(pcm, [1.0, 0.75, 0.5, 0.25], "Rampe von 1.0 bis 1/dauer");

        let mut pcm = [1.0f32, 2.0, 3.0];
        ausblenden(&mut pcm, 0, 3, 2);
        assert_eq!(pcm, [1.0, 2.0, 1.5], "nur die letzten 2 Proben");

        let mut pcm = [1.0f32, 2.0, 3.0];
        ausblenden(&mut pcm, 0, 3, 0);
        assert_eq!(pcm, [1.0, 2.0, 3.0], "Dauer 0 ändert nichts");

        let mut pcm = [1.0f32; 3];
        ausblenden(&mut pcm, 0, 3, 99);
        assert_eq!(
            pcm,
            [1.0, 2.0 / 3.0, 1.0 / 3.0],
            "Dauer > Bereich füllt ihn"
        );

        let mut pcm = [1.0f32, 2.0, 3.0];
        ausblenden(&mut pcm, 2, 1, 1);
        assert_eq!(pcm, [1.0, 2.0, 3.0], "von > bis ändert nichts");
    }

    #[test]
    fn normalisieren_spitze_und_stille() {
        let mut pcm = [0.25f32, -0.5, 0.1];
        let faktor = normalisieren(&mut pcm, 0, 3);
        assert!(nah(faktor, 2.0), "Faktor 2.0, war {faktor}");
        assert!(nah(pcm[0], 0.5));
        assert!(nah(pcm[1], -1.0), "negative Spitze wird -1.0");
        assert!(nah(pcm[2], 0.2));

        let mut still = [0.0f32; 4];
        let faktor = normalisieren(&mut still, 0, 4);
        assert_eq!(faktor, 1.0, "Stille bleibt unverändert");
        assert_eq!(still, [0.0; 4]);

        // Nur der gewählte Bereich zählt; außerhalb bleibt alles.
        let mut pcm = [0.1f32, 2.0, 0.5];
        let faktor = normalisieren(&mut pcm, 0, 1);
        assert!(nah(faktor, 10.0));
        assert!(nah(pcm[0], 1.0));
        assert_eq!(pcm[1], 2.0, "außerhalb unverändert");

        // Lautes Material wird heruntergerechnet.
        let mut pcm = [2.0f32, -0.5];
        let faktor = normalisieren(&mut pcm, 0, 2);
        assert!(nah(faktor, 0.5));
        assert_eq!(pcm, [1.0, -0.25]);

        // Grenzen: leerer/umgekehrter Bereich ändert nichts.
        let mut pcm = [0.5f32, 0.25];
        assert_eq!(normalisieren(&mut pcm, 1, 1), 1.0);
        assert_eq!(normalisieren(&mut pcm, 1, 0), 1.0);
        assert_eq!(pcm, [0.5, 0.25]);
        assert_eq!(normalisieren(&mut pcm, 9, 10), 1.0);
    }
}
