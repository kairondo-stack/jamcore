//! Harmonics-/Chord-Filter des Live-Scans (Portierung von `is_note_in_chord`
//! und `in_notesset_range` aus dem Desktop-rrn5).
//!
//! Der Filter entscheidet, welche Noten der Scan überhaupt spielen darf:
//!
//! - `an = false` lässt alles durch (Desktop: `harmonics`-Taste `4`).
//! - `notenmenge = true`: je Harmonie eine Notenklassen-Menge mit einem
//!   X-Band (Desktop: `useNotesSet`, Tasten `3`/`5`).
//! - sonst Dur (`0,4,(7)`), Plagal (`0,5,7`) oder Moll (`0,3,(7)`) —
//!   die Map ist der `is_note_in_chord`-Zweig, `quinte` schaltet die 7 dazu.
//!
//! Der Desktop-Chord-Teil (`chordTypes`/`is_note_in_chordpatt`) ist dort toter
//! Code (die Liste wird nie gefüllt); er wird bewusst nicht portiert.

/// Einstellungen und Notenmengen des Harmonics-Filters.
#[derive(Clone, Debug, PartialEq)]
pub struct Harmonie {
    /// Filter an/aus (`harmonics`).
    pub an: bool,
    /// Dur-Akkord (`useMajorChords`).
    pub dur: bool,
    /// Plagal (`usePlagalCadence`).
    pub plagal: bool,
    /// Perfekte Quinte zuschalten (`usePerfectFifth`).
    pub quinte: bool,
    /// Notenmengen statt Akkord (`useNotesSet`).
    pub notenmenge: bool,
    /// Transposition der Prüfnote (`noteValueadd`).
    pub note_value_add: i32,
    /// Je Harmonie die erlaubten Notenklassen.
    pub sets: Vec<Vec<i32>>,
    /// Je Harmonie das X-Band als Startspalte.
    pub setx: Vec<Vec<i32>>,
    /// Wie viele Harmonie-Mengen gelten (`notessetcount`).
    pub notessetcount: i32,
}

impl Default for Harmonie {
    /// Wie die Desktop-Vorgaben: Filter an, Dur, Quinte und Plagal bereit,
    /// Notenmengen aus.
    fn default() -> Self {
        Self {
            an: true,
            dur: true,
            plagal: true,
            quinte: true,
            notenmenge: false,
            note_value_add: 0,
            sets: Vec::new(),
            setx: Vec::new(),
            notessetcount: 0,
        }
    }
}

/// Notenklasse in den harmonischen Mengen prüfen; die X-Bänder gelten je Menge
/// (Menge 0 für `x < setx[0][0]`, Menge v für `setx[v-1][0] <= x < setx[v][0]`).
/// Portierung von `in_notesset_range` aus dem Desktop-rrn5.
///
/// Wichtig: `setx[0]` ist das **Ende** des ersten Bandes, nicht ein Start —
/// das erste Band beginnt immer bei 0. Spalten ab dem letzten Eintrag sind
/// gesperrt (Desktop-Test `in_notesset_range(2, 250, …) == false`).
pub fn in_notesset_range(n: i32, x: i32, sets: &[Vec<i32>], setx: &[Vec<i32>]) -> bool {
    for v in 0..sets.len() {
        let cur = setx.get(v).and_then(|s| s.first()).copied();
        let applies = if v == 0 {
            cur.is_some_and(|cx| x < cx)
        } else {
            match (cur, setx.get(v - 1).and_then(|s| s.first()).copied()) {
                (Some(cx), Some(px)) => x < cx && x >= px,
                _ => false,
            }
        };
        if applies && sets[v].contains(&n) {
            return true;
        }
    }
    false
}

impl Harmonie {
    /// Darf die Note an der Spalte `x` klingen? Portierung von
    /// `is_note_in_chord` (mit `harmonics`, `useNotesSet`, `useMajorChords`,
    /// `usePlagalCadence`, `usePerfectFifth`, `noteValueadd`).
    pub fn note_erlaubt(&self, note: i32, x: i32) -> bool {
        if !self.an {
            return true;
        }
        let n = (((note + self.note_value_add) % 12) + 12) % 12;
        if self.notenmenge {
            let count = (self.notessetcount.max(0) as usize)
                .min(self.sets.len())
                .min(self.setx.len());
            return in_notesset_range(n, x, &self.sets[..count], &self.setx[..count]);
        }
        if self.dur {
            n == 0 || n == 4 || (self.quinte && n == 7)
        } else if self.plagal {
            n == 0 || n == 5 || n == 7
        } else {
            n == 0 || n == 3 || (self.quinte && n == 7)
        }
    }

