//! Pattern-Raster für den Rhythmus-Screen.
//!
//! Zeitachse wie im Desktop-`pattern_recorder` und im Renoise-Clipboard: ein
//! Pattern-Zyklus geht von 0.0 bis 1.0, ein Schritt belegt `1/steps_per_cycle`.

/// Schritte je Pattern-Zyklus als Vorgabe. Bild-Maske und Rhythmus-Raster
/// teilen sich diese Laenge (eine Quelle: das Pattern).
pub const STEPS_PER_CYCLE: u16 = 512;
/// Obergrenze der waehlbaren Patternlaenge (Rhythmus-Tab und Bild-Schritte).
pub const MAX_STEPS_PER_CYCLE: u16 = 1024;
/// Unter- und Obergrenze der Schrittzahl aus der Bildbreite.
pub const MIN_SCHRITTE: u16 = 4;
pub const MAX_SCHRITTE: u16 = 4096;

/// Schrittzahl aus der **X-Auflösung des Bildes**: ein Schritt je Punkt Breite
/// (wie im Desktop-rrn5 eine Spalte `x;key;iialpha;xkeyk`). Damit ist die
/// Schrittzahl zugleich die Patternlänge; auf ganze Punkte gerundet.
pub fn schritte_aus_breite(breite_pt: f32) -> u16 {
    if !breite_pt.is_finite() || breite_pt <= 0.0 {
        return MIN_SCHRITTE;
    }
    (breite_pt
        .round()
        .max(MIN_SCHRITTE as f32)
        .min(MAX_SCHRITTE as f32)) as u16
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Eine Note auf einem Schritt. `instrument` und `glide` werden nur für den
/// CSV-Austausch mit dem Desktop gebraucht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub step: u16,
    pub instrument: u8,
    pub pitch: i32,
    pub glide: bool,
}

/// Pattern-Raster mit höchstens einer Note je Schritt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    steps_per_cycle: u16,
    notes: Vec<Note>,
    /// Startpunkt des Ausschnitts in der X-Auflösung (0 = ganzes Pattern).
    abschnitt_von: u16,
}

impl Default for Pattern {
    fn default() -> Self {
        Self::new()
    }
}

/// Schritte, die eine Rasterzelle bei `buendel` Schritten je Zelle abdeckt
/// (Anzeige-Regler im Rhythmus-Tab). Die letzte Zelle wird am Patternende
/// gekappt.
pub fn buendel_spanne(zelle: usize, buendel: u16, schritte: u16) -> (u16, u16) {
    let buendel = buendel.max(1) as u32;
    let letzter = schritte.saturating_sub(1) as u32;
    let von = ((zelle as u32) * buendel).min(letzter) as u16;
    let bis = (((zelle as u32 + 1) * buendel).saturating_sub(1)).min(letzter) as u16;
    (von, bis)
}

impl Pattern {
    pub fn new() -> Self {
        Self::with_steps(STEPS_PER_CYCLE)
    }

    pub fn with_steps(steps_per_cycle: u16) -> Self {
        Self {
            steps_per_cycle: steps_per_cycle.max(1),
            notes: Vec::new(),
            abschnitt_von: 0,
        }
    }

    /// Pattern aus einem **Abschnitt** der X-Auflösung: `von` bis `bis`
    /// (einschließlich). Genau wie am Desktop, wo `s` `0` nach „Width?" und
    /// „Start?" fragt — der Ausschnitt bestimmt, wie lang das Pattern wird.
    ///
    /// Ende hinter dem Start beginnt beim Endpunkt (Start 0), Ende außerhalb der
    /// X-Auflösung wird abgeschnitten. Schritte werden mit
    /// [`Pattern::abschnitt_step`] in die Patternlänge übersetzt.
    pub fn mit_abschnitt(von: u16, bis: u16, x_auflosung: u16) -> Self {
        let aufloesung = x_auflosung.max(1);
        let (von, bis) = (von.min(aufloesung - 1), bis.min(aufloesung - 1));
        // Start hinter dem Ende: es gibt kein sinnvolles Fenster, also wird ab
        // dem Endpunkt begonnen (Start 0).
        let (von, bis) = if von > bis { (0, bis) } else { (von, bis) };
        Self {
            steps_per_cycle: (bis - von + 1).max(1),
            notes: Vec::new(),
            abschnitt_von: von,
        }
    }

    /// Startpunkt des Ausschnitts; ohne Ausschnitt 0.
    pub fn abschnitt_von(&self) -> u16 {
        self.abschnitt_von
    }

    /// Übersetzt einen Schritt der X-Auflösung in die Patternlänge. Schritte
    /// außerhalb des Ausschnitts werden umgebrochen, damit eine Note im Bild
    /// immer irgendwo im Pattern landet.
    pub fn abschnitt_step(&self, step: u16) -> u16 {
        // In u32 rechnen: step nahe u16::MAX wuerde sonst ueberlaufen.
        ((step as u32 + self.steps_per_cycle as u32 - self.abschnitt_von as u32)
            % self.steps_per_cycle as u32) as u16
    }

    pub fn steps_per_cycle(&self) -> u16 {
        self.steps_per_cycle
    }

