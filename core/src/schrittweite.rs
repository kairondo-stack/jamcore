//! Schrittweite: ein Wisch ueber die Masken-Felder als Stepbreiten-Rampe.
//!
//! Port aus `pattern_recorder_202601.cpp`: zwei aufeinanderfolgende
//! Tastendruecke tragen `startIndexnum` und `endIndexnum` (dort die `pitch`
//! der Noten mit `scl == 1`). Daraus wird die Rampe
//!
//! ```text
//! schrittwa = 1/startIndexnum      schrittwe = 1/endIndexnum
//! erster Schritt = schrittwa       (C++: `näschritt += schrittwa`)
//! Indexnumlaenge = laenge - schrittwa     (C++ `totalDi==1`)
//! kd        = Indexnumlaenge - basis * n  sumanz = n(n+1)/2  kdtanz = kd/sumanz
//! Breite(i) = schrittwe + (n-i+1)*kdtanz      (von breit nach schmal)
//!          | schrittwa +  i      *kdtanz      (von schmal nach breit)
//! ```
//!
//! (C++-Zeilen 2800–2846, 2862–2889 und 2943–2949). Die Summenprobe:
//! `sum(Breite) == laenge`, die Rampe fuellt den Abschnitt also genau. Der
//! **erste** Schritt ist dabei 1/startIndexnum selbst — ohne ihn waren alle
//! Breiten fast gleich und von 1/4 war nichts mehr zu sehen (gemessen
//! 2026-10-12).
//!
//! **Abweichungen vom Original, mit Grund:**
//! - `laenge` ist die Länge des Abschnitts in Sekunden; der C++ schaltet
//!   zwischen `totalDi==1` (Dauer in Sekunden) und `totalDi==0` (Zyklus 0..1,
//!   am Ende mit der Dauer skaliert) um. Beide Modi sind im Kern dieselbe
//!   Form, die App nimmt immer den Sekunden-Modus.
//! - `n = |endIndexnum - startIndexnum|`. Der C++ rechnet `(e-s)^2/(e-s)`,
//!   also `e - s` mit Vorzeichen — bei „43 auf 23" ist das negativ und die
//!   Schleife `for (i=1; i<=schrittweitanz; ++i)` laeuft nie, es entsteht kein
//!   einziger Schritt. Ohne den Betrag waere ein breiter werdender Wisch tot.
//! - Es gibt nur eine Rampe, nicht den Mittelwert der linken und rechten
//!   (C++ 2893–2898). Der Mittelwert verschiebt die Rampe um einen halben
//!   Schritt und ist fuer die App nicht noetig.
//! - Bei sehr kurzen Abschnitten koennen die Rohbreiten negativ werden (der
//!   erste Schritt 1/von ist fast so lang wie der Abschnitt); dann wird
//!   geklemmt und auf die Laenge skaliert.
//! - Zusaetzlich [`Abschnitt`]-Regeln: in der App laesst sich ein Abschnitt in
//!   der Zeitleiste antippen und mit min/max-Schritten ueberschreiben, ohne
//!   die Feldwerte neu zu tippen.

/// Ein Fingerdruck auf dem Masken-Feld: Zeit seit Aufnahmebeginn und der
/// Feldwert als Indexnum (Unterteilungen je Schritt).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tipp {
    pub zeit: f32,
    pub indexnum: f32,
}

impl Tipp {
    /// Druck zur Zeit `zeit` auf dem Feld mit dem Wert `indexnum`.
    pub fn new(zeit: f32, indexnum: f32) -> Self {
        Self { zeit, indexnum }
    }
}

/// Schrittbreite eines Indexnum: `1/indexnum`. Ohne Indexnum (0 oder
/// negativer Wert) gibt es keine Teilung, also 0.
pub fn schrittweite(indexnum: f32) -> f32 {
    if indexnum <= 0.0 {
        0.0
    } else {
        1.0 / indexnum
    }
}

/// Anzahl Schritte zwischen zwei Feldern: die Differenz der Indexnum wie im
/// C++ (`rschrittweitanz`), aber als Betrag (siehe Modulkopf). Ohne
/// Unterschied 0.
pub fn schrittanzahl(von: f32, bis: f32) -> usize {
    (bis - von).abs().round().max(0.0) as usize
}

