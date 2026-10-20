//! Hörbeispiel für die Laub-Kette: Blattabstände → Genre-Rhythmus → WAV.
//!
//! Aufruf:
//!
//! ```text
//! cargo run --manifest-path core/Cargo.toml --example laub_beispiel
//! cargo run --manifest-path core/Cargo.toml --example laub_beispiel -- \
//!     --abstaende 1.2,1.8,0.9,1.0
//! ```
//!
//! Ohne `--abstaende` läuft ein dokumentierter „Beispielweg“ mit 20 Werten
//! (0,5 m … 2,5 m, Median 1,0 m). Für jedes Genre (Techno, Groove, Breakbeat,
//! Drum and Bass), jedes Raster (96/128/192/256) und jede Feinheit (16/32/64)
//! entsteht eine WAV-Datei in `/tmp/opencode/laub_beispiele/`.
//!
//! # Was gerendert wird
//!
//! Der Kern liefert die Schrittnummern **eines Takts** ([`schritte_aus_abstaenden_bpm`])
//! und bildet sie mit [`raster_auf_zeilen`] auf Pattern-Zeilen ab. Ein Takt
//! belegt bei Feinheit 16/32/64 genau 16/32/64 Zeilen; damit das 128-Zeilen-
//! Pattern nicht nach einer Zeile verstummt, wird der Takt über das
//! `start`-Argument der Kernfunktion bis Zeile 128 wiederholt. Gespielt werden
//! zwei Durchläufe des 128-Zeilen-Patterns.
//!
//! Die Farbe ist hier nur eine Hörhilfe: `farbe = index % 4`, damit alle vier
//! Tonhöhen vorkommen. Der Kern kennt keine Bildfarben, wenn nur Abstände
//! vorliegen.

use jamcore::laub::{
    genre, raster_auf_zeilen, schritte_aus_abstaenden_bpm, Abstandsmodus, FARBE_BRAUN, FARBE_GELB,
    FARBE_GRUEN, FARBE_ROT,
};
use std::fs;
use std::path::Path;

/// Ausgabeordner für alle WAV-Dateien.
const AUSGABE: &str = "/tmp/opencode/laub_beispiele";

/// Raster aus [`jamcore::laub::rasterwahl`].
const RASTER: [u16; 4] = [96, 128, 192, 256];

/// Feinheit: Zeilen je Takt (16 = 16tel-Zeilen, 32 = 32tel, 64 = 64tel).
const FEINHEITEN: [u16; 3] = [16, 32, 64];

/// Länge des Patterns in Zeilen.
const ZEILEN: u16 = 128;

/// Zwei Durchläufe des Patterns, danach Ende.
const DURCHLAEUFE: usize = 2;

/// Tempo für den absoluten Modus und die Zeilendauer.
const BPM: f64 = 120.0;

/// Abtastrate der WAV-Dateien.
const SAMPLE_RATE: u32 = 44_100;

/// Länge einer perkussiven Note in Sekunden.
const NOTENLAENGE_S: f64 = 0.120;

/// Grundamplitude einer Note (vor der Summe und der Pegelkorrektur).
const AMPLITUDE: f32 = 0.30;

/// Genres als (Anzeigename für [`genre`], Dateinamen-Kürzel).
const GENRES: [(&str, &str); 4] = [
    ("Techno", "techno"),
    ("Groove", "groove"),
    ("Breakbeat", "breakbeat"),
    ("Drum and Bass", "dnb"),
];

/// Beispielweg: 20 Blattabstände in Metern, in der Reihenfolge, in der sie
/// beim Gehen auftreten. Kleinster Abstand 0,5 m, größter 2,5 m, Median 1,0 m
/// — ein gemütlicher Weg mit ein paar Lücken und einem dichten Stück.
const BEISPIELWEG_M: [f32; 20] = [
    1.0, 1.4, 0.7, 1.0, 2.1, 0.8, 1.1, 0.5, 1.3, 1.0, 1.6, 0.9, 1.2, 2.5, 1.0, 0.6, 0.9, 1.8, 2.3,
    0.8,
];