    /// Laenge des Zyklus setzen (1..=128, wie im Rhythmus-Tab waehlbar).
    /// Noten auf Schritten, die es danach nicht mehr gibt, fallen weg — sonst
    /// waeren sie im Raster unsichtbar, aber beim Abspielen noch da.
    pub fn set_steps_per_cycle(&mut self, schritte: u16) {
        let neu = schritte.clamp(1, MAX_STEPS_PER_CYCLE);
        self.steps_per_cycle = neu;
        self.notes.retain(|n| n.step < neu);
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Legt die Note an oder entfernt sie; `true` heißt „angelegt“.
    /// Schritte ausserhalb der Patternlänge werden umgebrochen.
    pub fn toggle(&mut self, step: u16, pitch: i32) -> bool {
        let step = step % self.steps_per_cycle;
        match self.notes.iter().position(|n| n.step == step) {
            Some(idx) => {
                if self.notes[idx].pitch == pitch {
                    self.notes.remove(idx);
                    false
                } else {
                    self.notes[idx].pitch = pitch;
                    true
                }
            }
            None => {
                self.notes.push(Note {
                    step,
                    instrument: 0,
                    pitch,
                    glide: false,
                });
                self.notes.sort_by_key(|n| (n.step, n.pitch));
                true
            }
        }
    }

    /// Setzt eine Note (inklusive Instrument) auf dem Schritt; vorhandene Noten
    /// auf dem Schritt werden ersetzt.
    pub fn insert(&mut self, note: Note) -> bool {
        let step = self.abschnitt_step(note.step);
        self.notes.retain(|n| n.step != step);
        self.notes.push(Note { step, ..note });
        self.notes.sort_by_key(|n| (n.step, n.pitch));
        true
    }

    pub fn has_at(&self, step: u16, pitch: i32) -> bool {
        let step = step % self.steps_per_cycle;
        self.notes
            .iter()
            .any(|n| n.step == step && n.pitch == pitch)
    }

    /// Buendel (Anzeige): liegt in `[von, bis]` (beide eingeschlossen) eine
    /// Note dieser Hoehe?
    pub fn hat_im_buendel(&self, von: u16, bis: u16, pitch: i32) -> bool {
        let n = self.steps_per_cycle;
        let (von, bis) = (von % n, bis % n);
        self.notes
            .iter()
            .any(|note| note.pitch == pitch && note.step >= von && note.step <= bis)
    }

    /// Buendel schalten: hat es schon eine Note dieser Hoehe, werden **alle**
    /// Schritte des Buendels entfernt; sonst werden **alle** Schritte gesetzt
    /// (Nutzerentscheidung 2026-10-14: „Tippen setzt/loescht alle Schritte
    /// der Zelle“). `true` heisst „gesetzt“.
    pub fn toggle_buendel(&mut self, von: u16, bis: u16, pitch: i32) -> bool {
        let n = self.steps_per_cycle;
        let (von, bis) = (von % n, bis % n);
        let (von, bis) = (von.min(bis), von.max(bis));
        if self.hat_im_buendel(von, bis, pitch) {
            self.notes
                .retain(|note| !(note.pitch == pitch && note.step >= von && note.step <= bis));
            false
        } else {
            for step in von..=bis {
                self.toggle(step, pitch);
            }
            true
        }
    }

    pub fn clear(&mut self) {
        self.notes.clear();
    }

    /// Alle Noten im Schrittbereich `von..=bis` löschen (Portierung des
    /// „remove“-Teils von `remove_and_record`; der Desktop fragt die Grenzen
    /// interaktiv ab).
    pub fn bereich_loeschen(&mut self, von: u16, bis: u16) -> usize {
        let (von, bis) = (von.min(bis), von.max(bis));
        let vorher = self.notes.len();
        self.notes.retain(|n| n.step < von || n.step > bis);
        vorher - self.notes.len()
    }

    /// Zeit des Schritts in Zyklen (0.0 bis 1.0), Format des Desktop-CSV.
    pub fn timestamp(&self, step: u16) -> f32 {
        (step % self.steps_per_cycle) as f32 / self.steps_per_cycle as f32
    }

    /// Umkehrung von [`Pattern::timestamp`], mit Überlauf um die Patternlänge.
    pub fn step_from_timestamp(&self, timestamp: f32) -> u16 {
        let steps = (timestamp * self.steps_per_cycle as f32).round();
        let steps = if steps.is_finite() && steps > 0.0 {
            steps as i64
        } else {
            0
        };
        (steps % self.steps_per_cycle as i64) as u16
    }

    /// CSV wie im Desktop-`pattern_recorder`: `timestamp,instrument,pitch,glide`.
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        for note in &self.notes {
            out += &format!(
                "{},{},{},{}\n",
                self.timestamp(note.step),
                note.instrument,
                note.pitch,
                if note.glide { 1 } else { 0 }
            );
        }
        out
    }

