//! Impulse-Tracker-(IT)-Dateien schreiben: Slices als Samples, Pattern leer.
//!
//! Nutzerwunsch: ein geschnittenes WAV als `.it` sichern (Slices als
//! Samples/Instrumente, Pattern leer) und **später** einzelne Patterns
//! nachreichen. Deshalb ist der Writer zweigeteilt:
//!
//! - [`bytes`]/[`schreibe`] schreiben Samples + ein leeres 64-Zeilen-Pattern.
//! - [`bytes_mit_patterns`] nimmt fertige [`Pattern`]-Daten entgegen (je
//!   Pattern eine Order 0, 1, 2, …). Dort werden später die aufgenommenen
//!   Patterns eingehängt — der Rest der Datei bleibt unverändert.
//!
//! Dateiaufbau (ITTECH.TXT; gegengeprüft mit libopenmpt `soundlib/Load_it.cpp`
//! und echten IT-Dateien):
//!
//! | Offset              | Inhalt                                          |
//! |---------------------|-------------------------------------------------|
//! | 0x0000, 192 Bytes   | Kopf `IMPM` (Felder siehe [`kopf`])             |
//! | 0x00C0              | Order-Liste (`OrdNum` Bytes)                    |
//! | danach              | Instrument-Parapointer (`InsNum` × 4) — leer    |
//! | danach              | Sample-Parapointer (`SMPNum` × 4)               |
//! | danach              | Pattern-Parapointer (`PatNum` × 4)              |
//! | danach              | Sample-Köpfe (je 80 Bytes, `IMPS`)              |
//! | danach              | Pattern-Daten (nur gefüllte Patterns)           |
//! | danach              | PCM-Daten (16 Bit little-endian, mono)          |
//!
//! Ein **leeres 64-Zeilen-Pattern** wird wie in OpenMPT als Parapointer `0`
//! abgelegt; libopenmpt legt dafür 64 leere Zeilen an (`Load_it.cpp`,
//! „Empty 64-row pattern"). Ein gefülltes Pattern wird gepackt geschrieben
//! (Kanalmaske je Zeile, Zeilenende `0x00`).

/// Ein Sample für die .it-Datei (16 Bit PCM, mono).
#[derive(Clone, Debug)]
pub struct Sample {
    pub name: String,
    /// PCM-Daten, 16 Bit signed; Länge in Samples.
    pub daten: Vec<i16>,
    /// Abtastrate der Daten (z. B. 44100).
    pub rate: u32,
    /// C-5-Frequenz im Sample-Kopf; `0` heißt: [`Sample::rate`] verwenden.
    pub c5speed: u32,
}

/// Eine Pattern-Zelle; `None` heißt leer.
///
/// Die Werte sind die rohen IT-Bytes: `note` 0…119 (0 = C-0),
/// 254 = Note-Cut, 255 = Note-Off; `instrument` 1-basiert.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zelle {
    pub note: Option<u8>,
    pub instrument: Option<u8>,
    pub volume: Option<u8>,
    pub befehl: Option<u8>,
    /// Nur zusammen mit [`Zelle::befehl`] erlaubt.
    pub parameter: Option<u8>,
}

/// Ein Pattern: `zeilen` × `kanaele` Zellen, zeilenweise abgelegt.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub zeilen: u16,
    pub kanaele: u16,
    pub zellen: Vec<Zelle>,
}

impl Pattern {
    /// Leeres 64-Zeilen-Pattern (wird als Parapointer `0` geschrieben).
    pub fn leer() -> Self {
        Pattern {
            zeilen: 64,
            kanaele: 0,
            zellen: Vec::new(),
        }
    }

    /// Keine einzige Zelle belegt?
    pub fn ist_leer(&self) -> bool {
        self.zellen.iter().all(|z| *z == Zelle::default())
    }
}