/// Breiten der Schritte, mit denen der Abschnitt von `von` bis `bis` (Indexnum,
/// Laenge `laenge`) gefuellt wird. Leeres Ergebnis, wenn beide Felder
/// denselben Wert tragen — dann gibt es nichts zu verteilen.
pub fn abschnitt_breiten(von: f32, bis: f32, laenge: f32) -> Vec<f32> {
    let n = schrittanzahl(von, bis);
    // Feld 0 ist die Pause und hat keine Teilung: mit ihr als Rand gibt es
    // keinen Schritt, den man verteilen koennte.
    if n == 0 || laenge <= 0.0 || von <= 0.0 || bis <= 0.0 {
        return Vec::new();
    }
    let schrittwa = schrittweite(von);
    let schrittwe = schrittweite(bis);
    // Der **erste** Schritt ist der Startwert selbst: der C++ setzt
    // `näschritt += schrittwa` vor die Rampe, und die Rampe fuellt nur den
    // Rest (`Indexnumlaenge = laenge - 1/startIndexnum`, Modus `totalDi==1`).
    // Ohne diesen Schritt waren alle Breiten fast gleich und von 1/4 war
    // nichts mehr zu sehen (gemessen 2026-10-12).
    let indexnumlaenge = laenge - schrittwa;
    // Startet der Abschnitt breit (1/von > 1/bis), laeuft die Rampe von der
    // Endbreite herunter, sonst von der Anfangsbreite hinauf — genau die
    // beiden Zweige des C++.
    let fallend = schrittwa > schrittwe;
    let basis = if fallend { schrittwe } else { schrittwa };
    let kd = indexnumlaenge - basis * n as f32;
    let sumanz = (n * (n + 1)) / 2;
    let kdtanz = if sumanz == 0 { 0.0 } else { kd / sumanz as f32 };
    let mut roh: Vec<f32> = Vec::with_capacity(n + 1);
    roh.push(schrittwa);
    for i in 1..=n {
        roh.push(if fallend {
            basis + (n - i + 1) as f32 * kdtanz
        } else {
            basis + i as f32 * kdtanz
        });
    }
    // Kurze Abschnitte koennen die Rampe unter null druecken (der erste
    // Schritt 1/von ist schon fast so lang wie der ganze Abschnitt). Dann
    // klemmen und auf die Laenge skalieren: alle Schritte bleiben positiv
    // und die Summe ist weiter genau die Abschnittslaenge.
    if roh.iter().any(|b| *b <= 1e-6) {
        let klein = 1e-6;
        let geklemmt: Vec<f32> = roh.iter().map(|b| b.max(klein)).collect();
        let summe: f32 = geklemmt.iter().sum();
        return geklemmt.iter().map(|b| b * laenge / summe).collect();
    }
    roh
}

/// Zeitpunkte innerhalb eines Abschnitts `[start, start+laenge]`: `breiten`
/// aufsummiert, **ohne** den Endpunkt (der ist der naechste Tipp). Also
/// `n` Punkte fuer `n` Breiten; die letzte Breite endet genau bei
/// `start + laenge`.
pub fn abschnitt_zeitpunkte(start: f32, breiten: &[f32]) -> Vec<f32> {
    let mut raus = Vec::with_capacity(breiten.len());
    let mut t = start;
    for breite in breiten {
        t += breite;
        raus.push(t);
    }
    raus
}

/// Alle Zeitpunkte der Folge: die Tipps selbst plus die Zwischenschritte der
/// Schrittweiten-Abschnitte. Ohne Wertunterschied zwischen zwei Feldern gibt
/// es keine Zwischenschritte, dann stehen nur die Tipps im Ergebnis.
///
/// Der letzte Punkt eines Abschnitts faellt mit dem naechsten Tipp zusammen
/// und faellt deshalb weg — sonst stuende dieselbe Zeit zweimal im Ergebnis.
pub fn timestamps(tipps: &[Tipp]) -> Vec<f32> {
    timestamps_mit_regeln(tipps, &[])
}

/// Regel fuer den Abschnitt zwischen zwei Tipps: die Indexnums, die die Rampe
/// begrenzen. In der App setzt man sie in der Zeitleiste (min/max Schritte),
/// ohne die Feldwerte neu zu tippen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Abschnitt {
    pub von: f32,
    pub bis: f32,
}