    /// Schism-Clipboard-Format wie im Desktop-Recorder
    /// (`notes_to_schism_pattern`): Kopfzeile „Pasted Pattern - IT“, dann je
    /// Schritt eine 12 Zeichen breite Zeile `|<Note>00..|...`; leere Schritte
    /// sind `|...........` („|“ + 10 Punkte + Abschlusspunkt). Noten ohne Tonhöhe
    /// (0 oder negativ) fallen weg.
    pub fn to_schism(&self) -> String {
        let mut out = String::from("Pasted Pattern - IT\n");
        for schritt in 0..self.steps_per_cycle {
            let mut zeile = String::new();
            let mut getroffen = false;
            for note in &self.notes {
                if note.step == schritt && note.pitch > 0 {
                    zeile += "|";
                    zeile += &note_name(note.pitch);
                    zeile += "00..";
                    zeile += "...";
                    zeile += ".\n";
                    getroffen = true;
                }
            }
            if !getroffen {
                // Wie am Desktop: „|" + 10 Punkte + Abschlusspunkt = 12 Zeichen.
                zeile += "|...........\n";
            }
            out += &zeile;
        }
        out
    }

    /// Liest das Desktop-CSV; Schritte ausserhalb der Patternlänge fallen weg.
    pub fn from_csv(content: &str, steps_per_cycle: u16) -> Result<Self, String> {
        let steps_per_cycle = steps_per_cycle.max(1);
        let mut pattern = Self::with_steps(steps_per_cycle);
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let tokens: Vec<&str> = line.split(',').collect();
            if tokens.len() < 4 {
                continue;
            }
            let timestamp: f32 = tokens[0]
                .trim()
                .parse()
                .map_err(|e| format!("Zeile „{line}“: {e}"))?;
            if !timestamp.is_finite() {
                continue;
            }
            // Abschneiden wie der Desktop (`(ts / time_per_line) as i32`), sonst
            // fallen Timestamps am Patternende durchs Runden weg.
            let raw = (timestamp * steps_per_cycle as f32) as i64;
            if !(0..steps_per_cycle as i64).contains(&raw) {
                continue;
            }
            let step = raw as u16;
            let instrument: u8 = tokens[1].trim().parse().unwrap_or(0);
            let pitch: i32 = tokens[2]
                .trim()
                .parse()
                .map_err(|e| format!("Zeile „{line}“: {e}"))?;
            let glide = matches!(
                tokens[3].trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            );
            pattern.notes.retain(|n| n.step != step);
            pattern.notes.push(Note {
                step,
                instrument,
                pitch,
                glide,
            });
        }
        pattern.notes.sort_by_key(|n| (n.step, n.pitch));
        Ok(pattern)
    }

    /// Renoise-Clipboard-XML zum Einfügen in ein Pattern-Fenster.
    pub fn to_renoise_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n");
        xml += "<PatternClipboard.BlockBuffer doc_version=\"0\">\n";
        xml += "  <Columns>\n";
        xml += "    <Column>\n";
        xml += "      <Column>\n";
        xml += "        <Lines>\n";
        for step in 0..self.steps_per_cycle {
            let note = self.notes.iter().find(|n| n.step == step);
            match note {
                None => xml += &format!("          <Line index=\"{step}\"/>\n"),
                Some(note) => {
                    xml += &format!("          <Line index=\"{step}\">\n");
                    xml += "            <NoteColumns>\n";
                    xml += "              <NoteColumn>\n";
                    xml += &format!("                <Note>{}</Note>\n", note_name(note.pitch));
                    // Der Desktop liest das Instrument als Hex mit Padding ("00").
                    xml += &format!(
                        "                <Instrument>{:02X}</Instrument>\n",
                        note.instrument
                    );
                    xml += "              </NoteColumn>\n";
                    xml += "            </NoteColumns>\n";
                    xml += "          </Line>\n";
                }
            }
        }
        xml += "        </Lines>\n";
        xml += "        <ColumnType>NoteColumn</ColumnType>\n";
        xml +=
            "        <SubColumnMask>true true true false false false false false</SubColumnMask>\n";
        xml += "      </Column>\n";
        xml += "    </Column>\n";
        xml += "  </Columns>\n";
        xml += "</PatternClipboard.BlockBuffer>\n";
        xml
    }

    /// Liest ein Renoise-Clipboard-XML; je Zeile zählt die erste Note.
    pub fn from_renoise_xml(content: &str, steps_per_cycle: u16) -> Result<Self, String> {
        if !content.contains("<PatternClipboard.BlockBuffer") {
            return Err("kein PatternClipboard-XML".to_string());
        }
        let mut pattern = Self::with_steps(steps_per_cycle);
        let mut step: Option<u16> = None;
        // `<Note>` steht im XML vor `<Instrument>`, deshalb merken und beim
        // Instrument (oder bei der naechsten Zeile) schreiben.
        let mut pending: Option<(u16, i32)> = None;
        for tag_line in content.lines() {
            let line = tag_line.trim();
            if let Some(rest) = line.strip_prefix("<Line index=") {
                if let Some((step, pitch)) = pending.take() {
                    pattern.insert(Note {
                        step,
                        instrument: 0,
                        pitch,
                        glide: false,
                    });
                }
                step = rest
                    .trim_end_matches("/>")
                    .trim_end_matches('>')
                    .trim_matches('"')
                    .parse::<u16>()
                    .ok()
                    .filter(|s| *s < pattern.steps_per_cycle);
            }
            if let Some(name) = line
                .strip_prefix("<Note>")
                .and_then(|r| r.strip_suffix("</Note>"))
            {
                let Some(step) = step else { continue };
                if let Some(pitch) = pitch_from_note_name(name.trim()) {
                    pending = Some((step, pitch));
                }
            }
            // Renoise und der Desktop-Recorder schreiben das Instrument als
            // Hex mit fuehrender Null ("00"); ein unlesbarer Wert zaehlt als 0.
            if let Some(rest) = line
                .strip_prefix("<Instrument>")
                .and_then(|r| r.strip_suffix("</Instrument>"))
            {
                let instrument = u8::from_str_radix(rest.trim(), 16).unwrap_or(0);
                if let Some((step, pitch)) = pending.take() {
                    pattern.insert(Note {
                        step,
                        instrument,
                        pitch,
                        glide: false,
                    });
                }
            }
        }
        if let Some((step, pitch)) = pending {
            pattern.insert(Note {
                step,
                instrument: 0,
                pitch,
                glide: false,
            });
        }
        pattern.notes.sort_by_key(|n| (n.step, n.pitch));
        Ok(pattern)
    }
}