    /// Ein Harmonie-Band anhängen: die Klassen gelten für Spalten bis `bis_x`
    /// (Band 0 ab 0, jedes weitere ab dem Ende des vorigen). Die App hängt die
    /// Bänder aus der Quintenzirkel-Auswahl an; der Desktop füllt sie über
    /// Tastatur-Dialoge in `quintenzirkel_flow`.
    pub fn set_anhaengen(&mut self, klassen: Vec<i32>, bis_x: i32) {
        self.sets.push(klassen);
        self.setx.push(vec![bis_x]);
        self.notessetcount = self.sets.len() as i32;
    }

    /// Wie [`Harmonie::set_anhaengen`], aber mit eigener x-Grenze — so wie im
    /// Desktop-quintenzirkel_flow: dort wird **pro Harmonie** eine x-Stelle im
    /// Bild geklickt (`notessetx[c] = x`). Vorherige Grenzen rutschen nach
    /// rechts, damit die Baender disjunkt bleiben (in_notesset_range erwartet
    /// aufsteigende Grenzen).
    pub fn set_anhaengen_bei(&mut self, klassen: Vec<i32>, bis_x: i32) {
        // Aufsteigend sortieren, damit jedes Band echte Laenge hat.
        let mut grenzen: Vec<i32> = self
            .setx
            .iter()
            .filter_map(|s| s.first().copied())
            .chain(std::iter::once(bis_x))
            .collect();
        grenzen.sort_unstable();
        self.setx = grenzen.into_iter().map(|x| vec![x]).collect();
        // Die alten Klassen gehoeren zu den Grenzen, die schon da waren — sie
        // wandern mit, statt an Ort und Stelle zu bleiben.
        let mut sets: Vec<Vec<i32>> = self.sets.drain(..).take(self.setx.len()).collect();
        sets.push(klassen);
        self.sets = sets;
        // Wenn weniger Klassen als Grenzen da sind, hinten auffüllen, damit
        // `sets[..count]` und `setx[..count]` dieselbe Zahl haben.
        while self.sets.len() < self.setx.len() {
            self.sets.push(Vec::new());
        }
        self.notessetcount = self.sets.len() as i32;
    }