/// Wie [`timestamps`], aber `regeln[i]` ersetzt fuer den Abschnitt zwischen
/// Tipp `i` und `i+1` die Indexnums. Ohne Regel (oder ohne Eintrag) gelten die
/// Feldwerte der Tipps.
pub fn timestamps_mit_regeln(tipps: &[Tipp], regeln: &[Option<Abschnitt>]) -> Vec<f32> {
    let mut raus: Vec<f32> = Vec::new();
    for (i, tipp) in tipps.iter().enumerate() {
        raus.push(tipp.zeit);
        let Some(naechster) = tipps.get(i + 1) else {
            break;
        };
        let laenge = naechster.zeit - tipp.zeit;
        let (von, bis) = match regeln.get(i).copied().flatten() {
            Some(regel) => (regel.von, regel.bis),
            None => (tipp.indexnum, naechster.indexnum),
        };
        let breiten = abschnitt_breiten(von, bis, laenge);
        let mut punkte = abschnitt_zeitpunkte(tipp.zeit, &breiten);
        punkte.pop();
        raus.extend(punkte);
    }
    raus
}

/// Kuerzungsfaktor je Schritt aus dem Geschwindigkeitsprofil eines Wischs.
///
/// `tempo` sind die Wege je Frame (Wischweg/Wischzeit), `tempo_ref` der
/// Mittelwert. Der Faktor vergleicht die momentane Geschwindigkeit mit der
/// durchschnittlichen: **gleichmaessiges Wischen ergibt ueberall 1.0** und
/// damit keine Aenderung der Timestamps — das ist die Vorgabe, an der sich die
/// Modulation messen laesst.
pub fn kruemmung(tempo: &[f32], tempo_ref: f32) -> Vec<f32> {
    if tempo_ref.abs() < 1e-6 {
        return vec![1.0; tempo.len()];
    }
    tempo.iter().map(|v| v / tempo_ref).collect()
}

/// Mittelwert des Geschwindigkeitsprofils. Ohne verwertbare Werte 0.0, was in
/// [`kruemmung`] zu einem Faktor von 1.0 fuehrt (keine Modulation).
pub fn tempo_mittel(tempo: &[f32]) -> f32 {
    if tempo.is_empty() {
        return 0.0;
    }
    tempo.iter().sum::<f32>() / tempo.len() as f32
}

/// Untere und obere Grenze des Kuerzungsfaktors. Das Geschwindigkeitsdelta
/// wird auf dieses Band gemappt: ein Wisch, der 20-mal so schnell ist wie der
/// Durchschnitt, soll die Rampe stark kippen, aber nicht die Rechnung
/// aufblasen.
pub const KAPPA_MIN: f32 = 0.25;
/// Siehe [`KAPPA_MIN`].
pub const KAPPA_MAX: f32 = 4.0;

/// Die Timestamps eines Abschnitts mit der Kruemmung des Wischs verschieben.
///
/// `kappa` sind die Kuerzungsfaktoren aus [`kruemmung`] (einer je Zeitpunkt),
/// `staerke` waechelt, wie stark die Modulation wirkt. Schneller wischen
/// (kappa > 1) rueckt die Zwischenschritte **frueher**, langsamer wischen
/// spaeter; das ist die Beschleunigung als Zeit-Verschiebung.
///
/// Damit sich dabei **keine Punkte ueberholen** (ein Nullschritt oder ein
/// Rueckwaertslaufen waere musicalisch muell und im Sweep falsch), wird die
/// Verschiebung auf den Raum begrenzt, den die Nachbarn frei lassen: hoechstens
/// die Haelfte des Abstands zum naechstgelegenen Nachbarn. Ausserdem wird
/// kappa auf [`KAPPA_MIN`]…[`KAPPA_MAX`] gemappt.
///
/// Die Randpunkte (Anfang und Ende des Abschnitts) bleiben unangetastet, sonst
/// wuerde die Folge aus dem Takt gerutscht. Fuer gleichmaessiges Wischen
/// (kappa ueberall 1.0) ist das Ergebnis bitgleich der Eingabe.
pub fn modulieren(zeiten: &[f32], kappa: &[f32], staerke: f32) -> Vec<f32> {
    if zeiten.len() < 3 || kappa.len() < zeiten.len() || staerke.abs() < 1e-9 {
        return zeiten.to_vec();
    }
    let start = zeiten[0];
    let spanne = zeiten[zeiten.len() - 1] - start;
    if spanne.abs() < 1e-9 {
        return zeiten.to_vec();
    }
    zeiten
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            // Der Abschnittsanfang und sein Ende bleiben, wo sie sind.
            if i == 0 || i + 1 == zeiten.len() {
                return t;
            }
            // Sinus-Glocke: 0 an beiden Raendern, 1 in der Mitte.
            let u = (t - start) / spanne;
            let glocke = (std::f32::consts::PI * u).sin();
            let k = kappa
                .get(i)
                .copied()
                .unwrap_or(1.0)
                .clamp(KAPPA_MIN, KAPPA_MAX);
            let roh = (k - 1.0) * staerke * glocke * spanne;
            // Raum bis zu den Nachbarn, geviertelt: jeder Punkt darf auf
            // beiden Seiten nur ein Viertel des Abstands nutzen. Ziehen zwei
            // Nachbarn gegeneinander, schliessen sich ihre Wege dann auf
            // hoechstens zur Haelfte des Abstands — sie kommen sich nie nah.
            let links = t - zeiten[i - 1];
            let rechts = zeiten[i + 1] - t;
            let raum = links.min(rechts) / 4.0;
            (t - roh.clamp(-raum, raum)).max(start)
        })
        .collect()
}