/// Renoise-Notenname: 60 ist `C-4`, 61 ist `C#4`, 0 ist `C--1`, -1 heisst `Off`.
/// Der Trennstrich gehört zum Naturton, der Halbton hängt am Buchstaben; in
/// negativer Oktave kommt ein zweiter Strich als Vorzeichen dazu.
pub fn note_name(pitch: i32) -> String {
    if pitch < 0 {
        return "Off".to_string();
    }
    let semitone = pitch.rem_euclid(12) as usize;
    let octave = pitch / 12 - 1;
    // Halbtöne hängen am Buchstaben, Naturtöne bekommen einen Trennstrich; in
    // negativer Oktave steht das Vorzeichen vor der Zahl (`C--1`, `C#--1`).
    let trennstrich = if NOTE_NAMES[semitone].ends_with('#') && octave >= 0 {
        ""
    } else {
        "-"
    };
    let oktave = octave.to_string();
    format!("{}{}{}", NOTE_NAMES[semitone], trennstrich, oktave)
}

/// Umkehrung von [`note_name`]; `Off` und unbekannte Namen ergeben `None`.
///
/// Gelesen werden die kanonischen Renoise-Formen: Halbton ohne Trennstrich
/// (`C#4`), Naturton mit Trennstrich (`C-4`), negative Oktave mit zusätzlichem
/// Strich (`C--1`, `C#--1`). Die kürzere Schreibweise des Desktop-Recorders
/// (`C4`) wird ebenfalls akzeptiert.
pub fn pitch_from_note_name(name: &str) -> Option<i32> {
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("off") || name.eq_ignore_ascii_case("empty") {
        return None;
    }
    let digits = name.find(|c: char| c.is_ascii_digit())?;
    let (letters, octave_text) = (&name[..digits], &name[digits..]);
    let dashes = letters.len() - letters.trim_end_matches('-').len();
    if dashes > 2 {
        return None;
    }
    let base_letters = letters.trim_end_matches('-');
    let sharps = base_letters.bytes().filter(|b| *b == b'#').count() as i32;
    let base = base_letters.trim_end_matches('#');
    let index = NOTE_NAMES.iter().position(|n| *n == base)? as i32;
    let octave: i32 = if dashes == 2 {
        format!("-{}", octave_text).parse().ok()?
    } else {
        octave_text.parse().ok()?
    };
    Some((octave + 1) * 12 + index + sharps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern_with(notes: &[(u16, i32)]) -> Pattern {
        // 16 Schritte: diese Tests pruefen konkrete Schrittnummern aus der
        // Zeit vor der 512er-Vorgabe. Die Vorgabe selbst prueft
        // `neue_patterns_sind_512_schritte_lang`.
        let mut p = Pattern::with_steps(16);
        for &(step, pitch) in notes {
            p.toggle(step, pitch);
        }
        p
    }

    #[test]
    fn neue_patterns_sind_512_schritte_lang() {
        // Vorgabe des Nutzers (2026-10-11): Pattern und Bild-Maske teilen sich
        // **eine** Laenge, und die startet bei 512.
        assert_eq!(Pattern::new().steps_per_cycle(), 512);
        assert!(
            MAX_STEPS_PER_CYCLE >= STEPS_PER_CYCLE,
            "die Vorgabe muss waehlbar sein"
        );
    }

    #[test]
    fn buendel_spanne_deckt_die_zelle() {
        // Nutzerwunsch 2026-10-14: bei Buendelung 16 deckt eine Zelle die
        // Schritte ab, die sie zusammenfasst.
        assert_eq!(buendel_spanne(0, 2, 512), (0, 1));
        assert_eq!(buendel_spanne(255, 2, 512), (510, 511));
        assert_eq!(buendel_spanne(2, 4, 10), (8, 9), "letzte Zelle gekappt");
        assert_eq!(buendel_spanne(9, 4, 10), (9, 9), "hinter dem Ende");
        assert_eq!(buendel_spanne(3, 1, 512), (3, 3), "Buendel 1 = Schritt");
    }

    #[test]
    fn buendel_leuchtet_bei_einer_note() {
        let mut p = Pattern::with_steps(16);
        p.toggle(3, 60);
        assert!(p.hat_im_buendel(2, 3, 60), "Note 3 im Buendel 2..3");
        assert!(!p.hat_im_buendel(4, 5, 60));
        assert!(!p.hat_im_buendel(2, 3, 62), "andere Hoehe");
    }

    #[test]
    fn toggle_buendel_setzt_und_loescht_das_ganze_buendel() {
        let mut p = Pattern::with_steps(512);
        assert!(p.toggle_buendel(4, 7, 60), "leer -> setzen");
        assert_eq!(p.notes().len(), 4, "vier Schritte");
        assert!((4..=7).all(|s| p.has_at(s, 60)));
        assert!(!p.toggle_buendel(4, 7, 60), "belegt -> loeschen");
        assert!(p.is_empty());
    }

    #[test]
    fn toggle_adds_and_removes_and_reports_it() {
        let mut p = Pattern::new();
        assert!(p.toggle(4, 60), "erstes Toggle legt an");
        assert!(!p.toggle(4, 60), "zweites Toggle entfernt");
        assert!(p.is_empty());
    }

    #[test]
    fn toggle_keeps_one_note_per_step() {
        let mut p = Pattern::new();
        p.toggle(4, 60);
        p.toggle(4, 72);
        assert_eq!(p.notes().len(), 1, "gleicher Schritt wird ersetzt");
        assert_eq!(p.notes()[0].pitch, 72);
    }

    #[test]
    fn laenge_ist_von_1_bis_128_waehlbar() {
        let mut p = Pattern::with_steps(16);
        for laenge in [1_u16, 3, 64, 128] {
            p.set_steps_per_cycle(laenge);
            assert_eq!(p.steps_per_cycle(), laenge, "Laenge {laenge}");
        }
        // Ausserhalb geklemmt, damit die Anzeige nie einen leeren Zyklus zeigt.
        p.set_steps_per_cycle(0);
        assert_eq!(p.steps_per_cycle(), 1, "unter 1 wird auf 1 geklemmt");
        p.set_steps_per_cycle(5000);
        assert_eq!(
            p.steps_per_cycle(),
            MAX_STEPS_PER_CYCLE,
            "ueber 128 wird geklemmt"
        );
    }

    #[test]
    fn verkuerzen_verliert_die_noten_ausserhalb() {
        let mut p = Pattern::with_steps(16);
        p.insert(Note {
            step: 0,
            instrument: 0,
            pitch: 60,
            glide: false,
        });
        p.insert(Note {
            step: 5,
            instrument: 0,
            pitch: 64,
            glide: false,
        });
        p.insert(Note {
            step: 15,
            instrument: 0,
            pitch: 67,
            glide: false,
        });
        p.set_steps_per_cycle(6);
        assert_eq!(p.steps_per_cycle(), 6);
        assert_eq!(
            p.notes().iter().map(|n| n.step).collect::<Vec<_>>(),
            vec![0, 5],
            "der Schritt 15 liegt jetzt ausserhalb und faellt weg"
        );
        // Verlaengern holt nichts zurueck — die Noten waren weg.
        p.set_steps_per_cycle(16);
        assert_eq!(p.notes().len(), 2, "Verlaengern erzeugt keine Noten");
    }

    #[test]
    fn bei_laenge_ein_bleibt_erste_note_erhalten() {
        let mut p = Pattern::with_steps(16);
        p.insert(Note {
            step: 0,
            instrument: 0,
            pitch: 60,
            glide: false,
        });
        p.insert(Note {
            step: 3,
            instrument: 0,
            pitch: 60,
            glide: false,
        });
        p.set_steps_per_cycle(1);
        assert_eq!(p.notes().len(), 1);
        assert_eq!(p.notes()[0].step, 0);
    }

    #[test]
    fn clear_empties_the_pattern() {
        let mut p = pattern_with(&[(0, 36), (8, 48)]);
        p.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn schritte_aus_breite_ist_ein_schritt_je_punkt() {
        assert_eq!(schritte_aus_breite(317.0), 317, "Hochformat");
        assert_eq!(schritte_aus_breite(872.0), 872, "Querformat");
        assert_eq!(schritte_aus_breite(317.4), 317, "gerundet");
        assert_eq!(schritte_aus_breite(0.0), MIN_SCHRITTE, "leer = Minimum");
        assert_eq!(schritte_aus_breite(-5.0), MIN_SCHRITTE);
        assert_eq!(schritte_aus_breite(f32::NAN), MIN_SCHRITTE);
        assert_eq!(schritte_aus_breite(99_999.0), MAX_SCHRITTE, "Obergrenze");
        assert_eq!(schritte_aus_breite(2.0), MIN_SCHRITTE, "unter dem Minimum");
    }

    #[test]
    fn csv_bleibt_normiert_0_bis_1() {
        // Dieselben Zeitpunkte, zwei Patternlängen: der CSV bleibt 0..1, nur die
        // Auflösung (steps_per_cycle) unterscheidet sich.
        let mut a = Pattern::with_steps(16);
        let mut b = Pattern::with_steps(40);
        a.insert(Note {
            step: 0,
            instrument: 0,
            pitch: 36,
            glide: false,
        });
        b.insert(Note {
            step: 0,
            instrument: 0,
            pitch: 36,
            glide: false,
        });
        assert_eq!(a.to_csv(), b.to_csv());
        assert_eq!(a.to_csv(), "0,0,36,0\n");
    }

    #[test]
    fn einlesen_quantisiert_nach_der_aufloesung() {
        let csv = "0.025,0,60,0\n0.5,0,64,0\n";
        let fein = Pattern::from_csv(csv, 40).unwrap();
        assert_eq!(fein.notes()[0].step, 1, "0,025 * 40 = 1");
        assert_eq!(fein.notes()[1].step, 20);
        let grob = Pattern::from_csv(csv, 16).unwrap();
        assert_eq!(grob.notes()[0].step, 0, "0,025 * 16 = 0,4 gerundet 0");
        assert_eq!(grob.notes()[1].step, 8);
    }

    #[test]
    fn xml_indiziert_je_schritt_bis_zum_letzten() {
        let mut p = Pattern::with_steps(40);
        p.insert(Note {
            step: 39,
            instrument: 0,
            pitch: 72,
            glide: false,
        });
        let xml = p.to_renoise_xml();
        assert!(xml.contains("Line index=\"39\""), "letzter Schritt: {xml}");
        assert!(!xml.contains("index=\"40\""), "nur bis 39");
        // Kuerzere Laenge: Schritte ausserhalb fallen weg (kein Umlauf).
        assert!(
            Pattern::from_renoise_xml(&xml, 16)
                .unwrap()
                .notes()
                .is_empty(),
            "Schritt 39 passt nicht in 16 Schritte"
        );
        // Genau diese Laenge liest die Note zurueck.
        assert_eq!(
            Pattern::from_renoise_xml(&xml, 40).unwrap().notes()[0].step,
            39
        );
    }

    #[test]
    fn start_und_endpunkt_waehlen_den_abschnitt() {
        // Der Nutzer legt fest, welcher Abschnitt der X-Aufloesung in die
        // Patternlaenge kommt — so wird aus dem Bild ein Pattern von gewaehlter
        // Laenge. Die Noten zaehlen ab dem Startpunkt wieder bei 0.
        let mut p = Pattern::mit_abschnitt(4, 20, 40);
        assert_eq!(p.steps_per_cycle(), 17, "Schritte 4..=20");
        // Noten kommen in X-Aufloesungs-Schritten und landen im Ausschnitt bei 0.
        p.insert(Note {
            step: 4,
            instrument: 0,
            pitch: 36,
            glide: false,
        });
        p.insert(Note {
            step: 20,
            instrument: 0,
            pitch: 72,
            glide: false,
        });
        // Der CSV bleibt normiert 0..1: Schritt 16 von 17 ist 16/17.
        assert_eq!(p.to_csv(), "0,0,36,0\n0.9411765,0,72,0\n");
        let xml = p.to_renoise_xml();
        assert!(xml.contains("index=\"0\""), "{xml}");
        assert!(xml.contains("index=\"16\""), "letzter Schritt: {xml}");
    }

    #[test]
    fn bereich_loeschen_trifft_nur_den_bereich() {
        let mut p = pattern_with(&[(0, 36), (3, 60), (4, 62), (8, 64), (15, 72)]);
        let geloescht = p.bereich_loeschen(3, 8);
        assert_eq!(geloescht, 3, "Schritte 3, 4 und 8");
        let steps: Vec<u16> = p.notes().iter().map(|n| n.step).collect();
        assert_eq!(steps, vec![0, 15]);
        // Verdrehte Grenzen werden getauscht, leerer Bereich loescht nichts.
        assert_eq!(p.bereich_loeschen(9, 2), 0);
        assert_eq!(p.notes().len(), 2);
    }

    #[test]
    fn abschnitt_begrenzt_die_schritte() {
        let p = Pattern::mit_abschnitt(10, 10, 40);
        assert_eq!(p.steps_per_cycle(), 1, "nur ein Schritt");
        assert_eq!(p.abschnitt_von(), 10);
        // Schritte werden in die Patternlaenge uebersetzt und umgebrochen.
        assert_eq!(p.abschnitt_step(10), 0);
        assert_eq!(p.abschnitt_step(9), 0, "eins davor, Umlauf");
        assert_eq!(p.abschnitt_step(11), 0, "eins danach, Umlauf");

        let mut ganz = Pattern::mit_abschnitt(4, 20, 40);
        assert_eq!(ganz.abschnitt_step(4), 0, "Startpunkt = 0");
        assert_eq!(ganz.abschnitt_step(20), 16, "Endpunkt = letzter Schritt");
        assert_eq!(ganz.abschnitt_step(2), 15, "davor umgebrochen");
        ganz.insert(Note {
            step: 20,
            instrument: 0,
            pitch: 60,
            glide: false,
        });
        assert_eq!(ganz.notes()[0].step, 16);
        // Start hinter dem Ende: es gibt kein sinnvolles Fenster, also wird mit dem
        // Endpunkt begonnen und der Start auf 0 gesetzt.
        let verkehrt = Pattern::mit_abschnitt(40, 3, 16);
        assert_eq!(verkehrt.steps_per_cycle(), 4, "0..=3");
        assert_eq!(verkehrt.abschnitt_von(), 0);
        // Ausserhalb der X-Aufloesung wird abgeschnitten.
        let zu_breit = Pattern::mit_abschnitt(0, 99, 16);
        assert_eq!(zu_breit.steps_per_cycle(), 16, "ganze Aufloesung");
        assert_eq!(zu_breit.abschnitt_von(), 0);
    }

    #[test]
    fn schism_export_trifft_das_clipboard_format() {
        // Regression aus dem Desktop-Recorder
        // (`notes_to_schism_pattern_matches_clipboard_format`): gleiche Noten,
        // gleiche Zeilen, gleiche Zellenbreite.
        let mut p = Pattern::with_steps(64);
        for (step, pitch) in [(0, 60), (21, 60), (32, 61), (42, 60), (49, 61), (56, 62)] {
            p.insert(Note {
                step,
                instrument: 0,
                pitch,
                glide: false,
            });
        }
        let s = p.to_schism();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], "Pasted Pattern - IT");
        assert_eq!(lines.len(), 65, "Kopfzeile + ein Schritt je Zeile");
        for l in &lines[1..] {
            assert_eq!(l.len(), 12, "Zellenbreite: {l}");
        }
        assert_eq!(lines[1], "|C-400......");
        assert_eq!(lines[22], "|C-400......");
        assert_eq!(lines[33], "|C#400......");
        assert_eq!(lines[43], "|C-400......");
        assert_eq!(lines[50], "|C#400......");
        assert_eq!(lines[57], "|D-400......");
        assert_eq!(Pattern::with_steps(3).to_schism().lines().count(), 4);
        assert_eq!(
            Pattern::with_steps(2).to_schism().lines().next().unwrap(),
            "Pasted Pattern - IT"
        );
    }

    #[test]
    fn csv_roundtrip_keeps_step_and_pitch() {
        let p = pattern_with(&[(0, 36), (3, 60), (15, 72)]);
        let parsed = Pattern::from_csv(&p.to_csv(), 16).expect("CSV lesbar");
        assert_eq!(parsed.notes(), p.notes());
    }

    #[test]
    fn csv_uses_desktop_format_timestamp_instrument_pitch_glide() {
        let p = pattern_with(&[(8, 60)]);
        assert_eq!(p.to_csv(), "0.5,0,60,0\n");
    }

    #[test]
    fn csv_accepts_true_false_and_zero_one_for_glide() {
        let parsed = Pattern::from_csv("0,0,60,true\n0.5,1,61,1\n", 16).unwrap();
        assert!(parsed.notes()[0].glide);
        assert!(parsed.notes()[1].glide);
    }

    #[test]
    fn csv_ignores_zeilen_mit_zu_wenigen_spalten() {
        let parsed = Pattern::from_csv("0,0,60,0\nquatsch\n0.5,2\n", 16).unwrap();
        assert_eq!(parsed.notes().len(), 1);
    }

    #[test]
    fn step_und_timestamp_rechnen_ineinander() {
        // Das Verhaeltnis gilt fuer jede Laenge; die konkreten Zahlen dieses
        // Tests stammen aus dem 16er-Raster.
        let p = Pattern::with_steps(16);
        assert!((p.timestamp(4) - 0.25).abs() < 1e-6);
        assert_eq!(p.step_from_timestamp(0.25), 4);
        assert_eq!(
            p.step_from_timestamp(1.25),
            4,
            "Positionen ausserhalb werden umgebrochen"
        );
    }

    #[test]
    fn notennamen_entsprechen_renoise_konvention() {
        assert_eq!(note_name(60), "C-4");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(0), "C--1");
        assert_eq!(note_name(-1), "Off");
        assert_eq!(note_name(127), "G-9");
        assert_eq!(note_name(128), "G#9");
    }

    #[test]
    fn notennamen_lassen_sich_zurueckrechnen() {
        for pitch in [0, 12, 36, 60, 61, 69, 127] {
            assert_eq!(
                pitch_from_note_name(&note_name(pitch)),
                Some(pitch),
                "{pitch}"
            );
        }
        assert_eq!(pitch_from_note_name("Off"), None);
        assert_eq!(pitch_from_note_name("H4"), None);
    }

    #[test]
    fn notennamen_rechnen_fuer_alle_tonarten_und_oktaven() {
        for pitch in 0..128 {
            assert_eq!(
                pitch_from_note_name(&note_name(pitch)),
                Some(pitch),
                "{} -> {}",
                pitch,
                note_name(pitch)
            );
        }
    }

    #[test]
    fn notennamen_lesen_kanonische_und_kurzform() {
        assert_eq!(pitch_from_note_name("C-4"), Some(60));
        assert_eq!(pitch_from_note_name("C#4"), Some(61));
        assert_eq!(pitch_from_note_name("C--1"), Some(0));
        assert_eq!(pitch_from_note_name("C#--1"), Some(1));
        assert_eq!(pitch_from_note_name("D#--1"), Some(3));
        assert_eq!(pitch_from_note_name("G#--1"), Some(8));
        assert_eq!(pitch_from_note_name("A#--1"), Some(10));
        assert_eq!(
            pitch_from_note_name("C4"),
            Some(60),
            "Desktop-Schreibweise ohne Trennstrich"
        );
    }

    #[test]
    fn notennamen_lehnen_unmoegliches_ab() {
        assert_eq!(pitch_from_note_name("C---4"), None);
        assert_eq!(pitch_from_note_name("C"), None);
        assert_eq!(pitch_from_note_name(""), None);
        assert_eq!(pitch_from_note_name("Empty"), None);
    }

    #[test]
    fn renoise_xml_setzt_noten_je_zeile() {
        let p = pattern_with(&[(0, 60), (1, 60)]);
        let xml = p.to_renoise_xml();
        assert!(xml.starts_with(
            "<?xml version=\"1.0\"?>\n<PatternClipboard.BlockBuffer doc_version=\"0\">"
        ));
        assert!(xml.contains("<Line index=\"0\">\n            <NoteColumns>\n              <NoteColumn>\n                <Note>C-4</Note>\n                <Instrument>00</Instrument>\n              </NoteColumn>\n            </NoteColumns>\n          </Line>"), "{xml}");
        assert!(!xml.contains("<Line index=\"1\"/>"));
        assert!(xml.contains("          <Line index=\"2\"/>\n"));
        assert!(xml.ends_with(
            "        <ColumnType>NoteColumn</ColumnType>\n        <SubColumnMask>true true true false false false false false</SubColumnMask>\n      </Column>\n    </Column>\n  </Columns>\n</PatternClipboard.BlockBuffer>\n"
        ));
    }

    #[test]
    fn csv_verwirft_schritte_ausserhalb_der_patternlaenge() {
        let p = Pattern::from_csv("1.25,0,60,0\n", 16).expect("CSV lesbar");
        assert!(p.is_empty(), "Schritt 20 passt nicht in 16 Schritte");
    }

    #[test]
    fn csv_behaelt_noten_am_patternende() {
        let p = Pattern::from_csv("0.97,0,60,0\n0.98,1,61,0\n", 16).expect("CSV lesbar");
        assert_eq!(p.notes().len(), 1, "beide Zeilen landen auf Schritt 15");
        assert_eq!(
            p.notes()[0].step,
            15,
            "0.97 darf nicht wegfallen wie beim Runden"
        );
        assert_eq!(p.notes()[0].pitch, 61, "der letzte Eintrag gewinnt");
    }

    #[test]
    fn csv_ignoriert_nan_und_unendliche_zeit() {
        let p = Pattern::from_csv("nan,0,60,0\ninf,0,61,0\n0.5,0,62,0\n", 16).unwrap();
        assert_eq!(p.notes().len(), 1);
        assert_eq!(p.notes()[0].pitch, 62);
    }

    #[test]
    fn csv_meldet_kaputten_pitch() {
        let err = Pattern::from_csv("0,0,keine_note,0\n", 16).unwrap_err();
        assert!(err.contains("keine_note"), "{err}");
    }

    #[test]
    fn csv_ignoriert_zusaetzliche_spalten_und_kr_zeilen() {
        let p = Pattern::from_csv("0,0,60,0,9,9\r\n0.5,1,61,1\n", 16).unwrap();
        assert_eq!(
            p.notes().len(),
            2,
            "CR am Zeilenende darf die Zeile nicht fressen"
        );
    }

    #[test]
    fn with_steps_null_wird_ein_schritt() {
        let mut p = Pattern::with_steps(0);
        assert_eq!(p.steps_per_cycle(), 1);
        p.toggle(7, 60);
        assert_eq!(
            p.notes()[0].step,
            0,
            "jeder Schritt ist bei einem Zyklus Schritt 0"
        );
    }

    #[test]
    fn instrument_bleibt_ueber_den_xml_leserlauf() {
        let mut p = Pattern::new();
        p.insert(Note {
            step: 0,
            instrument: 10,
            pitch: 60,
            glide: false,
        });
        let xml = p.to_renoise_xml();
        assert!(
            xml.contains("<Instrument>0A</Instrument>"),
            "Hex mit Padding"
        );
        let back = Pattern::from_renoise_xml(&xml, 16).unwrap();
        assert_eq!(
            back.notes()[0].instrument,
            10,
            "Instrument muss den XML-Lauf überstehen"
        );
    }

    #[test]
    fn toggle_bricht_schritte_auf_die_patternlaenge_um() {
        let p = pattern_with(&[(20, 60)]);
        assert_eq!(p.notes()[0].step, 4);
    }

    #[test]
    fn renoise_xml_liest_sich_zurueck() {
        let p = pattern_with(&[(0, 60), (5, 72), (9, 36)]);
        let parsed = Pattern::from_renoise_xml(&p.to_renoise_xml(), 16).expect("XML lesbar");
        assert_eq!(parsed.notes(), p.notes());
    }

    #[test]
    fn renoise_xml_meldet_fehlende_zeilen_root() {
        assert!(Pattern::from_renoise_xml("<html>", 16).is_err());
    }
}