/// Kopf-Größe in Bytes (`MPT_BINARY_STRUCT(ITFileHeader, 192)`).
const KOPF: usize = 0xC0;
/// Sample-Kopf-Größe in Bytes (`MPT_BINARY_STRUCT(ITSample, 80)`).
const SAMPLE_KOPF: usize = 0x50;
/// Obergrenze für Samples (Slices liegen weit darunter; hält die Datei im
/// Rahmen dessen, was IT/libopenmpt klaglos laden).
const MAX_SAMPLES: usize = 255;
/// Obergrenze für Patterns (eine Order je Pattern).
const MAX_PATTERNS: usize = 255;
/// Obergrenze für Pattern-Zeilen (`MAX_PATTERN_ROWS` in libopenmpt).
const MAX_ZEILEN: u16 = 4096;

/// Schreibt eine .it-Datei mit Samples und einem leeren 64-Zeilen-Pattern.
pub fn schreibe(pfad: &str, samples: &[Sample], titel: &str) -> Result<(), String> {
    let daten = bytes(samples, titel)?;
    std::fs::write(pfad, daten).map_err(|e| format!("{e} ({pfad})"))
}

/// Baut die Bytes für Samples + ein leeres 64-Zeilen-Pattern (für Tests,
/// ohne Datei). Später aufgenommene Patterns: [`bytes_mit_patterns`].
pub fn bytes(samples: &[Sample], titel: &str) -> Result<Vec<u8>, String> {
    let leer = Pattern::leer();
    bytes_mit_patterns(samples, std::slice::from_ref(&leer), titel)
}