/// Pause in Sekunden, ab der ein Felddruck die Aufnahme neu beginnt. Also:
/// schneller als dieser Abstand gehoert die Folge zusammen, langsamer beginnt
/// eine neue. Ohne diese Grenze lief die Aufnahme die ganze Sitzung durch —
/// der erste Tipp kam dann bei den Sekunden seit dem App-Start an (gemessen
/// 548 s) statt bei 0.
pub const NEU_AFTER_SEKUNDEN: f32 = 8.0;

/// Ob ein Fingerdruck die Aufnahme neu beginnt.
///
/// Zwei Bedingungen muessen zutreffen: der Druck muss ein **Feld** getroffen
/// haben, und seit dem Anker muss mehr als `grenze` Sekunden vergangen sein
/// (kein Anker heisst: der allererste Druck).
///
/// Der erste Teil ist nicht Kosmetik: das Feld hat rechts neben der letzten
/// Zeile leere Zellen, und ein Druck dort loescht die Folge und setzt den
/// Anker, ohne selbst einen Ton zu liefern. Der naechste echte Tipp kam dann
/// bei 1,70 s statt bei 0 an (gemessen 2026-10-10).
pub fn beginnt_aufnahme(feld_getroffen: bool, anker_sekunden: Option<f32>, grenze: f32) -> bool {
    feld_getroffen && anker_sekunden.map(|s| s > grenze).unwrap_or(true)
}