    /// Grenzen und Klassen der gesetzten Harmonien lesbar (Anzeige, Test).
    pub fn baender(&self) -> Vec<(i32, Vec<i32>)> {
        self.setx
            .iter()
            .enumerate()
            .map(|(v, s)| {
                (
                    s.first().copied().unwrap_or_default(),
                    self.sets.get(v).cloned().unwrap_or_default(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests_mehrere_x {
    use super::*;

    #[test]
    fn jede_harmonie_bekommt_ihre_own_x_grenze() {
        // Desktop-quintenzirkel_flow: pro Harmonie eine x-Stelle im Bild.
        let mut h = Harmonie::default();
        h.set_anhaengen_bei(vec![0, 4, 7], 100);
        h.set_anhaengen_bei(vec![2, 5, 9], 220);
        assert_eq!(
            h.baender(),
            vec![(100, vec![0, 4, 7]), (220, vec![2, 5, 9])]
        );
        h.an = true;
        h.notenmenge = true;
        // Zwei Baender mit verschiedenen Klassen an verschiedenen x.
        assert!(h.note_erlaubt(4, 50), "Dur-3 im ersten Band");
        assert!(!h.note_erlaubt(2, 50), "Moll-3 nicht im ersten Band");
        assert!(h.note_erlaubt(2, 150), "Moll-3 im zweiten Band");
        assert!(!h.note_erlaubt(4, 150), "Dur-3 nicht mehr im zweiten Band");
    }

    #[test]
    fn out_of_order_grenzen_werden_sortiert() {
        let mut h = Harmonie::default();
        h.set_anhaengen_bei(vec![0, 4, 7], 300);
        h.set_anhaengen_bei(vec![2, 5, 9], 120);
        // Aufsteigend, sonst haetten die Baender keine Laenge.
        let grenzen: Vec<i32> = h.baender().iter().map(|(x, _)| *x).collect();
        assert!(grenzen.windows(2).all(|w| w[0] < w[1]), "{grenzen:?}");
    }

    #[test]
    fn gleiche_grenz_bleibt_die_letzte_gewinnt() {
        let mut h = Harmonie::default();
        h.set_anhaengen_bei(vec![0, 4], 100);
        h.set_anhaengen_bei(vec![2, 5], 100);
        // Zwei Baender mit derselben Grenze: das erste Band (x<100) traegt die
        // zuletzt gesetzte Klasse, das zweite (100<=x<100) ist leer.
        let b = h.baender();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].0, 100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notenmengen_mit_x_baendern() {
        // Desktop-Test `in_notesset_range_honors_x_bands`.
        let sets = vec![vec![0, 4, 7], vec![2, 5, 9]];
        let setx = vec![vec![100], vec![200]];
        assert!(in_notesset_range(4, 50, &sets, &setx));
        assert!(!in_notesset_range(2, 50, &sets, &setx));
        assert!(in_notesset_range(2, 150, &sets, &setx));
        assert!(!in_notesset_range(4, 150, &sets, &setx));
        assert!(!in_notesset_range(2, 250, &sets, &setx));
        assert!(!in_notesset_range(4, 250, &sets, &setx));
    }

    #[test]
    fn dur_filter_nimmt_null_vier_und_quinte() {
        let h = Harmonie::default();
        assert!(h.an && h.dur && h.quinte);
        for note in [0, 4, 7, 12, 16, 19] {
            assert!(h.note_erlaubt(note, 0), "Klasse {} erlaubt", note % 12);
        }
        for note in [1, 2, 3, 5, 6, 8, 9, 10, 11] {
            assert!(!h.note_erlaubt(note, 0), "Klasse {} gesperrt", note % 12);
        }
    }

    #[test]
    fn quinte_laesst_sich_abschalten() {
        let mut h = Harmonie::default();
        h.quinte = false;
        assert!(h.note_erlaubt(0, 0));
        assert!(h.note_erlaubt(4, 0));
        assert!(!h.note_erlaubt(7, 0), "ohne Quinte keine 7");
    }

    #[test]
    fn plagal_und_moll_je_nach_schaltern() {
        let mut h = Harmonie {
            dur: false,
            plagal: true,
            ..Harmonie::default()
        };
        assert!(h.note_erlaubt(0, 0));
        assert!(h.note_erlaubt(5, 0), "Plagal hat die 5");
        assert!(h.note_erlaubt(7, 0), "Plagal hat die 7 fest");
        assert!(!h.note_erlaubt(4, 0), "kein Dur mehr");
        // Ohne Dur und Plagal bleibt Moll (0,3,7).
        h.plagal = false;
        assert!(h.note_erlaubt(3, 0));
        assert!(!h.note_erlaubt(4, 0));
        assert!(!h.note_erlaubt(5, 0));
        h.quinte = false;
        assert!(!h.note_erlaubt(7, 0));
    }

    #[test]
    fn notenmenge_hat_vorrang_vor_den_akkorden() {
        let mut h = Harmonie {
            notenmenge: true,
            ..Harmonie::default()
        };
        // Band 0 gilt bis x = 100: {2, 5}.
        h.set_anhaengen(vec![2, 5], 100);
        // Klasse 4 waere im Dur-Akkord, aber die Notenmenge gilt.
        assert!(!h.note_erlaubt(4, 10));
        assert!(h.note_erlaubt(2, 10));
        assert!(h.note_erlaubt(5, 50));
        // Hinter dem letzten Band ist gesperrt (Desktop-Verhalten).
        assert!(!h.note_erlaubt(2, 150));
        // Zweites Band ab 100 bis 200: nur Klasse 0.
        h.set_anhaengen(vec![0], 200);
        assert!(h.note_erlaubt(0, 150));
        assert!(!h.note_erlaubt(2, 150));
        assert!(!h.note_erlaubt(0, 210), "hinter dem letzten Band");
    }

    #[test]
    fn baender_treffen_sich_am_ende_des_vorigen() {
        let mut h = Harmonie {
            notenmenge: true,
            ..Harmonie::default()
        };
        h.set_anhaengen(vec![0], 50);
        h.set_anhaengen(vec![7], 100);
        assert!(h.note_erlaubt(0, 49), "erstes Band");
        assert!(!h.note_erlaubt(7, 49));
        assert!(h.note_erlaubt(7, 50), "zweites Band ab 50");
        assert!(!h.note_erlaubt(0, 50));
        assert!(h.note_erlaubt(7, 99));
        assert!(!h.note_erlaubt(7, 100), "Ende exklusiv");
        assert_eq!(h.setx, vec![vec![50], vec![100]]);
        assert_eq!(h.notessetcount, 2);
    }

    #[test]
    fn filter_aus_laesst_alles_durch() {
        let h = Harmonie {
            an: false,
            ..Harmonie::default()
        };
        for note in 0..128 {
            assert!(h.note_erlaubt(note, 0), "Note {note}");
        }
    }

    #[test]
    fn note_value_add_transponiert_die_pruefung() {
        let mut h = Harmonie::default();
        // Note 11 (H) + 1 = 0 (C) -> erlaubt.
        assert!(!h.note_erlaubt(11, 0));
        h.note_value_add = 1;
        assert!(h.note_erlaubt(11, 0));
        // Negative Noten werden normalisiert.
        h.note_value_add = -1;
        assert!(h.note_erlaubt(1, 0), "1 - 1 = 0");
    }
}