/// Baut die Bytes mit fertigen Patterns (Orders `0..patterns.len()`).
///
/// Leere 64-Zeilen-Patterns landen als Parapointer `0` in der Datei (so macht
/// es OpenMPT); alle anderen werden gepackt hinter den Sample-Köpfen
/// abgelegt, die PCM-Daten danach. Eine leere Pattern-Liste schreibt eine
/// reine Sample-Datei ohne Patterns/Orders.
pub fn bytes_mit_patterns(
    samples: &[Sample],
    patterns: &[Pattern],
    titel: &str,
) -> Result<Vec<u8>, String> {
    if samples.len() > MAX_SAMPLES {
        return Err(format!(
            "zu viele Samples: {} (höchstens {MAX_SAMPLES})",
            samples.len()
        ));
    }
    if patterns.len() > MAX_PATTERNS {
        return Err(format!(
            "zu viele Patterns: {} (höchstens {MAX_PATTERNS})",
            patterns.len()
        ));
    }
    let ord_num = patterns.len() as u16;
    let smp_num = samples.len() as u16;
    let pat_num = patterns.len() as u16;

    // Absolute Offsets im Aufbau: Orders, Parapointer, Sample-Köpfe, Patterns,
    // PCM. Instrumente gibt es nicht (reine Samples), also keine Ins-Pointer.
    let inspos_off = KOPF + ord_num as usize;
    let smppos_off = inspos_off;
    let patpos_off = smppos_off + 4 * samples.len();
    let sample_koepfe_off = patpos_off + 4 * patterns.len();

    // Pattern-Daten vorbereiten; leere 64-Zeilen-Patterns bleiben Parapointer 0.
    let mut pat_zeiger = vec![0u32; patterns.len()];
    let mut pat_bloecke: Vec<(u16, Vec<u8>)> = Vec::new(); // (Zeilen, gepackt)
    let mut pos = sample_koepfe_off + SAMPLE_KOPF * samples.len();
    for (i, p) in patterns.iter().enumerate() {
        if p.zeilen == 64 && p.ist_leer() {
            continue;
        }
        let gepackt = pattern_packen(p)?;
        pat_zeiger[i] = pos as u32;
        pos += 8 + gepackt.len();
        pat_bloecke.push((p.zeilen, gepackt));
    }
    let sample_daten_off = pos;

    // Sample-Daten-Offsets (PCM liegt am Ende, in Reihenfolge der Samples).
    let mut daten_offset = sample_daten_off;
    let mut pcm_offset = Vec::with_capacity(samples.len());
    for s in samples {
        pcm_offset.push(daten_offset);
        daten_offset += s.daten.len() * 2;
    }

    let mut out = Vec::with_capacity(daten_offset);
    out.extend_from_slice(&kopf(titel, ord_num, smp_num, pat_num));
    for i in 0..ord_num {
        out.push(i as u8); // Order-Liste: 0, 1, 2, …
    }
    for i in 0..samples.len() {
        // Sample-Parapointer zeigen auf die Sample-Köpfe.
        out.extend_from_slice(&((sample_koepfe_off + i * SAMPLE_KOPF) as u32).to_le_bytes());
    }
    for z in &pat_zeiger {
        out.extend_from_slice(&z.to_le_bytes());
    }
    for (i, s) in samples.iter().enumerate() {
        let c5 = if s.c5speed > 0 {
            s.c5speed
        } else if s.rate > 0 {
            s.rate
        } else {
            8363 // libopenmpt-Vorgabe, wenn gar nichts gesetzt ist
        };
        out.extend_from_slice(&sample_kopf(
            &s.name,
            s.daten.len() as u32,
            c5,
            pcm_offset[i] as u32,
        ));
    }
    for (zeilen, gepackt) in &pat_bloecke {
        out.extend_from_slice(&(gepackt.len() as u16).to_le_bytes());
        out.extend_from_slice(&zeilen.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // reserviert
        out.extend_from_slice(gepackt);
    }
    for s in samples {
        for wert in &s.daten {
            out.extend_from_slice(&wert.to_le_bytes());
        }
    }
    debug_assert_eq!(out.len(), daten_offset);
    Ok(out)
}

/// Der 192-Byte-Kopf. Instrumente gibt es nicht (reine Samples).
fn kopf(titel: &str, ord_num: u16, smp_num: u16, pat_num: u16) -> [u8; KOPF] {
    let mut k = [0u8; KOPF];
    k[0x00..0x04].copy_from_slice(b"IMPM");
    let name = kuerzen(titel, 26);
    k[0x04..0x04 + name.len()].copy_from_slice(name.as_bytes());
    k[0x1e] = 4; // PHiligt: Rows je Beat
    k[0x1f] = 16; // PHiligt: Rows je Takt
    k[0x20..0x22].copy_from_slice(&ord_num.to_le_bytes());
    k[0x22..0x24].copy_from_slice(&0u16.to_le_bytes()); // InsNum: reine Samples
    k[0x24..0x26].copy_from_slice(&smp_num.to_le_bytes());
    k[0x26..0x28].copy_from_slice(&pat_num.to_le_bytes());
    k[0x28..0x2a].copy_from_slice(&0x0214u16.to_le_bytes()); // Cwt/v: IT 2.14
    k[0x2a..0x2c].copy_from_slice(&0x0214u16.to_le_bytes()); // Cmwt
    k[0x2c..0x2e].copy_from_slice(&0x0001u16.to_le_bytes()); // Flags: Stereo-Wiedergabe
    // 0x2e Special = 0 (keine eingebettete Nachricht/History/Highlights)
    k[0x30] = 128; // GV: Global Volume
    k[0x31] = 48; // MV: Mix Volume (OpenMPT-Vorgabe)
    k[0x32] = 6; // IS: Initial Speed
    k[0x33] = 125; // IT: Initial Tempo
    k[0x34] = 128; // Sep: Pan-Separation
    k[0x35] = 0; // PWD: Pitch-Wheel-Tiefe
    // 0x36 MsgLgth = 0, 0x38 MsgOffset = 0, 0x3c Reserved = 0
    k[0x40..0x80].fill(32); // ChnlPan: alle Kanäle mittig (0x20)
    k[0x80..0xc0].fill(64); // ChnlVol: volle Lautstärke
    k
}

/// Der 80-Byte-Sample-Kopf (`IMPS`). 16 Bit signed, mono, ohne Schleife.
fn sample_kopf(name: &str, laenge: u32, c5speed: u32, daten_offset: u32) -> [u8; SAMPLE_KOPF] {
    let mut k = [0u8; SAMPLE_KOPF];
    k[0x00..0x04].copy_from_slice(b"IMPS");
    // 0x04 DOS-Dateiname (13) bleibt leer
    k[0x11] = 64; // GvL: Global Volume
    k[0x12] = 0x03; // Flg: Daten vorhanden (0x01) + 16 Bit (0x02)
    k[0x13] = 64; // Vol
    let name = kuerzen(name, 26);
    k[0x14..0x14 + name.len()].copy_from_slice(name.as_bytes());
    k[0x2e] = 0x01; // Cvt: signed
    // 0x2f DfP = 0: kein Pan-Override
    k[0x30..0x34].copy_from_slice(&laenge.to_le_bytes());
    // 0x34 LoopBeg = 0, 0x38 LoopEnd = 0 (keine Schleife)
    k[0x3c..0x40].copy_from_slice(&c5speed.to_le_bytes());
    // 0x40 SusLBeg = 0, 0x44 SusLEnd = 0
    k[0x48..0x4c].copy_from_slice(&daten_offset.to_le_bytes());
    // 0x4c ViS/ViD/ViR/ViT = 0
    k
}

/// Packt ein Pattern wie libopenmpt es liest: je belegter Zelle
/// `(kanal+1) | 0x80`, Maske, Werte; je Zeile ein `0x00` als Zeilenende.
/// (Delta-Optimierung „gleiche Maske wie vorige Zeile" lassen wir weg —
/// die volle Maske ist immer gültig.)
fn pattern_packen(p: &Pattern) -> Result<Vec<u8>, String> {
    if p.zeilen == 0 || p.zeilen > MAX_ZEILEN {
        return Err(format!("Pattern-Zeilen {} ungültig (1…{MAX_ZEILEN})", p.zeilen));
    }
    if p.kanaele > 64 {
        // Der Kanalbyte im Pattern hält nur 7 Bit, und IT hat 64 Kanäle.
        return Err(format!("Pattern: {} Kanäle (höchstens 64)", p.kanaele));
    }
    let erwartet = p.zeilen as usize * p.kanaele as usize;
    if p.zellen.len() != erwartet {
        return Err(format!(
            "Pattern: {} Zellen, erwartet {erwartet} ({} Zeilen × {} Kanäle)",
            p.zellen.len(),
            p.zeilen,
            p.kanaele
        ));
    }
    let mut out = Vec::new();
    for zeile in 0..p.zeilen as usize {
        for kanal in 0..p.kanaele as usize {
            let z = p.zellen[zeile * p.kanaele as usize + kanal];
            if z.parameter.is_some() && z.befehl.is_none() {
                return Err(format!("Pattern-Zelle {zeile}/{kanal}: Parameter ohne Befehl"));
            }
            let mut maske = 0u8;
            if z.note.is_some() {
                maske |= 0x01;
            }
            if z.instrument.is_some() {
                maske |= 0x02;
            }
            if z.volume.is_some() {
                maske |= 0x04;
            }
            if z.befehl.is_some() {
                maske |= 0x08;
            }
            if maske == 0 {
                continue; // leere Zelle
            }
            out.push((kanal as u8 + 1) | 0x80);
            out.push(maske);
            if let Some(n) = z.note {
                out.push(n);
            }
            if let Some(i) = z.instrument {
                out.push(i);
            }
            if let Some(v) = z.volume {
                out.push(v);
            }
            if let Some(c) = z.befehl {
                out.push(c);
                out.push(z.parameter.unwrap_or(0));
            }
        }
        out.push(0x00); // Zeilenende
    }
    Ok(out)
}

/// Auf höchstens `max` Bytes kürzen, ohne ein UTF-8-Zeichen zu zerteilen.
fn kuerzen(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut ende = max;
    while ende > 0 && !text.is_char_boundary(ende) {
        ende -= 1;
    }
    &text[..ende]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_le(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
    }

    fn u32_le(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    fn i16_le(b: &[u8], o: usize) -> i16 {
        i16::from_le_bytes(b[o..o + 2].try_into().unwrap())
    }

    /// Mini-Parser: Offset des Sample-Kopfes `i` (über die Parapointer).
    fn sample_kopf_off(b: &[u8], i: usize) -> usize {
        let ord = u16_le(b, 0x20) as usize;
        let ins = u16_le(b, 0x22) as usize;
        let base = KOPF + ord + 4 * ins;
        u32_le(b, base + 4 * i) as usize
    }

    /// Mini-Parser: Offset des Pattern-Parapointers `i`.
    fn pattern_zeiger_off(b: &[u8]) -> usize {
        let ord = u16_le(b, 0x20) as usize;
        let ins = u16_le(b, 0x22) as usize;
        let smp = u16_le(b, 0x24) as usize;
        KOPF + ord + 4 * ins + 4 * smp
    }

    fn sample(name: &str, n: usize) -> Sample {
        Sample {
            name: name.to_string(),
            daten: (0..n).map(|i| (i % 200) as i16 - 100).collect(),
            rate: 44100,
            c5speed: 0,
        }
    }

    #[test]
    fn kopf_und_anzahl() {
        let s = [sample("Eins", 10), sample("Zwei", 20)];
        let b = bytes(&s, "Mein Titel").unwrap();
        assert_eq!(&b[..4], b"IMPM", "Signatur");
        assert_eq!(&b[4..14], b"Mein Titel", "Songname");
        assert_eq!(b[0x1e], 4, "Highlight Minor (Rows je Beat)");
        assert_eq!(b[0x1f], 16, "Highlight Major (Rows je Takt)");
        assert_eq!(u16_le(&b, 0x20), 1, "OrdNum");
        assert_eq!(u16_le(&b, 0x22), 0, "InsNum: reine Samples");
        assert_eq!(u16_le(&b, 0x24), 2, "SMPNum");
        assert_eq!(u16_le(&b, 0x26), 1, "PatNum");
        assert_eq!(u16_le(&b, 0x28), 0x0214, "Cwt/v: IT 2.14");
        assert_eq!(u16_le(&b, 0x2a), 0x0214, "Cmwt");
        assert_eq!(u16_le(&b, 0x2c) & 0x04, 0, "kein Instrument-Modus");
        assert_eq!(b[0xc0], 0, "Order 0 zeigt auf Pattern 0");
        assert_eq!(u16_le(&b, 0x36), 0, "keine Nachricht");
        assert!(u32_le(&b, KOPF + 1) > 0, "Sample-Parapointer gesetzt");
    }

    #[test]
    fn sample_laenge_und_rate() {
        let daten: Vec<i16> = (0..1000).map(|i| (i % 100) as i16 - 50).collect();
        let s = [Sample {
            name: "A".to_string(),
            daten: daten.clone(),
            rate: 44100,
            c5speed: 0,
        }];
        let b = bytes(&s, "T").unwrap();
        let k = sample_kopf_off(&b, 0);
        assert_eq!(&b[k..k + 4], b"IMPS", "Sample-Signatur");
        assert_eq!(b[k + 0x12], 0x03, "Daten vorhanden + 16 Bit");
        assert_eq!(b[k + 0x2e], 0x01, "signed");
        assert_eq!(u32_le(&b, k + 0x30), 1000, "Länge in Samples");
        assert_eq!(u32_le(&b, k + 0x3c), 44100, "C5Speed = rate");
        assert_eq!(u32_le(&b, k + 0x34), 0, "LoopBeg aus");
        assert_eq!(u32_le(&b, k + 0x38), 0, "LoopEnd aus");
        // PCM liegt am Ende und stimmt Sample für Sample.
        let p = u32_le(&b, k + 0x48) as usize;
        assert_eq!(p + 1000 * 2, b.len(), "PCM direkt hinter den Köpfen");
        for (i, wert) in daten.iter().enumerate() {
            assert_eq!(i16_le(&b, p + i * 2), *wert, "Sample {i}");
        }
        // c5speed hat Vorrang vor rate.
        let s = [Sample {
            name: "B".to_string(),
            daten: vec![0; 4],
            rate: 44100,
            c5speed: 8363,
        }];
        let b = bytes(&s, "T").unwrap();
        let k = sample_kopf_off(&b, 0);
        assert_eq!(u32_le(&b, k + 0x3c), 8363, "c5speed überschreibt rate");
    }

    #[test]
    fn leeres_pattern() {
        let s = [sample("A", 100)];
        let b = bytes(&s, "T").unwrap();
        assert_eq!(u16_le(&b, 0x26), 1, "ein Pattern");
        let pz = pattern_zeiger_off(&b);
        assert_eq!(u32_le(&b, pz), 0, "leeres Pattern = Parapointer 0");
        // Ohne Pattern-Daten beginnt das PCM direkt nach den Sample-Köpfen.
        let k = sample_kopf_off(&b, 0);
        assert_eq!(u32_le(&b, k + 0x48) as usize, k + SAMPLE_KOPF);
        assert_eq!(b.len(), k + SAMPLE_KOPF + 200, "nur Kopf + PCM");
    }

    #[test]
    fn namen_werden_gekuerzt() {
        let lang = "A".repeat(30);
        let s = [Sample {
            name: lang.clone(),
            daten: vec![0, 0],
            rate: 8000,
            c5speed: 0,
        }];
        let b = bytes(&s, &lang).unwrap();
        let k = sample_kopf_off(&b, 0);
        assert_eq!(&b[k + 0x14..k + 0x2e], &[b'A'; 26][..], "Samplename: 26");
        assert_eq!(&b[4..0x1e], &[b'A'; 26][..], "Songname: 26");
        // Ein Umlaut wird nicht zerteilt (20 × „ä" = 40 Bytes → 13 Zeichen).
        let umlaut = "ä".repeat(20);
        let s = [Sample {
            name: umlaut.clone(),
            daten: vec![0, 0],
            rate: 8000,
            c5speed: 0,
        }];
        let b = bytes(&s, &umlaut).unwrap();
        let k = sample_kopf_off(&b, 0);
        let name = &b[k + 0x14..k + 0x2e];
        assert_eq!(name, "ä".repeat(13).as_bytes(), "13 Umlaute, nicht zerteilt");
    }

    #[test]
    fn leere_sample_liste_ist_ok() {
        let b = bytes(&[], "Leer").unwrap();
        assert_eq!(u16_le(&b, 0x20), 1, "eine Order");
        assert_eq!(u16_le(&b, 0x24), 0, "keine Samples");
        assert_eq!(u16_le(&b, 0x26), 1, "ein leeres Pattern");
        // Kopf + Order + Pattern-Parapointer (kein Sample, kein PCM).
        assert_eq!(b.len(), KOPF + 1 + 4);
    }

    #[test]
    fn gefuelltes_pattern_wird_gepackt() {
        let p = Pattern {
            zeilen: 2,
            kanaele: 2,
            zellen: vec![
                Zelle {
                    note: Some(59), // C-5
                    instrument: Some(1),
                    ..Zelle::default()
                },
                Zelle::default(),
                Zelle::default(),
                Zelle {
                    note: Some(254), // Note-Cut
                    ..Zelle::default()
                },
            ],
        };
        let b = bytes_mit_patterns(&[sample("A", 8)], &[p], "T").unwrap();
        assert_eq!(u16_le(&b, 0x26), 1, "ein Pattern");
        let pz = pattern_zeiger_off(&b);
        let pp = u32_le(&b, pz) as usize;
        assert_ne!(pp, 0, "gefülltes Pattern hat echte Daten");
        assert_eq!(u16_le(&b, pp + 2), 2, "zwei Zeilen");
        let len = u16_le(&b, pp) as usize;
        let daten = &b[pp + 8..pp + 8 + len];
        // Zeile 0: Kanal 0 mit Note+Instrument, dann Zeilenende.
        assert_eq!(daten, &[0x81, 0x03, 59, 1, 0x00, 0x82, 0x01, 254, 0x00]);
        // Leere Liste: reine Sample-Datei ohne Patterns/Orders.
        let b = bytes_mit_patterns(&[sample("A", 8)], &[], "T").unwrap();
        assert_eq!(u16_le(&b, 0x20), 0, "keine Orders");
        assert_eq!(u16_le(&b, 0x26), 0, "keine Patterns");
    }
}