/// Die Zeitpunkte auf die Spalten der X-Aufloesung legen — der Massstab von
/// `KeyEventRender.txt`: die Zeitpunkte einer Folge decken die Breite ab, und
/// jeder landet auf der Spalte, die seiner Position entspricht.
///
/// Ohne `schritte` (oder bei nur einem Zeitpunkt) kommt nichts heraus. Die
/// Spalten sind sortiert und liegen immer im Bereich, damit der Sweep vorwaerts
/// laeuft.
pub fn spalten(zeiten: &[f32], schritte: usize) -> Vec<usize> {
    let (Some(&erste), Some(&letzte)) = (zeiten.first(), zeiten.last()) else {
        return Vec::new();
    };
    if schritte == 0 || zeiten.len() < 2 {
        return Vec::new();
    }
    let spanne = letzte - erste;
    let mut raus: Vec<usize> = zeiten
        .iter()
        .map(|t| {
            let anteil = if spanne.abs() < 1e-9 {
                0.0
            } else {
                (t - erste) / spanne
            };
            let spalte = (anteil * schritte as f32).round();
            (spalte.max(0.0) as usize).min(schritte - 1)
        })
        .collect();
    raus.sort_unstable();
    raus.dedup();
    raus
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kurz: zwei Werte mit Nachschaerfe vergleichen.
    fn nah(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn schrittweite_ist_der_kehrwert() {
        assert_eq!(schrittweite(2.0), 0.5);
        assert_eq!(schrittweite(4.0), 0.25);
        assert!(nah(schrittweite(43.0), 1.0 / 43.0));
    }

    #[test]
    fn ohne_indexnum_keine_teilung() {
        assert_eq!(schrittweite(0.0), 0.0, "Feld 0 = Pause, kein Schritt");
        assert_eq!(schrittweite(-3.0), 0.0);
    }

    #[test]
    fn schrittanzahl_ist_die_differenz_der_felder() {
        assert_eq!(schrittanzahl(2.0, 4.0), 2, "2 auf 4");
        assert_eq!(schrittanzahl(3.0, 15.0), 12, "3 auf 15");
        assert_eq!(schrittanzahl(43.0, 23.0), 20, "43 auf 23 (Betrag!)");
        assert_eq!(schrittanzahl(5.0, 5.0), 0, "gleiches Feld");
    }

    #[test]
    fn breiten_fuellen_den_abschnitt_genau() {
        for (von, bis, laenge) in [
            (2.0_f32, 4.0_f32, 1.0_f32),
            (4.0, 2.0, 2.0),
            (3.0, 15.0, 0.5),
        ] {
            let breiten = abschnitt_breiten(von, bis, laenge);
            let summe: f32 = breiten.iter().sum();
            assert!(nah(summe, laenge), "{von}->{bis}: {summe} != {laenge}");
        }
    }

    #[test]
    fn erster_schritt_ist_der_startwert() {
        // Der C++ setzt `näschritt += schrittwa` **vor** die Rampe: der erste
        // Schritt eines Abschnitts ist 1/startIndexnum lang, die Rampe fuellt
        // nur den Rest (Indexnumlaenge = laenge - 1/startIndexnum).
        // Gemessen 2026-10-12 am Port: ohne diesen Schritt waren alle Breiten
        // fast gleich und 1/4 war nicht mehr zu sehen.
        let breiten = abschnitt_breiten(4.0, 26.0, 1.03);
        assert_eq!(breiten.len(), 23, "erster Schritt + 22 Rampenschritte");
        assert!(nah(breiten[0], 0.25), "1/4: {breiten:?}");
        assert!(
            breiten[0] > 6.0 * breiten[1],
            "der erste Schritt ist deutlich breiter als die Rampe: {breiten:?}"
        );
        let summe: f32 = breiten.iter().sum();
        assert!(nah(summe, 1.03), "Summe bleibt die Abschnittslaenge");
    }

    #[test]
    fn kurze_abschnitte_bleiben_positiv() {
        // 0,3 s fuer 22 Rampenschritte: 1/4 passt kaum hinein, die Rohbreiten
        // wuerden negativ. Dann wird geklemmt und auf die Laenge skaliert —
        // kein Rueckwaerts-Timestamp.
        let breiten = abschnitt_breiten(4.0, 26.0, 0.3);
        assert!(breiten.iter().all(|b| *b > 0.0), "{breiten:?}");
        let summe: f32 = breiten.iter().sum();
        assert!(nah(summe, 0.3), "{summe} != 0.3");
    }

    #[test]
    fn regeln_ersetzen_die_indexnums_eines_abschnitts() {
        // Nutzerwunsch 2026-10-12: in der Zeitleiste einen Abschnitt antippen
        // und Schritt-min/max setzen, ohne die Feldwerte neu zu tippen.
        let tipps = [Tipp::new(0.0, 2.0), Tipp::new(1.0, 4.0)];
        let ohne = timestamps(&tipps);
        assert_eq!(ohne.len(), 4);
        let mit = timestamps_mit_regeln(&tipps, &[Some(Abschnitt { von: 2.0, bis: 8.0 })]);
        assert_eq!(mit.len(), 8, "sechs Rampenschritte: {mit:?}");
        assert!(nah(mit[1], 0.5), "erster Schritt bleibt 1/2: {mit:?}");
        assert!(mit.windows(2).all(|w| w[0] < w[1]), "{mit:?}");
        assert_eq!(*mit.last().unwrap(), 1.0, "endet beim zweiten Tipp");
    }

    #[test]
    fn ohne_regel_bleibt_alles_wie_gehabt() {
        let tipps = [Tipp::new(0.0, 2.0), Tipp::new(1.0, 4.0)];
        assert_eq!(timestamps_mit_regeln(&tipps, &[]), timestamps(&tipps));
        assert_eq!(timestamps_mit_regeln(&tipps, &[None]), timestamps(&tipps));
    }

    #[test]
    fn rampe_von_2_nach_4_faellt() {
        // Feld 2 -> schrittweite 0.5 (breit), Feld 4 -> 0.25 (schmal): der
        // Abschnitt wird schmaler, das ist der C++-Zweig `schrittwa >
        // schrittwe` mit der Basis `schrittwe`. Der erste Schritt ist 1/2,
        // die beiden Rampenschritte sind bei kd = 0 genau 1/4.
        let breiten = abschnitt_breiten(2.0, 4.0, 1.0);
        assert_eq!(breiten.len(), 3);
        assert!(nah(breiten[0], 0.5), "{breiten:?}");
        assert!(breiten[1] <= breiten[0], "Rampe schmaler: {breiten:?}");
        assert!(
            nah(breiten[1], 0.25) && nah(breiten[2], 0.25),
            "{breiten:?}"
        );
    }

    #[test]
    fn rampe_von_43_nach_23_waechst() {
        // Feld 43 -> 1/43 = 0.023 (schmal), Feld 23 -> 1/23 = 0.043 (breit):
        // der Abschnitt wird breiter, der andere C++-Zweig.
        let breiten = abschnitt_breiten(43.0, 23.0, 1.0);
        assert_eq!(breiten.len(), 21, "erster Schritt + 20 Rampenschritte");
        assert!(nah(breiten[0], 1.0 / 43.0), "{breiten:?}");
        assert!(
            breiten[20] > breiten[0],
            "letzter Schritt breiter: {breiten:?}"
        );
    }

    #[test]
    fn gleiche_felder_erzeugen_keine_zwischenschritte() {
        assert!(abschnitt_breiten(5.0, 5.0, 1.0).is_empty());
        assert!(
            abschnitt_breiten(5.0, 5.0, 0.0).is_empty(),
            "keine Zeit, nichts zu fuellen"
        );
    }

    #[test]
    fn feld_null_ist_die_pause_und_teilt_nicht() {
        assert!(
            abschnitt_breiten(0.0, 8.0, 1.0).is_empty(),
            "von der Pause aus"
        );
        assert!(
            abschnitt_breiten(8.0, 0.0, 1.0).is_empty(),
            "in die Pause hinein"
        );
        let zeiten = timestamps(&[Tipp::new(0.0, 0.0), Tipp::new(1.0, 8.0)]);
        assert_eq!(zeiten, vec![0.0, 1.0], "nur die Tipps: {zeiten:?}");
    }

    #[test]
    fn zeitpunkte_summieren_sich_bis_zum_ende() {
        let breiten = abschnitt_breiten(2.0, 4.0, 1.0);
        let punkte = abschnitt_zeitpunkte(2.0, &breiten);
        assert_eq!(punkte.len(), breiten.len());
        let spanne = punkte.last().copied().unwrap_or(0.0) - 2.0;
        assert!(
            nah(spanne, 1.0),
            "letzter Punkt liegt am Abschnittsende: {spanne}"
        );
    }

    #[test]
    fn timestamps_enthalten_die_tipps_und_die_abschnitte() {
        let tipps = [Tipp::new(0.0, 2.0), Tipp::new(1.0, 4.0)];
        let zeiten = timestamps(&tipps);
        // Zwei Tipps plus zwei Zwischenschritte: erster Schritt 1/2, dann zwei
        // Rampenschritte à 1/4 — deren letzter Endpunkt ist der naechste Tipp.
        assert_eq!(zeiten.len(), 4, "{zeiten:?}");
        assert!(
            zeiten.windows(2).all(|w| w[0] <= w[1]),
            "sortiert: {zeiten:?}"
        );
        assert_eq!(zeiten[0], 0.0);
        assert_eq!(*zeiten.last().unwrap(), 1.0, "endet beim letzten Tipp");
        assert!(
            nah(zeiten[1], 0.5) && nah(zeiten[2], 0.75),
            "Zwischenschritte: {zeiten:?}"
        );
    }

    #[test]
    fn timestamps_bei_gleichen_feldern_sind_nur_die_tipps() {
        let tipps = [
            Tipp::new(0.0, 3.0),
            Tipp::new(0.5, 3.0),
            Tipp::new(1.0, 3.0),
        ];
        assert_eq!(timestamps(&tipps), vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn timestamps_sind_streng_steigend() {
        let tipps = [
            Tipp::new(0.0, 15.0),
            Tipp::new(0.7, 3.0),
            Tipp::new(1.6, 43.0),
        ];
        let zeiten = timestamps(&tipps);
        assert!(
            zeiten.windows(2).all(|w| w[1] > w[0]),
            "keine doppelten Zeiten: {zeiten:?}"
        );
    }

    #[test]
    fn leere_und_ein_tipp() {
        assert!(timestamps(&[]).is_empty());
        assert_eq!(timestamps(&[Tipp::new(0.3, 7.0)]), vec![0.3]);
    }

    #[test]
    fn gleichmaessiges_wischen_aendert_nichts() {
        // Die Vorgabe: bei gleichmaessigem Wischen bleiben die Timestamps
        // unveraendert. Das Profil ist konstant, also kappa ueberall 1.0.
        let tempo = [0.5, 0.5, 0.5, 0.5, 0.5];
        let kappa = kruemmung(&tempo, tempo_mittel(&tempo));
        assert!(kappa.iter().all(|k| (*k - 1.0).abs() < 1e-6), "{kappa:?}");

        let zeiten = [0.0, 0.25, 0.5, 0.75, 1.0];
        let raus = modulieren(&zeiten, &kappa, 0.6);
        for (a, b) in zeiten.iter().zip(&raus) {
            assert_eq!(*a, *b, "bitgleich bei gleichmaessigem Wischen: {raus:?}");
        }
    }

    #[test]
    fn beschleunigen_rueckt_die_schritte_frueher() {
        let zeiten = [0.0, 0.25, 0.5, 0.75, 1.0];
        let kappa = [1.0, 2.0, 2.0, 2.0, 1.0];
        let raus = modulieren(&zeiten, &kappa, 0.6);
        assert!(
            raus[1] < zeiten[1],
            "schneller wischen -> frueher: {raus:?}"
        );
        assert_eq!(raus[0], zeiten[0], "Anfang bleibt");
        assert_eq!(*raus.last().unwrap(), zeiten[4], "Ende bleibt");
    }

    #[test]
    fn langsamer_werden_schiebt_die_schritte_spaeter() {
        let zeiten = [0.0, 0.25, 0.5, 0.75, 1.0];
        let kappa = [1.0, 0.5, 0.5, 0.5, 1.0];
        let raus = modulieren(&zeiten, &kappa, 0.6);
        assert!(
            raus[1] > zeiten[1],
            "langsamer wischen -> spaeter: {raus:?}"
        );
    }

    #[test]
    fn kruemmung_ohne_geschwindigkeit_bleibt_neutral() {
        assert_eq!(
            kruemmung(&[0.0; 4], 0.0),
            vec![1.0; 4],
            "Ruhe = keine Modulation"
        );
        assert_eq!(tempo_mittel(&[]), 0.0);
        let zeiten = [0.0, 0.5, 1.0];
        assert_eq!(modulieren(&zeiten, &[1.0, 1.0, 1.0], 0.6), zeiten);
    }

    #[test]
    fn modulation_veraendert_die_reihenfolge_nicht() {
        let zeiten = [0.0, 0.2, 0.45, 0.7, 0.9, 1.0];
        let kappa = [1.0, 2.5, 0.4, 1.8, 0.6, 1.0];
        let raus = modulieren(&zeiten, &kappa, 0.5);
        assert!(
            raus.windows(2).all(|w| w[0] < w[1]),
            "sortiert bleiben: {raus:?}"
        );
    }

    #[test]
    fn keiner_ueberholt_sich_auch_bei_wildem_wisch() {
        // Ganz egal wie kraeftig und ungleichmaessig gewischt wird: die
        // Zeitpunkte bleiben streng in der Reihenfolge.
        for staerke in [0.2_f32, 0.6, 1.0, 4.0] {
            let zeiten: Vec<f32> = (0..=12).map(|i| i as f32 * 0.077).collect();
            let kappa: Vec<f32> = (0..=12)
                .map(|i| if i % 2 == 0 { 9.0 } else { 0.02 })
                .collect();
            let raus = modulieren(&zeiten, &kappa, staerke);
            assert!(
                raus.windows(2).all(|w| w[1] > w[0]),
                "staerke {staerke}: {raus:?}"
            );
        }
    }

    #[test]
    fn verschiebung_bleibt_im_viertel_des_abstands() {
        let zeiten = [0.0, 0.25, 0.5, 0.75, 1.0];
        let kappa = [1.0, 4.0, 4.0, 4.0, 1.0];
        let raus = modulieren(&zeiten, &kappa, 1.0);
        for i in 1..zeiten.len() - 1 {
            let verschiebung = (zeiten[i] - raus[i]).abs();
            let raum = (zeiten[i] - zeiten[i - 1]).min(zeiten[i + 1] - zeiten[i]) / 4.0;
            assert!(
                verschiebung <= raum + 1e-6,
                "Punkt {i}: {verschiebung} > {raum}"
            );
        }
    }

    #[test]
    fn kappa_wird_auf_sein_band_gemappt() {
        let zeiten = [0.0, 0.25, 0.5, 0.75, 1.0];
        let kappa = [1.0, 100.0, 100.0, 100.0, 1.0];
        let raus = modulieren(&zeiten, &kappa, 1.0);
        // Mit kappa = 100 waere die Rohverschiebung riesig; nach dem Band und
        // der Raumgrenze bleibt sie klein.
        assert!(raus[1] > zeiten[1] - 0.1, "{raus:?}");
        assert!(raus.windows(2).all(|w| w[0] < w[1]), "sortiert: {raus:?}");
        let _ = KAPPA_MIN;
    }

    #[test]
    fn zu_kurze_abschnitte_bleiben_ungeaendert() {
        let zeiten = [0.0, 1.0];
        let kappa = [2.0, 2.0];
        assert_eq!(
            modulieren(&zeiten, &kappa, 0.9),
            zeiten,
            "nur zwei Punkte: nichts zu ruecken"
        );
    }

    #[test]
    fn spalten_bleiben_im_raster() {
        // Vier gleichmaessige Tipps ueber 16 Schritte: 0, 5, 11, 15.
        let zeiten = [0.0, 0.33, 0.67, 1.0];
        let spalten = spalten(&zeiten, 16);
        assert_eq!(spalten, vec![0, 5, 11, 15], "{spalten:?}");
        assert!(spalten.iter().all(|s| *s < 16), "keine Spalte ausserhalb");
    }

    #[test]
    fn spalten_ohne_raster_oder_ohne_spanne() {
        assert!(spalten(&[], 16).is_empty());
        assert!(
            spalten(&[0.5], 16).is_empty(),
            "ein Zeitpunkt, keine Spanne"
        );
        assert!(spalten(&[0.0, 1.0], 0).is_empty(), "kein Raster");
        assert_eq!(
            spalten(&[0.4, 0.4], 16),
            vec![0],
            "gleiche Zeit -> eine Spalte"
        );
    }

    #[test]
    fn spalten_sind_sortiert_und_ohne_doppelte() {
        // Zwei Zeitpunkte auf derselben Position fallen auf eine Spalte.
        let zeiten = [0.0, 0.1, 0.1, 0.5, 1.0];
        let spalten = spalten(&zeiten, 8);
        assert_eq!(spalten, vec![0, 1, 4, 7], "{spalten:?}");
        assert!(
            spalten.windows(2).all(|w| w[0] < w[1]),
            "streng steigend: {spalten:?}"
        );
    }

    #[test]
    fn die_ganze_breite_wird_je_einmal_belegt() {
        let zeiten: Vec<f32> = (0..=64).map(|i| i as f32 / 64.0).collect();
        let spalten = spalten(&zeiten, 16);
        assert_eq!(spalten.first(), Some(&0), "Anfang auf Spalte 0");
        assert_eq!(spalten.last(), Some(&15), "Ende auf der letzten Spalte");
    }

    #[test]
    fn druck_ohne_feld_startet_keine_aufnahme() {
        // Gemessen 2026-10-10: ein Tippen auf die leere Zelle rechts neben dem
        // letzten Feld loeschte die Folge und setzte den Anker — der naechste
        // echte Tipp kam bei 1,70 s statt bei 0 an.
        assert!(
            !beginnt_aufnahme(false, None, NEU_AFTER_SEKUNDEN),
            "ohne Anker und ohne Feld: nichts"
        );
        assert!(
            !beginnt_aufnahme(false, Some(99.0), NEU_AFTER_SEKUNDEN),
            "auch ein uralter Anker wird vom leeren Feld nicht geaendert"
        );
    }

    #[test]
    fn nach_langer_pause_beginnt_der_naechste_felddruck_neu() {
        assert!(beginnt_aufnahme(true, None, NEU_AFTER_SEKUNDEN));
        assert!(beginnt_aufnahme(
            true,
            Some(NEU_AFTER_SEKUNDEN + 0.1),
            NEU_AFTER_SEKUNDEN
        ));
    }

    #[test]
    fn innerhalb_der_pause_laeuft_die_aufnahme_weiter() {
        assert!(
            !beginnt_aufnahme(true, Some(1.7), NEU_AFTER_SEKUNDEN),
            "zweiter Tipp 1,7 s spaeter haengt sich an"
        );
        assert!(
            !beginnt_aufnahme(true, Some(NEU_AFTER_SEKUNDEN), NEU_AFTER_SEKUNDEN),
            "genau auf der Grenze noch nicht"
        );
    }

    #[test]
    fn die_ausgewaehlten_schrittweiten_liegen_auf_ihren_spalten() {
        // Der ganze Weg: Tipps auf 2, 4 und 43 -> Zwischenschritte aus der
        // Rampe -> Spalten. Der erste Tipp liegt auf Spalte 0, der letzte auf
        // der letzten, alles dazwischen sortiert.
        let tipps = [
            Tipp::new(0.0, 2.0),
            Tipp::new(0.6, 43.0),
            Tipp::new(1.2, 5.0),
        ];
        let zeiten = timestamps(&tipps);
        let spalten = spalten(&zeiten, 16);
        assert_eq!(spalten.first(), Some(&0));
        assert_eq!(spalten.last(), Some(&15));
        assert!(spalten.windows(2).all(|w| w[0] < w[1]), "{spalten:?}");
        assert!(
            zeiten.len() > tipps.len(),
            "die Rampe hat Zwischenschritte erzeugt"
        );
    }
}