fn main() {
    if let Err(fehler) = run() {
        eprintln!("Fehler: {fehler}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut abstaende = BEISPIELWEG_M.to_vec();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--abstaende" => {
                let liste = args.next().ok_or_else(|| {
                    "--abstaende braucht eine kommagetrennte Liste, z. B. 1.2,1.8".to_string()
                })?;
                abstaende = parse_abstaende(&liste)?;
            }
            "-h" | "--hilfe" => {
                hilfe();
                return Ok(());
            }
            sonst => {
                return Err(format!(
                    "unbekanntes Argument {sonst:?} (erlaubt: --abstaende, --hilfe)"
                ));
            }
        }
    }

    let ausgabe = Path::new(AUSGABE);
    fs::create_dir_all(ausgabe).map_err(|e| format!("Ausgabeordner {}: {e}", ausgabe.display()))?;

    println!(
        "Abstände ({}): {}",
        abstaende.len(),
        abstaende
            .iter()
            .map(|m| format!("{m:.2}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("Ausgabe: {}", ausgabe.display());

    let mut dateien = 0usize;
    for (name, kuerzel) in GENRES {
        let genre = genre(name).ok_or_else(|| format!("Genre {name:?} fehlt im Kern"))?;
        for raster in RASTER {
            let schritte = schritte_aus_abstaenden_bpm(
                &abstaende,
                raster,
                Abstandsmodus::gehen(),
                &genre,
                BPM as f32,
            );
            let takt = takt_mit_farben(&schritte);

            for feinheit in FEINHEITEN {
                // Der Takt belegt feinheit Zeilen; bis Zeile 128 wiederholen.
                let mut noten = Vec::new();
                for start in (0..ZEILEN).step_by(feinheit as usize) {
                    noten.extend(raster_auf_zeilen(&takt, raster, feinheit, ZEILEN, start));
                }

                let puffer = rendern(&noten, feinheit);
                let dateiname = format!("{kuerzel}_{raster}_{feinheit}.wav");
                let pfad = ausgabe.join(&dateiname);
                schreibe_wav(&pfad, &puffer)?;
                dateien += 1;

                let dauer = puffer.len() as f64 / SAMPLE_RATE as f64;
                println!(
                    "{dateiname}: Genre={} Raster={raster} Feinheit={feinheit} \
                     Schritte im Takt={} Zeilen={ZEILEN} Noten={} Dauer={dauer:.2}s",
                    genre.name,
                    schritte.len(),
                    noten.len()
                );
            }
        }
    }

    println!("Fertig: {dateien} WAV-Dateien in {}", ausgabe.display());
    Ok(())
}

fn hilfe() {
    println!("laub_beispiel – Laub-Rhythmen als WAV hören");
    println!();
    println!("Aufruf:");
    println!(
        "  cargo run --manifest-path core/Cargo.toml --example laub_beispiel \
         [-- --abstaende 1.2,1.8,...]"
    );
    println!();
    println!("  --abstaende   Kommagetrennte Blattabstände in Metern (Vorgabe: Beispielweg)");
    println!("  -h, --hilfe   Diese Hilfe");
    println!();
    println!("Ausgabe: {AUSGABE}");
}

/// Liest eine kommagetrennte Liste von Metern; kaputte Werte sind ein Fehler.
fn parse_abstaende(text: &str) -> Result<Vec<f32>, String> {
    let mut werte = Vec::new();
    for teil in text.split(',') {
        let teil = teil.trim();
        if teil.is_empty() {
            return Err(format!("leerer Wert in der Abstandsliste {text:?}"));
        }
        let wert: f32 = teil
            .parse()
            .map_err(|_| format!("{teil:?} ist keine Zahl in Metern"))?;
        if !wert.is_finite() || wert < 0.0 {
            return Err(format!("Abstand {teil:?} muss eine Zahl ≥ 0 sein"));
        }
        werte.push(wert);
    }
    if werte.is_empty() {
        return Err("die Abstandsliste ist leer".to_string());
    }
    Ok(werte)
}

/// Schrittnummern zu (Schritt, Farbe); die Farbe ist `index % 4` und damit
/// eine reine Hörhilfe (alle vier Tonhöhen kommen vor).
fn takt_mit_farben(schritte: &[u32]) -> Vec<(u32, u8)> {
    schritte
        .iter()
        .enumerate()
        .map(|(index, schritt)| (*schritt, (index % 4) as u8))
        .collect()
}

/// Sinusfrequenz je Farbklasse.
fn frequenz(farbe: u8) -> f32 {
    match farbe {
        FARBE_ROT => 261.6,
        FARBE_GELB => 311.1,
        FARBE_BRAUN => 392.0,
        FARBE_GRUEN => 523.3,
        _ => 261.6,
    }
}

/// Rendert zwei Durchläufe des Patterns als 44,1-kHz-Mono-Samples.
fn rendern(noten: &[(u16, u8)], feinheit: u16) -> Vec<f32> {
    // Zeilendauer: 60/BPM ist eine Viertel; feinheit/4 Zeilen sind eine Viertel.
    let zeilendauer = 60.0 / BPM / (feinheit as f64 / 4.0);
    let gesamt =
        (DURCHLAEUFE as f64 * ZEILEN as f64 * zeilendauer * SAMPLE_RATE as f64).round() as usize;
    let mut puffer = vec![0.0f32; gesamt];
    let noten_samples = (NOTENLAENGE_S * SAMPLE_RATE as f64).round() as usize;

    for durchlauf in 0..DURCHLAEUFE {
        for &(zeile, farbe) in noten {
            let zeit = (durchlauf * ZEILEN as usize + zeile as usize) as f64 * zeilendauer;
            let start = (zeit * SAMPLE_RATE as f64).round() as usize;
            let freq = frequenz(farbe) as f64;
            for i in 0..noten_samples {
                let index = start + i;
                if index >= puffer.len() {
                    break;
                }
                let t = i as f64 / SAMPLE_RATE as f64;
                // Exponentielle Hüllkurve (~120 ms), kurzer Attack gegen Klicks.
                let huelle = (-t / 0.030).exp();
                let attack = (t / 0.001).min(1.0);
                let wert = (std::f64::consts::TAU * freq * t).sin();
                puffer[index] += (AMPLITUDE as f64 * huelle * attack * wert) as f32;
            }
        }
    }

    // Nur absenken, wenn die Summe übersteuert; leise Patterns bleiben leise.
    let spitze = puffer.iter().fold(0.0f32, |max, wert| max.max(wert.abs()));
    if spitze > 0.89 {
        let faktor = 0.89 / spitze;
        for wert in &mut puffer {
            *wert *= faktor;
        }
    }
    puffer
}

/// Schreibt 44,1 kHz, 16 Bit, mono als RIFF/WAVE (Header von Hand).
fn schreibe_wav(pfad: &Path, samples: &[f32]) -> Result<(), String> {
    let daten_bytes = samples.len() * 2;
    let mut bytes = Vec::with_capacity(44 + daten_bytes);

    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36u32 + daten_bytes as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // Größe des fmt-Chunks
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // Byte/s
    bytes.extend_from_slice(&2u16.to_le_bytes()); // Blockausrichtung
    bytes.extend_from_slice(&16u16.to_le_bytes()); // Bits je Sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(daten_bytes as u32).to_le_bytes());

    for &sample in samples {
        let wert = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        bytes.extend_from_slice(&wert.to_le_bytes());
    }

    fs::write(pfad, bytes).map_err(|e| format!("{}: {e}", pfad.display()))
}
