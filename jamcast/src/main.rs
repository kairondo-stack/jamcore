//! jamcast – „Hörverständnis fürs Pattern“.
//!
//! Ein kleines CLI: es hört in eine Audiodatei hinein (Sprache, Geräusche,
//! Musik), findet Anschläge und Tonwechsel, schätzt die Tonhöhe und gießt
//! beides in ein Impulse-Tracker-Pattern. Ausgabe sind eine `.it`-Datei
//! (im Player/Renoise ladbar), eine CSV-Zeile je Note und eine Zusammenfassung
//! auf der Konsole.
//!
//! # Warum dieser Weg
//!
//! Die Rechenarbeit steckt im Kern, damit Desktop und Android dieselbe Logik
//! sehen:
//!
//! - [`jamcore::audio::laden`] liest wav/mp3/ogg/m4a/aiff über symphonia.
//! - [`jamcore::slice::schnitte`] findet die Onsets über den Energie-Fluss
//!   (dieselbe Transienten-Erkennung wie der Slicer in der App).
//! - [`jamcore::tonfolge::tonhoehen`]/[`jamcore::tonfolge::midi`] schätzen die
//!   Tonhöhe am Anschlag per Autokorrelation.
//! - [`jamcore::laub::schritte_aus_abstaenden_bpm`] rundet die Abstände auf
//!   das Vokabular eines Genres und faltet sie in einen Takt;
//!   [`jamcore::laub::raster_auf_zeilen`] legt die Schritte auf Pattern-Zeilen.
//! - [`jamcore::itwrite`] schreibt die `.it` samt Song-Message.
//!
//! # Zeit statt Weg
//!
//! `laub` denkt in Metern auf einem Weg. Für Audio ist der „Weg“ die Zeit:
//! wir übergeben die Abstände in Sekunden als Meter und setzen die
//! Schrittlänge auf fast null. Damit gilt `Meter = Sekunde`, und die
//! Umrechnung `Sekunden × BPM × Raster / 240` liefert genau die Schritte im
//! Takt. Den Genre-Rundungs- und Faltmechanismus von `laub` behalten wir
//! dadurch unverändert.
//!
//! # Anschläge und Tonwechsel
//!
//! `slice::schnitte` begrenzt auf 64 Anschläge und findet nur Transienten:
//! ein legato gespielter Tonwechsel (C → E) hat keinen neuen Energie-Anstieg.
//! Deshalb liefert die Tonhöhen-Spur zusätzlich die Tonwechsel; beide Listen
//! werden zu höchstens 96 Ereignissen vereinigt. Für ein 4-Takt-Pattern ist
//! das reichlich, und ein 30-Minuten-Podcast bleibt in einem Durchgang
//! bezahlbar.

use std::path::Path;

use jamcore::audio;
use jamcore::grid::note_name;
use jamcore::itwrite::{self, Meta, Pattern, Sample, Zelle};
use jamcore::klang::{puffer_welle, Welle};
use jamcore::laub::{self, Abstandsmodus, Genre};
use jamcore::slice::{self, Schnittmodus};
use jamcore::tonfolge;

/// Vorgabe-Tempo, wie bei den übrigen Werkzeugen.
const BPM_VORGABE: f32 = 120.0;
/// Vorgabe-Raster (Schritte je Takt); 128 ist das gebräuchliche Laub-Raster.
const RASTER_VORGABE: u16 = 128;
/// Vorgabe-Feinheit: 32 Zeilen je Takt = 32tel-Raster.
const FEINHEIT_VORGABE: u16 = 32;
/// Vorgabe-Instrument (0 = Sinus).
const INSTRUMENT_VORGABE: usize = 0;
/// So viele Takte bekommt das Pattern. Vier Takte sind ein hörbarer Loop und
/// passen mit allen Feinheiten in die IT-Grenze von 4096 Zeilen.
const TAKTE: u16 = 4;
/// Höchstens so viele Anschläge (Grenze von `slice::schnitte`).
const MAX_ANSCHLAEGE: usize = 64;
/// Höchstens so viele Ereignisse (Anschläge + Tonwechsel) im Takt. Ein
/// 4-Takt-Pattern hat bei Raster 256 höchstens 256 Schritte; 96 Ereignisse
/// reichen für ein dichtes Muster und halten die Analyse bezahlbar.
const MAX_EREIGNISSE: usize = 96;
/// Unter dieser RMS gilt ein Analysefenster als Stille (wie in `slice.rs`).
const STILLE_RMS: f32 = 1e-3;
/// Ein Tonabschnitt muss so lange klingen, sonst ist er ein Analyse-Zucken
/// (Autokorrelation) und kein Tonwechsel.
const MIN_TON_S: f32 = 0.08;
/// Abtastrate der erzeugten Instrument-Samples.
const SAMPLE_RATE: u32 = 44_100;
/// Länge der erzeugten Instrument-Samples: lang genug für einen hörbaren Ton,
/// kurz genug, dass schnelle Noten einander nicht zumüllen.
const SAMPLE_MS: u32 = 220;
/// Grundfrequenz der Samples: C-4 (MIDI 60). Mit `c5speed = rate` spielt die
/// Note 60 damit genau diese Frequenz; alle anderen Noten transponieren.
const C4_HZ: f32 = 261.625_6;

/// Eine der vier eingebauten Klangfarben. Die Reihenfolge ist die Nummer für
/// `--instrument`, damit Hilfe und Datei dieselbe Zählung zeigen.
#[derive(Clone, Copy)]
struct Stimme {
    /// Name im Sample-Kopf (ASCII, damit jeder Tracker ihn sauber zeigt).
    name: &'static str,
    /// Name in der Konsole (darf Umlaute haben).
    anzeige: &'static str,
    welle: Welle,
}

const STIMMEN: [Stimme; 4] = [
    Stimme {
        name: "Sinus",
        anzeige: "Sinus",
        welle: Welle::Sinus,
    },
    Stimme {
        name: "Dreieck",
        anzeige: "Dreieck",
        welle: Welle::Dreieck,
    },
    Stimme {
        name: "Saegezahn",
        anzeige: "Sägezahn",
        welle: Welle::Saegezahn,
    },
    Stimme {
        name: "Rechteck",
        anzeige: "Rechteck",
        welle: Welle::Rechteck,
    },
];

/// Geparste Kommandozeile.
#[derive(Debug, Clone)]
struct Optionen {
    eingabe: String,
    bpm: f32,
    raster: u16,
    feinheit: u16,
    genre: Genre,
    instrument: usize,
    ausgabe: Option<String>,
}

/// Ergebnis des Argument-Parsens: Hilfe oder ein Auftrag.
enum Anfrage {
    Hilfe,
    Start(Optionen),
}

/// Ein rhythmischer Treffer im Takt: Schrittnummer, Note und der Onset, von
/// dem die Tonhöhe kommt.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Treffer {
    schritt: u32,
    note: i32,
    onset: f32,
}

/// Ein gehaltener Tonabschnitt aus der Tonhöhen-Spur.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tonabschnitt {
    start: f32,
    ende: f32,
    note: i32,
}

/// Eine fertige Pattern-Zeile (für `.it` und CSV gemeinsam).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Zeile {
    /// Takt im Pattern, 1-basiert.
    takt: u16,
    /// Zeile im Pattern, 0-basiert (wie im Tracker).
    zeile: u16,
    /// Schritt im Takt-Raster.
    schritt: u32,
    note: i32,
    onset: f32,
}

fn main() {
    match argumente(std::env::args().skip(1)) {
        Ok(Anfrage::Hilfe) => {
            println!("{}", hilfe());
        }
        Ok(Anfrage::Start(optionen)) => {
            if let Err(fehler) = lauf(&optionen) {
                eprintln!("jamcast: {fehler}");
                std::process::exit(1);
            }
        }
        Err(fehler) => {
            eprintln!("jamcast: {fehler}");
            eprintln!("Hilfe: jamcast --hilfe");
            std::process::exit(2);
        }
    }
}

/// Die Hilfe (`--hilfe`) im Wortlaut.
fn hilfe() -> String {
    format!(
        "\
jamcast 0.1.0 – Hörverständnis fürs Pattern
Übersetzt Sprache, Geräusche oder Musik in ein Impulse-Tracker-Pattern.

AUFRUF
  jamcast <audio> [Optionen]

ARGUMENTE
  <audio>               Audiodatei: wav, mp3, ogg, flac, m4a, aac, aiff

OPTIONEN
  --bpm <zahl>         Tempo in Schlägen je Minute, 32…255 (Vorgabe {BPM_VORGABE})
  --raster <zahl>      Schritte je Takt, 1…256 (Vorgabe {RASTER_VORGABE})
  --feinheit <16|32|64> Zeilen je Takt im Pattern (Vorgabe {FEINHEIT_VORGABE})
  --genre <name>       techno | groove | breakbeat | dnb (Vorgabe techno)
  --instrument <0…3>   Klangfarbe: 0 Sinus, 1 Dreieck, 2 Sägezahn,
                       3 Rechteck (Vorgabe {INSTRUMENT_VORGABE})
  --ausgabe <datei.it> Zielpfad (Vorgabe: <audio>.it im aktuellen Ordner)
  --hilfe              diese Hilfe

AUSGABE
  <ausgabe>.it         {TAKTE} Takte, Quelle/Datum/Dauer/Onsets in der Song-Message
  <ausgabe>.csv        eine Zeile je Pattern-Note:
                       takt,zeile,schritt,note,note_name,onset_s

BEISPIEL
  jamcast stimme.m4a --bpm 96 --raster 128 --feinheit 32 \\
      --genre groove --instrument 1 --ausgabe stimme.it"
    )
}

/// Kommandozeile parsen (bewusst ohne clap: keine neue Abhängigkeit).
///
/// Unterstützt `--name wert` und `--name=wert`; `-h` ist die Kurzform von
/// `--hilfe`. Alles andere, was mit `-` beginnt, ist ein Fehler — so fällt ein
/// Tippfehler sofort auf, statt als Dateiname durchzugehen.
fn argumente<I: IntoIterator<Item = String>>(args: I) -> Result<Anfrage, String> {
    let mut eingabe: Option<String> = None;
    let mut bpm = BPM_VORGABE;
    let mut raster = RASTER_VORGABE;
    let mut feinheit = FEINHEIT_VORGABE;
    let mut genre = laub::genre("techno").expect("Techno ist im Kern definiert");
    let mut instrument = INSTRUMENT_VORGABE;
    let mut ausgabe: Option<String> = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (name, inline) = if arg.starts_with("--") {
            match arg.split_once('=') {
                Some((name, wert)) => (name.to_string(), Some(wert.to_string())),
                None => (arg.clone(), None),
            }
        } else {
            (arg.clone(), None)
        };
        match name.as_str() {
            "--hilfe" | "-h" => return Ok(Anfrage::Hilfe),
            "--bpm" => {
                let wert = wert_holen(inline, &mut args, "--bpm")?;
                let zahl: f32 = wert
                    .parse()
                    .map_err(|_| format!("--bpm: „{wert}“ ist keine Zahl"))?;
                if !zahl.is_finite() || !(32.0..=255.0).contains(&zahl) {
                    return Err(format!("--bpm: {wert} liegt nicht zwischen 32 und 255"));
                }
                bpm = zahl;
            }
            "--raster" => {
                let wert = wert_holen(inline, &mut args, "--raster")?;
                let zahl: u16 = wert
                    .parse()
                    .map_err(|_| format!("--raster: „{wert}“ ist keine Zahl"))?;
                if !(1..=256).contains(&zahl) {
                    return Err(format!("--raster: {zahl} liegt nicht zwischen 1 und 256"));
                }
                raster = zahl;
            }
            "--feinheit" => {
                let wert = wert_holen(inline, &mut args, "--feinheit")?;
                let zahl: u16 = wert
                    .parse()
                    .map_err(|_| format!("--feinheit: „{wert}“ ist keine Zahl"))?;
                if !matches!(zahl, 16 | 32 | 64) {
                    return Err(format!("--feinheit: {zahl} ist nicht 16, 32 oder 64"));
                }
                feinheit = zahl;
            }
            "--genre" => {
                let wert = wert_holen(inline, &mut args, "--genre")?;
                genre = laub::genre(&wert).ok_or_else(|| {
                    format!("--genre: „{wert}“ unbekannt (techno, groove, breakbeat, dnb)")
                })?;
            }
            "--instrument" => {
                let wert = wert_holen(inline, &mut args, "--instrument")?;
                let zahl: usize = wert
                    .parse()
                    .map_err(|_| format!("--instrument: „{wert}“ ist keine Zahl"))?;
                if zahl >= STIMMEN.len() {
                    return Err(format!(
                        "--instrument: {zahl} ist nicht 0…{}",
                        STIMMEN.len() - 1
                    ));
                }
                instrument = zahl;
            }
            "--ausgabe" => {
                let wert = wert_holen(inline, &mut args, "--ausgabe")?;
                if wert.trim().is_empty() {
                    return Err("--ausgabe: leerer Pfad".to_string());
                }
                ausgabe = Some(wert);
            }
            _ if name.starts_with('-') => {
                return Err(format!("unbekannte Option „{name}“"));
            }
            _ => {
                if eingabe.is_some() {
                    return Err(format!("nur eine Audiodatei, nicht auch noch „{arg}“"));
                }
                eingabe = Some(arg);
            }
        }
    }

    let eingabe = eingabe.ok_or("Audiodatei fehlt (z. B. jamcast aufnahme.wav)")?;
    Ok(Anfrage::Start(Optionen {
        eingabe,
        bpm,
        raster,
        feinheit,
        genre,
        instrument,
        ausgabe,
    }))
}

/// Wert einer Option: entweder hinter `=` oder als nächstes Argument.
fn wert_holen<I: Iterator<Item = String>>(
    inline: Option<String>,
    args: &mut I,
    name: &str,
) -> Result<String, String> {
    match inline {
        Some(wert) if !wert.is_empty() => Ok(wert),
        Some(_) => Err(format!("{name}: leerer Wert")),
        None => args.next().ok_or_else(|| format!("{name}: Wert fehlt")),
    }
}

/// Der eigentliche Lauf: laden, hören, schreiben, zusammenfassen.
fn lauf(optionen: &Optionen) -> Result<(), String> {
    let audio = audio::laden(&optionen.eingabe)?;
    if audio.rate == 0 {
        return Err(format!("{}: Abtastrate fehlt", optionen.eingabe));
    }
    let dauer = audio.proben.len() as f32 / audio.rate as f32;
    let quelle = dateiname(&optionen.eingabe);

    let anschlaege = anschlaege_aus_proben(&audio.proben, audio.rate);
    // Die Tonhöhen-Spur einmal über die ganze Datei: sie liefert sowohl die
    // Note je Ereignis als auch die Tonwechsel (legato), die der
    // Transienten-Detektor nicht sieht.
    let spur = tonfolge::tonhoehen(&audio.proben, audio.rate);
    let abschnitte = tonabschnitte(&spur);
    let onsets = vereinige_ereignisse(&anschlaege, &abschnitte);
    let (noten, erkannt) = noten_zu_onsets(&abschnitte, &onsets);
    let treffer = takt_ereignisse(
        &onsets,
        &noten,
        optionen.bpm,
        optionen.raster,
        &optionen.genre,
    );

    let zeilen = optionen.feinheit.saturating_mul(TAKTE).max(1);
    let zeilen_liste = zeilen_aus_treffern(&treffer, optionen.raster, optionen.feinheit, TAKTE);
    let pattern = pattern_aus_zeilen(&zeilen_liste, zeilen, optionen.instrument);
    let samples = stimmen_samples();

    let meta = meta_fuer(
        &quelle,
        audio.rate,
        dauer,
        anschlaege.len(),
        abschnitte.len(),
        onsets.len(),
        erkannt,
        optionen,
        zeilen,
        zeilen_liste.len(),
    );
    let titel = dateistamm(&optionen.eingabe);
    let mut daten = itwrite::bytes_mit_patterns_und_meta(&samples, &titel, &[pattern], &meta)?;
    setze_tempo(&mut daten, optionen.bpm, optionen.feinheit)?;

    let ausgabe = optionen
        .ausgabe
        .clone()
        .unwrap_or_else(|| format!("{titel}.it"));
    std::fs::write(&ausgabe, &daten).map_err(|e| format!("{e} ({ausgabe})"))?;

    let csv_pfad = mit_endung(&ausgabe, "csv");
    let csv = csv_text(&zeilen_liste);
    std::fs::write(&csv_pfad, csv).map_err(|e| format!("{e} ({csv_pfad})"))?;

    // Zusammenfassung. Bewusst mehrzeilig und mit den Zahlen, die auch in der
    // Song-Message stehen — so ist die Konsole der Prüfbericht zum Export.
    let schritte: Vec<String> = treffer.iter().map(|t| t.schritt.to_string()).collect();
    println!("jamcast: {} -> {}", optionen.eingabe, ausgabe);
    println!(
        "  Quelle    {quelle} ({} Hz, {dauer:.2} s, {} Proben)",
        audio.rate,
        audio.proben.len()
    );
    let ereignis_wort = if onsets.len() == 1 {
        "Ereignis"
    } else {
        "Ereignisse"
    };
    println!(
        "  Onsets    {} Anschläge, {} Tonwechsel -> {} {ereignis_wort} (Tonhöhe erkannt: {erkannt})",
        anschlaege.len(),
        abschnitte.len(),
        onsets.len()
    );
    println!(
        "  Takt      {} BPM, Raster {}, Feinheit {}, Genre {}",
        optionen.bpm, optionen.raster, optionen.feinheit, optionen.genre.name
    );
    println!("  Schritte  {}", schritte.join(", "));
    println!(
        "  Pattern   {TAKTE} Takte, {zeilen} Zeilen, {} Noten, Instrument {} ({})",
        zeilen_liste.len(),
        optionen.instrument,
        STIMMEN[optionen.instrument].anzeige
    );
    println!("  .it       {ausgabe} ({} Bytes)", daten.len());
    println!("  CSV       {csv_pfad} ({} Noten)", zeilen_liste.len());
    if onsets.is_empty() {
        println!("  Hinweis   keine Onsets gefunden – das Pattern bleibt leer.");
    }
    for zeile in &meta.zeilen {
        println!("  Message   {zeile}");
    }
    Ok(())
}

/// Anschlags-Zeiten (Sekunden) aus dem Energie-Fluss.
///
/// `slice::schnitte` liefert Slice-Anfänge genau an den Onsets. Ein einzelner
/// Slice über die ganze Datei ist aber auch der Rückfall bei „kein Onset“;
/// deshalb prüfen wir am Anfang auf Energie: startet die Datei laut, ist der
/// Anfang selbst ein Onset (z. B. ein Ton ohne Vorlauf), sonst nicht.
fn anschlaege_aus_proben(proben: &[f32], rate: u32) -> Vec<f32> {
    if proben.is_empty() || rate == 0 {
        return Vec::new();
    }
    let schnitte = slice::schnitte(proben, rate, Schnittmodus::Transienten, MAX_ANSCHLAEGE, 0.5);
    let mut zeiten: Vec<f32> = Vec::new();
    for schnitt in &schnitte {
        let ist_onset = schnitt.von > 0
            || schnitte.len() > 1
            // Einzelner Rückfall-Slice ab 0: nur bei lautem Anfang ein Onset.
            || start_ist_laut(proben, rate);
        if ist_onset {
            zeiten.push(schnitt.von as f32 / rate as f32);
        }
    }
    // Die Slice-Anfänge sind schon aufsteigend; ein Dedup fängt nur die
    // theoretisch gleiche Zeit aus zwei Analysefenstern ab.
    zeiten.sort_by(f32::total_cmp);
    zeiten.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    zeiten
}

/// RMS der ersten 10 ms über der Stille-Schwelle? (10 ms = Analysefenster
/// von `slice.rs`.)
fn start_ist_laut(proben: &[f32], rate: u32) -> bool {
    let fenster = (rate as usize / 100).max(1);
    let bis = fenster.min(proben.len());
    if bis == 0 {
        return false;
    }
    let summe: f32 = proben[..bis].iter().map(|s| s * s).sum();
    let rms = (summe / bis as f32).sqrt();
    rms.is_finite() && rms > STILLE_RMS
}

/// Gehaltene Tonabschnitte aus der Tonhöhen-Spur. Ein Wechsel der MIDI-Note
/// beginnt einen neuen Abschnitt; kurze Zuckungen (unter [`MIN_TON_S`]) sind
/// Analyse-Rauschen und fallen weg. Eine Pause von mindestens 150 ms beendet
/// den Abschnitt, damit ein erneuter Anschlag nach Stille wieder als eigener
/// Ton zählt.
fn tonabschnitte(spur: &[(f32, f32)]) -> Vec<Tonabschnitt> {
    let fenster_s = if spur.len() >= 2 {
        let differenz = spur[1].0 - spur[0].0;
        if differenz.is_finite() && differenz > 0.0 {
            differenz
        } else {
            tonfolge::FENSTER_S
        }
    } else {
        tonfolge::FENSTER_S
    };
    let mut roh: Vec<Tonabschnitt> = Vec::new();
    let mut letzte_note: Option<i32> = None;
    let mut stille_seit: Option<f32> = None;
    let mut letzte_ton_zeit: f32 = 0.0;

    for &(zeit, frequenz) in spur {
        if !zeit.is_finite() || !frequenz.is_finite() {
            continue;
        }
        let note = tonfolge::midi(frequenz);
        if note <= 0 {
            let beginn = *stille_seit.get_or_insert(zeit);
            if zeit - beginn >= 0.15 {
                // Lange Pause: der nächste Ton ist ein neuer Anschlag, auch
                // wenn er dieselbe Note hat.
                letzte_note = None;
            }
            continue;
        }
        stille_seit = None;
        if let Some(letzte) = letzte_note {
            if letzte == note {
                letzte_ton_zeit = zeit;
                continue;
            }
        }
        if let Some(abschnitt) = roh.last_mut() {
            abschnitt.ende = zeit;
        }
        roh.push(Tonabschnitt {
            start: zeit,
            ende: zeit + fenster_s,
            note,
        });
        letzte_note = Some(note);
        letzte_ton_zeit = zeit;
    }

    // Den letzten Abschnitt bis zum letzten klingenden Fenster strecken; sonst
    // wäre ein gehaltener Schlusston nur ein einziges Analysefenster lang.
    if let Some(abschnitt) = roh.last_mut() {
        abschnitt.ende = abschnitt.ende.max(letzte_ton_zeit + fenster_s);
        if abschnitt.ende <= abschnitt.start {
            abschnitt.ende = abschnitt.start + fenster_s;
        }
    }
    roh.retain(|abschnitt| abschnitt.ende - abschnitt.start >= MIN_TON_S);
    roh
}

/// Anschläge und Tonwechsel zu einer Ereignisliste vereinigen.
///
/// Warum beides: Ein Schlagzeug oder eine Silbe erzeugt einen Transienten, ein
/// legato gespielter Tonwechsel (C → E ohne neuen Anschlag) nur einen
/// Tonwechsel. Die Transienten haben Vorrang, wenn beide auf dieselbe Zeit
/// fallen. Bei mehr als [`MAX_EREIGNISSE`] bleiben alle Anschläge; die
/// Tonwechsel werden gleichmäßig ausgedünnt, damit der ganze Ausschnitt
/// vertreten bleibt.
fn vereinige_ereignisse(anschlaege: &[f32], abschnitte: &[Tonabschnitt]) -> Vec<f32> {
    let mut anschlaege: Vec<f32> = anschlaege
        .iter()
        .copied()
        .filter(|zeit| zeit.is_finite() && *zeit >= 0.0)
        .collect();
    anschlaege.sort_by(f32::total_cmp);
    anschlaege.dedup_by(|a, b| (*a - *b).abs() < 0.03);

    let mut wechsel: Vec<f32> = abschnitte
        .iter()
        .map(|abschnitt| abschnitt.start)
        .filter(|zeit| zeit.is_finite() && *zeit >= 0.0)
        .filter(|zeit| {
            !anschlaege
                .iter()
                .any(|anschlag| (anschlag - zeit).abs() < 0.03)
        })
        .collect();
    wechsel.sort_by(f32::total_cmp);
    wechsel.dedup_by(|a, b| (*a - *b).abs() < 0.03);

    let platz = MAX_EREIGNISSE.saturating_sub(anschlaege.len());
    if wechsel.len() > platz {
        if platz == 0 {
            wechsel.clear();
        } else {
            // Gleichmäßig ausdünnen: `wechsel[i * len / platz]` behält den
            // ersten und letzten und verteilt den Rest über die Zeit.
            let gesamt = wechsel.len();
            wechsel = (0..platz).map(|i| wechsel[i * gesamt / platz]).collect();
            wechsel.dedup_by(|a, b| (*a - *b).abs() < 0.03);
        }
    }

    let mut alle = anschlaege;
    alle.extend(wechsel);
    alle.sort_by(f32::total_cmp);
    alle.dedup_by(|a, b| (*a - *b).abs() < 0.03);
    alle
}

/// Note je Ereignis aus den Tonabschnitten, dazu die Anzahl der Ereignisse mit
/// wirklich erkanntem Ton. Ereignisse außerhalb eines Abschnitts (Schlagzeug,
/// Zischen) erben die letzte Note, damit das Pattern nicht bei jedem Klick auf
/// C springt; ohne jede erkannte Note bleibt C-4 (60) als neutraler Grundton.
fn noten_zu_onsets(abschnitte: &[Tonabschnitt], onsets: &[f32]) -> (Vec<i32>, usize) {
    let mut noten = Vec::with_capacity(onsets.len());
    let mut letzte: Option<i32> = None;
    let mut erkannt = 0usize;
    for &onset in onsets {
        // Toleranz an den Rändern: die Spur läuft in 50-ms-Schritten, ein
        // Anschlag liegt selten exakt auf einem Fensterbeginn.
        let note = abschnitte
            .iter()
            .rev()
            .find(|abschnitt| {
                // Toleranz an beiden Rändern, aber rückwärts suchen: ein
                // Ereignis kurz vor einem Tonwechsel gehört schon zum neuen
                // Ton (der Anschlag läuft dem Analysefenster oft ein paar
                // Millisekunden voraus).
                onset + 0.02 >= abschnitt.start && onset < abschnitt.ende + 0.02
            })
            .map(|abschnitt| abschnitt.note)
            .or_else(|| {
                // Kein Abschnitt trifft: den nächstgelegenen Anfang nehmen,
                // wenn er nicht zu weit weg ist.
                abschnitte
                    .iter()
                    .min_by(|a, b| (a.start - onset).abs().total_cmp(&(b.start - onset).abs()))
                    .filter(|abschnitt| (abschnitt.start - onset).abs() <= 0.25)
                    .map(|abschnitt| abschnitt.note)
            });
        let note = match note {
            Some(note) => {
                erkannt += 1;
                letzte = Some(note);
                note
            }
            None => letzte.unwrap_or(60),
        };
        noten.push(note.clamp(1, 119));
    }
    (noten, erkannt)
}

/// Die reine Onset→Schritt-Abbildung: Onset-Zeiten und Noten werden zu
/// Treffern im Takt-Raster.
///
/// Die Abstände zwischen den Onsets gehen als „Meter“ in
/// [`laub::schritte_aus_abstaenden_bpm`]; mit Gehtempo 1 m/s und einer
/// Schrittlänge nahe null ist ein Meter genau eine Sekunde (siehe Modulkopf).
/// Das Ergebnis ist die Menge der Schritte im Takt, sortiert und ohne
/// Doppelte. Ein einzelner Onset ergibt Schritt 0, ohne Onsets bleibt die
/// Liste leer.
fn takt_ereignisse(
    onsets: &[f32],
    noten: &[i32],
    bpm: f32,
    raster: u16,
    genre: &Genre,
) -> Vec<Treffer> {
    if onsets.is_empty() || raster == 0 {
        return Vec::new();
    }
    let bpm = if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        BPM_VORGABE
    };

    // Zeit und Note zusammenhalten, damit die Zuordnung nach dem Sortieren
    // nicht verrutscht.
    let mut paare: Vec<(f32, i32)> = onsets
        .iter()
        .enumerate()
        .filter(|(_, zeit)| zeit.is_finite() && **zeit >= 0.0)
        .map(|(i, zeit)| (*zeit, noten.get(i).copied().unwrap_or(60).clamp(1, 119)))
        .collect();
    paare.sort_by(|a, b| a.0.total_cmp(&b.0));
    paare.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4);
    if paare.is_empty() {
        return Vec::new();
    }

    let abstaende: Vec<f32> = paare
        .windows(2)
        .map(|fenster| (fenster[1].0 - fenster[0].0).max(0.0))
        .collect();
    let schritte = if abstaende.is_empty() {
        vec![0]
    } else {
        laub::schritte_aus_abstaenden_bpm(
            &abstaende,
            raster,
            Abstandsmodus::Absolut {
                geschwindigkeit_m_s: 1.0,
                // Fast null: ein Audio-Onset darf beliebig dicht liegen; die
                // Mindest-Schrittlänge des Gehens gilt hier nicht.
                schrittlaenge_m: 0.000_001,
            },
            genre,
            bpm,
        )
    };
    if schritte.is_empty() {
        return Vec::new();
    }

    // Ein Takt dauert bei 4/4 genau 240/BPM Sekunden. Die Tonhöhe eines
    // Schritts kommt vom zeitlich nächsten Onset im (umlaufenden) Takt.
    let takt_s = 240.0 / bpm;
    let erster = paare[0].0;
    let mut treffer = Vec::with_capacity(schritte.len());
    for schritt in schritte {
        let ziel = schritt as f32 * takt_s / raster as f32;
        let (index, _) = paare
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                zyklus_abstand(a.0 - erster, ziel, takt_s).total_cmp(&zyklus_abstand(
                    b.0 - erster,
                    ziel,
                    takt_s,
                ))
            })
            .expect("paare ist nicht leer");
        treffer.push(Treffer {
            schritt,
            note: paare[index].1,
            onset: paare[index].0,
        });
    }
    treffer
}

/// Abstand zweier Zeitpunkte im umlaufenden Takt (0 … Takt/2).
fn zyklus_abstand(rel: f32, ziel: f32, takt_s: f32) -> f32 {
    if !takt_s.is_finite() || takt_s <= 0.0 {
        return 0.0;
    }
    let rel = rel.rem_euclid(takt_s);
    let abstand = (rel - ziel).abs();
    abstand.min(takt_s - abstand)
}

/// Treffer auf Pattern-Zeilen legen: je Takt einmal
/// [`laub::raster_auf_zeilen`], damit die Abbildung exakt dieselbe ist wie im
/// Laub-Tab der App. Kollidieren zwei Schritte auf einer Zeile, gewinnt der
/// erste (so macht es `raster_auf_zeilen`).
fn zeilen_aus_treffern(treffer: &[Treffer], raster: u16, feinheit: u16, takte: u16) -> Vec<Zeile> {
    let zeilen = feinheit.saturating_mul(takte).max(1);
    let feinheit = feinheit.max(1);
    if treffer.is_empty() || raster == 0 {
        return Vec::new();
    }
    // Die Treffer-Nummer dient `raster_auf_zeilen` als Instrument-Tag; so
    // findet der Rückweg die Note wieder. Höchstens 256 Schritte, also passt
    // die Nummer in ein u8.
    let tag: Vec<(u32, u8)> = treffer
        .iter()
        .enumerate()
        .map(|(i, t)| (t.schritt, i as u8))
        .collect();
    let mut liste = Vec::new();
    for (takt, start) in (0..zeilen).step_by(feinheit as usize).enumerate() {
        for (zeile, index) in laub::raster_auf_zeilen(&tag, raster, feinheit, zeilen, start) {
            let treffer = &treffer[index as usize];
            liste.push(Zeile {
                takt: takt as u16 + 1,
                zeile,
                schritt: treffer.schritt,
                note: treffer.note,
                onset: treffer.onset,
            });
        }
    }
    liste
}

/// Aus den Pattern-Zeilen ein einkanaliges IT-Pattern bauen.
fn pattern_aus_zeilen(liste: &[Zeile], zeilen: u16, instrument: usize) -> Pattern {
    let zeilen = zeilen.max(1);
    let mut zellen = vec![Zelle::default(); zeilen as usize];
    for eintrag in liste {
        if let Some(zelle) = zellen.get_mut(eintrag.zeile as usize) {
            *zelle = Zelle {
                note: Some(eintrag.note.clamp(1, 119) as u8),
                instrument: Some((instrument + 1) as u8),
                ..Zelle::default()
            };
        }
    }
    Pattern {
        zeilen,
        kanaele: 1,
        zellen,
    }
}

/// CSV mit einer Zeile je Pattern-Note. `note_name` folgt der
/// Renoise-Schreibweise des Projekts (`grid::note_name`).
fn csv_text(liste: &[Zeile]) -> String {
    let mut out = String::from("takt,zeile,schritt,note,note_name,onset_s\n");
    for eintrag in liste {
        out.push_str(&format!(
            "{},{},{},{},{},{:.3}\n",
            eintrag.takt,
            eintrag.zeile,
            eintrag.schritt,
            eintrag.note,
            note_name(eintrag.note),
            eintrag.onset
        ));
    }
    out
}

/// Die vier Instrument-Klangfarben als IT-Samples. Alle vier landen in der
/// Datei, damit man im Tracker die Instrument-Spalte umstellen kann; die
/// Noten zeigen auf die mit `--instrument` gewählte.
fn stimmen_samples() -> Vec<Sample> {
    STIMMEN
        .iter()
        .map(|stimme| Sample {
            name: stimme.name.to_string(),
            daten: puffer_welle(C4_HZ, SAMPLE_RATE, SAMPLE_MS, stimme.welle),
            rate: SAMPLE_RATE,
            c5speed: SAMPLE_RATE,
        })
        .collect()
}

/// Speed (Ticks je Zeile) und Tempo (BPM) für den IT-Kopf, so dass eine
/// Pattern-Zeile genau eine `1/feinheit`-Taktzählzeit dauert.
///
/// Herleitung: eine IT-Zeile dauert `speed · 2,5 / tempo` Sekunden, ein Takt
/// bei `bpm` dauert `240 / bpm`. Gleichsetzen ergibt
/// `speed / tempo = 96 / (feinheit · bpm)`. Für 16 und 32 Zeilen je Takt passt
/// `speed = 96/feinheit` (6 bzw. 3) und `tempo = bpm`. Bei 64 Zeilen wäre
/// `speed = 1,5`; deshalb dort `speed = 3, tempo = 2·bpm` (bis 127 BPM) oder
/// `speed = 1, tempo ≈ 2·bpm/3` (darüber), was in die IT-Grenzen 1…255 bzw.
/// 32…255 passt.
fn timing(bpm: f32, feinheit: u16) -> (u8, u8) {
    let bpm = if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        BPM_VORGABE
    };
    let (speed, tempo): (f32, f32) = match feinheit {
        16 => (6.0, bpm),
        32 => (3.0, bpm),
        // 64 (und jeder andere Wert): mit speed 3 und doppeltem Tempo bis
        // 127 BPM, darüber mit speed 1 und zwei Dritteln des Tempos.
        _ => {
            if bpm <= 127.5 {
                (3.0, bpm * 2.0)
            } else {
                (1.0, bpm * 2.0 / 3.0)
            }
        }
    };
    (
        speed.round().clamp(1.0, 255.0) as u8,
        tempo.round().clamp(32.0, 255.0) as u8,
    )
}

/// Tempo und Takt-Hervorhebung im IT-Kopf setzen.
///
/// Warum nachträglich: `jamcore::itwrite` schreibt bewusst feste Vorgaben
/// (Speed 6, Tempo 125, 4/16-Hervorhebung), weil die App beim Export kein
/// Tempo kennt. jamcast kennt es — die beiden Kopfbytes sind Teil des
/// IT-Formats, also patchen wir sie direkt nach dem Schreiben; der Kern bleibt
/// unverändert.
fn setze_tempo(daten: &mut [u8], bpm: f32, feinheit: u16) -> Result<(), String> {
    if daten.len() < 0x40 {
        return Err("IT-Kopf zu kurz".to_string());
    }
    let (speed, tempo) = timing(bpm, feinheit);
    daten[0x1e] = (feinheit / 4).clamp(1, 255) as u8; // Zeilen je Viertel
    daten[0x1f] = feinheit.clamp(1, 255) as u8; // Zeilen je Takt
    daten[0x32] = speed; // IS: Initial Speed
    daten[0x33] = tempo; // IT: Initial Tempo
    Ok(())
}

/// Song-Message mit den Pflichtangaben Quelle, Datum, Dauer und Onset-Anzahl.
#[allow(clippy::too_many_arguments)]
fn meta_fuer(
    quelle: &str,
    rate: u32,
    dauer: f32,
    anschlaege: usize,
    tonwechsel: usize,
    onsets: usize,
    erkannt: usize,
    optionen: &Optionen,
    zeilen: u16,
    noten: usize,
) -> Meta {
    Meta {
        titel: "jamcast".to_string(),
        zeilen: vec![
            format!("Quelle: {quelle}"),
            format!("Datum: {}", itwrite::datum_heute()),
            format!("Dauer: {dauer:.2} s ({rate} Hz)"),
            format!(
                "Onsets: {onsets} (Anschläge: {anschlaege}, Tonwechsel: {tonwechsel}, Tonhöhe erkannt: {erkannt})"
            ),
            format!(
                "Takt: {} BPM, Raster {}, Feinheit {}, Genre {}",
                optionen.bpm, optionen.raster, optionen.feinheit, optionen.genre.name
            ),
            format!(
                "Pattern: {TAKTE} Takte, {zeilen} Zeilen, {noten} Noten, Instrument {} ({})",
                optionen.instrument, STIMMEN[optionen.instrument].anzeige
            ),
        ],
    }
}

/// Dateiname ohne Ordner (für Quelle und Song-Message).
fn dateiname(pfad: &str) -> String {
    Path::new(pfad)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| pfad.to_string())
}

/// Dateiname ohne Endung (für den IT-Titel).
fn dateistamm(pfad: &str) -> String {
    Path::new(pfad)
        .file_stem()
        .map(|stamm| stamm.to_string_lossy().into_owned())
        .filter(|stamm| !stamm.is_empty())
        .unwrap_or_else(|| "jamcast".to_string())
}

/// Endung ersetzen (`out.it` → `out.csv`) oder anhängen, wenn keine da ist.
fn mit_endung(pfad: &str, endung: &str) -> String {
    let pfad = Path::new(pfad);
    match pfad.extension() {
        Some(_) => pfad.with_extension(endung).display().to_string(),
        None => format!("{}.{endung}", pfad.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genre(name: &str) -> Genre {
        laub::genre(name).expect("Genre im Kern")
    }

    /// Klicks in Stille (wie die Tests in `slice.rs`): je Eintrag ein
    /// 5-ms-Ausschlag.
    fn klicks(rate: u32, dauer_s: f32, positionen: &[f32]) -> Vec<f32> {
        let n = (rate as f32 * dauer_s) as usize;
        let mut proben = vec![0.0f32; n];
        let laenge = (rate as f32 * 0.005) as usize;
        for &t in positionen {
            let start = (t * rate as f32) as usize;
            for k in 0..laenge {
                if start + k < n {
                    proben[start + k] = 0.9;
                }
            }
        }
        proben
    }

    #[test]
    fn vier_aufeinanderfolgende_onsets_werden_ein_takt() {
        // 120 BPM, Raster 128: eine halbe Sekunde ist genau eine Viertel,
        // also 32 Schritte. Vier Viertel ergeben 0, 32, 64, 96.
        let onsets = [0.0, 0.5, 1.0, 1.5];
        let noten = [69, 72, 76, 79];
        let treffer = takt_ereignisse(&onsets, &noten, 120.0, 128, &genre("techno"));
        assert_eq!(
            treffer.iter().map(|t| t.schritt).collect::<Vec<_>>(),
            vec![0, 32, 64, 96],
            "4-on-the-floor bei 120 BPM"
        );
        assert_eq!(
            treffer.iter().map(|t| t.note).collect::<Vec<_>>(),
            vec![69, 72, 76, 79],
            "jede Viertel behält ihre Tonhöhe"
        );
        assert_eq!(treffer[2].onset, 1.0, "Onset bleibt zur Note zugeordnet");
    }

    #[test]
    fn genre_bestimmt_die_aufloesung() {
        // 16tel bei 120 BPM: 0,25 s = 16 Schritte im 128er-Raster.
        let onsets = [0.0, 0.25, 0.5, 0.75, 1.0];
        let noten = [60, 62, 64, 65, 67];
        let techno = takt_ereignisse(&onsets, &noten, 120.0, 128, &genre("techno"));
        assert_eq!(
            techno.iter().map(|t| t.schritt).collect::<Vec<_>>(),
            vec![0, 32, 64, 96],
            "Techno rundet 16tel auf Viertel"
        );
        let groove = takt_ereignisse(&onsets, &noten, 120.0, 128, &genre("groove"));
        assert_eq!(
            groove.iter().map(|t| t.schritt).collect::<Vec<_>>(),
            vec![0, 16, 32, 48, 64],
            "Groove behält das 16tel (12/16/18 im 96er-Referenzraster)"
        );
    }

    #[test]
    fn ein_onset_wird_schritt_null() {
        let treffer = takt_ereignisse(&[1.23], &[69], 120.0, 128, &genre("techno"));
        assert_eq!(treffer.len(), 1);
        assert_eq!(treffer[0].schritt, 0, "der erste Anschlag ist der Puls");
        assert_eq!(treffer[0].note, 69);
        assert_eq!(treffer[0].onset, 1.23);
    }

    #[test]
    fn ohne_onsets_kein_takt() {
        assert!(takt_ereignisse(&[], &[], 120.0, 128, &genre("techno")).is_empty());
        // Kaputte Zeiten werden einzeln verworfen.
        assert!(
            takt_ereignisse(&[f32::NAN, -1.0], &[60, 60], 120.0, 128, &genre("techno")).is_empty()
        );
    }

    #[test]
    fn abstaende_ueber_einem_takt_falten_sich() {
        // 2,5 s bei 120 BPM = 2,5 Takte = 160 Schritte; 160 % 128 = 32.
        let treffer = takt_ereignisse(&[0.0, 2.5], &[60, 67], 120.0, 128, &genre("techno"));
        assert_eq!(
            treffer.iter().map(|t| t.schritt).collect::<Vec<_>>(),
            vec![0, 32],
            "Schritte werden in den Takt gefaltet"
        );
    }

    #[test]
    fn tonhoehe_kommt_vom_naechsten_onset_im_takt() {
        // 0,4 s bei 120 BPM sind 25,6 Schritte; Techno rundet auf eine Viertel
        // (32). Der Schritt liegt dann bei 0,5 s — der Onset bei 0,4 s ist ihm
        // näher als der bei 0 s, also trägt er die Note.
        let treffer = takt_ereignisse(&[0.0, 0.4], &[60, 72], 120.0, 128, &genre("techno"));
        assert_eq!(
            treffer.iter().map(|t| t.schritt).collect::<Vec<_>>(),
            vec![0, 32]
        );
        assert_eq!(treffer[1].note, 72, "Tonhöhe vom nächsten Onset");
        assert_eq!(treffer[1].onset, 0.4);
    }

    #[test]
    fn zyklus_abstand_ist_umlaufend() {
        // Ein Takt von 2 s: 1,9 s liegt nur 0,1 s vor 0 s.
        assert!((zyklus_abstand(1.9, 0.0, 2.0) - 0.1).abs() < 1e-6);
        assert!((zyklus_abstand(0.2, 0.0, 2.0) - 0.2).abs() < 1e-6);
        assert!((zyklus_abstand(1.0, 0.0, 2.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zeilen_folgen_dem_raster() {
        // Schritt 32 bei Raster 128 und Feinheit 32 = Zeile 8 im Takt.
        let treffer = vec![
            Treffer {
                schritt: 0,
                note: 60,
                onset: 0.0,
            },
            Treffer {
                schritt: 32,
                note: 64,
                onset: 0.5,
            },
            Treffer {
                schritt: 64,
                note: 67,
                onset: 1.0,
            },
            Treffer {
                schritt: 96,
                note: 72,
                onset: 1.5,
            },
        ];
        let liste = zeilen_aus_treffern(&treffer, 128, 32, 2);
        assert_eq!(liste.len(), 8, "vier Schritte je Takt, zwei Takte");
        assert_eq!(
            liste.iter().map(|z| z.zeile).collect::<Vec<_>>(),
            vec![0, 8, 16, 24, 32, 40, 48, 56],
            "Zeilen im 32er-Pattern"
        );
        assert_eq!(
            liste.iter().map(|z| z.takt).collect::<Vec<_>>(),
            vec![1, 1, 1, 1, 2, 2, 2, 2],
            "Taktnummer ist 1-basiert"
        );
        assert_eq!(liste[5].note, 64, "Note des zweiten Takts wiederholt sich");
        // Feinheit 16: derselbe Schritt landet auf Zeile 4.
        let fein = zeilen_aus_treffern(&treffer[..2], 128, 16, 1);
        assert_eq!(fein.iter().map(|z| z.zeile).collect::<Vec<_>>(), vec![0, 4]);
        // Kollision: bei Feinheit 16 und Raster 256 liegen 32 und 40 auf
        // derselben Zeile; der erste Treffer bleibt.
        let dicht = vec![
            Treffer {
                schritt: 32,
                note: 60,
                onset: 0.5,
            },
            Treffer {
                schritt: 40,
                note: 72,
                onset: 0.6,
            },
        ];
        let eine = zeilen_aus_treffern(&dicht, 256, 16, 1);
        assert_eq!(eine.len(), 1, "eine Zeile, ein Treffer");
        assert_eq!(eine[0].note, 60, "der erste Treffer gewinnt");
    }

    #[test]
    fn pattern_hat_noten_und_instrument() {
        let liste = vec![
            Zeile {
                takt: 1,
                zeile: 0,
                schritt: 0,
                note: 69,
                onset: 0.0,
            },
            Zeile {
                takt: 1,
                zeile: 8,
                schritt: 32,
                note: 72,
                onset: 0.5,
            },
        ];
        let pattern = pattern_aus_zeilen(&liste, 32, 2);
        assert_eq!(pattern.zeilen, 32);
        assert_eq!(pattern.kanaele, 1);
        assert_eq!(pattern.zellen.len(), 32);
        assert_eq!(
            pattern.zellen[0],
            Zelle {
                note: Some(69),
                instrument: Some(3), // --instrument 2 → IT-Sample 3
                ..Zelle::default()
            }
        );
        assert_eq!(pattern.zellen[8].note, Some(72));
        assert_eq!(pattern.zellen[1], Zelle::default(), "dazwischen leer");
        assert!(!pattern.ist_leer());
    }

    #[test]
    fn csv_hat_kopf_und_eine_zeile_je_note() {
        let liste = vec![
            Zeile {
                takt: 1,
                zeile: 0,
                schritt: 0,
                note: 69,
                onset: 0.0,
            },
            Zeile {
                takt: 2,
                zeile: 8,
                schritt: 32,
                note: 72,
                onset: 0.5,
            },
        ];
        let csv = csv_text(&liste);
        let zeilen: Vec<&str> = csv.lines().collect();
        assert_eq!(zeilen[0], "takt,zeile,schritt,note,note_name,onset_s");
        assert_eq!(zeilen[1], "1,0,0,69,A-4,0.000");
        assert_eq!(zeilen[2], "2,8,32,72,C-5,0.500");
        assert_eq!(zeilen.len(), 3);
    }

    #[test]
    fn onsets_aus_klicks() {
        let rate = 16_000;
        let proben = klicks(rate, 2.0, &[0.0, 0.5, 1.0, 1.5]);
        let onsets = anschlaege_aus_proben(&proben, rate);
        assert_eq!(onsets.len(), 4, "vier Klicks: {onsets:?}");
        for (onset, erwartet) in onsets.iter().zip([0.0, 0.5, 1.0, 1.5]) {
            assert!(
                (onset - erwartet).abs() < 0.02,
                "Onset {onset} statt {erwartet}"
            );
        }
    }

    #[test]
    fn stille_hat_keine_onsets() {
        let rate = 16_000;
        let proben = vec![0.0f32; rate as usize];
        assert!(anschlaege_aus_proben(&proben, rate).is_empty());
        assert!(anschlaege_aus_proben(&[], rate).is_empty());
        assert!(anschlaege_aus_proben(&proben, 0).is_empty());
    }

    #[test]
    fn einzelner_onset_am_anfang_und_spaeter() {
        let rate = 16_000;
        // Anfang laut: der Rückfall-Slice ab 0 ist ein echter Onset.
        let am_anfang = klicks(rate, 0.5, &[0.0]);
        assert_eq!(anschlaege_aus_proben(&am_anfang, rate), vec![0.0]);
        // Erst Stille, dann ein Klick: einen Slice ab 0,5 s.
        let spaeter = klicks(rate, 1.0, &[0.5]);
        let onsets = anschlaege_aus_proben(&spaeter, rate);
        assert_eq!(onsets.len(), 1);
        assert!(
            (onsets[0] - 0.5).abs() < 0.02,
            "Onset bei 0,5 s: {onsets:?}"
        );
    }

    #[test]
    fn tonabschnitte_finden_legato_wechsel() {
        // C (0–0,2), E (0,2–0,5), G (0,5–0,8), dann Stille.
        let spur = vec![
            (0.0, 261.63),
            (0.05, 261.63),
            (0.2, 329.63),
            (0.25, 329.63),
            (0.45, 329.63),
            (0.5, 392.0),
            (0.55, 392.0),
            (0.75, 392.0),
            (0.8, 0.0),
            (1.0, 0.0),
        ];
        let abschnitte = tonabschnitte(&spur);
        assert_eq!(abschnitte.len(), 3, "{abschnitte:?}");
        assert_eq!(abschnitte[0].note, 60);
        assert_eq!(abschnitte[1].note, 64);
        assert_eq!(abschnitte[2].note, 67);
        assert_eq!((abschnitte[1].start, abschnitte[1].ende), (0.2, 0.5));
        // Sehr kurze Zuckungen (unter 80 ms) fallen weg.
        let zucken = vec![(0.0, 261.63), (0.05, 329.63), (0.1, 392.0), (0.3, 392.0)];
        let abschnitte = tonabschnitte(&zucken);
        assert_eq!(abschnitte.len(), 1, "{abschnitte:?}");
        assert_eq!(abschnitte[0].note, 67);
    }

    #[test]
    fn ereignisse_vereinigen_anschlaege_und_tonwechsel() {
        let abschnitte = vec![
            Tonabschnitt {
                start: 0.0,
                ende: 0.2,
                note: 60,
            },
            Tonabschnitt {
                start: 0.2,
                ende: 0.5,
                note: 64,
            },
            Tonabschnitt {
                start: 0.7,
                ende: 1.0,
                note: 67,
            },
        ];
        // Der Anschlag bei 0,7 fällt mit dem Tonwechsel zusammen und bleibt
        // einmal stehen; 0,2 und 0,4 kommen als Tonwechsel dazu.
        let onsets = vereinige_ereignisse(&[0.0, 0.7], &abschnitte);
        assert_eq!(onsets, vec![0.0, 0.2, 0.7]);
        // Ein früher Tonwechsel vor dem ersten Anschlag bleibt erhalten.
        let abschnitte = vec![Tonabschnitt {
            start: 0.4,
            ende: 0.9,
            note: 64,
        }];
        assert_eq!(vereinige_ereignisse(&[1.0], &abschnitte), vec![0.4, 1.0]);
    }

    #[test]
    fn ereignisliste_bleibt_begrenzt() {
        let anschlaege: Vec<f32> = (0..MAX_ANSCHLAEGE).map(|i| i as f32 * 0.5).collect();
        let abschnitte: Vec<Tonabschnitt> = (0..200)
            .map(|i| Tonabschnitt {
                start: i as f32 * 0.1 + 0.02,
                ende: i as f32 * 0.1 + 0.08,
                note: 60,
            })
            .collect();
        let onsets = vereinige_ereignisse(&anschlaege, &abschnitte);
        assert_eq!(onsets.len(), MAX_EREIGNISSE, "Obergrenze greift");
        assert!(
            (0..MAX_ANSCHLAEGE).all(|i| onsets.contains(&(i as f32 * 0.5))),
            "alle Anschläge bleiben"
        );
        assert!(
            onsets.last().copied().unwrap_or(0.0) > 10.0,
            "die Tonwechsel verteilen sich über die ganze Zeit"
        );
    }

    #[test]
    fn noten_kommen_aus_dem_tonabschnitt() {
        let abschnitte = vec![
            Tonabschnitt {
                start: 0.0,
                ende: 0.45,
                note: 60,
            },
            Tonabschnitt {
                start: 0.5,
                ende: 1.0,
                note: 67,
            },
        ];
        let (noten, erkannt) = noten_zu_onsets(&abschnitte, &[0.0, 0.48, 0.9, 2.0]);
        assert_eq!(noten, vec![60, 67, 67, 67], "2,0 s erbt die letzte Note");
        assert_eq!(erkannt, 3, "der Onset außerhalb zählt nicht als erkannt");
        // Ohne jede Tonhöhe bleibt C-4.
        let (noten, erkannt) = noten_zu_onsets(&[], &[0.0, 1.0]);
        assert_eq!(noten, vec![60, 60]);
        assert_eq!(erkannt, 0);
    }

    #[test]
    fn samples_sind_vier_hoerbare_stimmen() {
        let samples = stimmen_samples();
        assert_eq!(samples.len(), 4);
        let erwartet = (SAMPLE_RATE as u64 * SAMPLE_MS as u64 / 1000) as usize;
        for sample in &samples {
            assert_eq!(sample.daten.len(), erwartet, "{}", sample.name);
            assert_eq!(sample.rate, SAMPLE_RATE);
            assert_eq!(sample.c5speed, SAMPLE_RATE);
            let spitze = sample.daten.iter().map(|s| s.abs()).max().unwrap_or(0);
            assert!(spitze > 1000, "{} klingt: Spitze {spitze}", sample.name);
        }
        assert_eq!(samples[0].name, "Sinus");
        assert_eq!(samples[3].name, "Rechteck");
    }

    #[test]
    fn tempo_im_kopf_passt_zum_bpm() {
        let mut daten = itwrite::bytes(&[], "T").unwrap();
        setze_tempo(&mut daten, 120.0, 32).unwrap();
        assert_eq!(daten[0x32], 3, "Speed");
        assert_eq!(daten[0x33], 120, "Tempo");
        assert_eq!(daten[0x1e], 8, "Hervorhebung: Zeilen je Viertel");
        assert_eq!(daten[0x1f], 32, "Hervorhebung: Zeilen je Takt");
        // 128 Zeilen × 3 Ticks × 2,5 / 120 = 8 s = vier Takte bei 120 BPM.
        let dauer = 128.0 * f32::from(daten[0x32]) * 2.5 / f32::from(daten[0x33]);
        assert!((dauer - 8.0).abs() < 1e-4, "Dauer {dauer}");

        // Feinheit 64: 256 Zeilen müssen ebenfalls vier Takte ergeben.
        setze_tempo(&mut daten, 120.0, 64).unwrap();
        assert_eq!((daten[0x32], daten[0x33]), (3, 240));
        let dauer = 256.0 * f32::from(daten[0x32]) * 2.5 / f32::from(daten[0x33]);
        assert!((dauer - 8.0).abs() < 1e-4, "Dauer {dauer}");

        // Schnelles Tempo: zwei Drittel-Tempo mit Speed 1, kleiner Rundungsrest.
        assert_eq!(timing(200.0, 64), (1, 133));
        setze_tempo(&mut daten, 200.0, 64).unwrap();
        let dauer = 256.0 * f32::from(daten[0x32]) * 2.5 / f32::from(daten[0x33]);
        assert!((dauer - 4.8).abs() < 0.02, "Dauer {dauer}");
        // Die anderen Feinheiten bleiben exakt.
        assert_eq!(timing(96.0, 16), (6, 96));
        assert_eq!(timing(96.0, 32), (3, 96));
    }

    #[test]
    fn meta_enthaelt_die_pflichtangaben() {
        let optionen = Optionen {
            eingabe: "stimme.wav".to_string(),
            bpm: 96.0,
            raster: 128,
            feinheit: 32,
            genre: genre("groove"),
            instrument: 1,
            ausgabe: None,
        };
        let meta = meta_fuer("stimme.wav", 44_100, 12.345, 7, 5, 9, 6, &optionen, 128, 9);
        let text = meta.zeilen.join("\n");
        assert!(text.contains("Quelle: stimme.wav"), "{text}");
        assert!(text.contains("Datum: "), "{text}");
        assert!(text.contains("Dauer: 12.35 s"), "{text}");
        assert!(text.contains("Onsets: 9"), "{text}");
        assert!(text.contains("Anschläge: 7"), "{text}");
        assert!(text.contains("Tonwechsel: 5"), "{text}");
        assert!(text.contains("Genre Groove"), "{text}");
        assert_eq!(meta.titel, "jamcast");
    }

    #[test]
    fn it_bytes_haben_kopf_order_und_meta() {
        let samples = stimmen_samples();
        let pattern = pattern_aus_zeilen(
            &[Zeile {
                takt: 1,
                zeile: 0,
                schritt: 0,
                note: 69,
                onset: 0.0,
            }],
            64,
            0,
        );
        let meta = Meta {
            titel: "jamcast".to_string(),
            zeilen: vec!["Quelle: test.wav".to_string(), "Onsets: 1".to_string()],
        };
        let bytes =
            itwrite::bytes_mit_patterns_und_meta(&samples, "test", &[pattern], &meta).unwrap();
        assert_eq!(&bytes[..4], b"IMPM", "Signatur");
        assert_eq!(&bytes[4..8], b"test", "Titel");
        let u16_le = |o: usize| u16::from_le_bytes(bytes[o..o + 2].try_into().unwrap());
        assert_eq!(u16_le(0x20), 1, "eine Order");
        assert_eq!(u16_le(0x24), 4, "vier Samples");
        assert_eq!(u16_le(0x26), 1, "ein Pattern");
        assert_eq!(bytes[0xc0], 0, "Order 0 → Pattern 0");
        assert_eq!(u16_le(0x2e) & 0x01, 1, "Song-Message vorhanden");
        assert!(u16_le(0x36) > 0, "MsgLgth > 0");
    }

    #[test]
    fn argumente_vorgaben_und_hilfe() {
        let Anfrage::Start(optionen) = argumente(["aufnahme.wav".to_string()]).unwrap() else {
            panic!("kein Start");
        };
        assert_eq!(optionen.eingabe, "aufnahme.wav");
        assert_eq!(optionen.bpm, 120.0);
        assert_eq!(optionen.raster, 128);
        assert_eq!(optionen.feinheit, 32);
        assert_eq!(optionen.genre.name, "Techno");
        assert_eq!(optionen.instrument, 0);
        assert!(optionen.ausgabe.is_none());
        assert!(matches!(
            argumente(["--hilfe".to_string()]).unwrap(),
            Anfrage::Hilfe
        ));
        assert!(matches!(
            argumente(["-h".to_string()]).unwrap(),
            Anfrage::Hilfe
        ));
    }

    #[test]
    fn argumente_lesen_alle_optionen() {
        let args = [
            "stimme.m4a",
            "--bpm=96",
            "--raster",
            "256",
            "--feinheit",
            "64",
            "--genre",
            "dnb",
            "--instrument",
            "3",
            "--ausgabe",
            "out.it",
        ]
        .map(String::from);
        let Anfrage::Start(optionen) = argumente(args).unwrap() else {
            panic!("kein Start");
        };
        assert_eq!(optionen.bpm, 96.0);
        assert_eq!(optionen.raster, 256);
        assert_eq!(optionen.feinheit, 64);
        assert_eq!(optionen.genre.name, "Drum and Bass");
        assert_eq!(optionen.instrument, 3);
        assert_eq!(optionen.ausgabe.as_deref(), Some("out.it"));
    }

    #[test]
    fn argumente_lehnen_unsinn_ab() {
        for args in [
            vec!["a.wav", "--feinheit", "48"],
            vec!["a.wav", "--instrument", "4"],
            vec!["a.wav", "--raster", "0"],
            vec!["a.wav", "--bpm", "-1"],
            vec!["a.wav", "--genre", "polka"],
            vec!["a.wav", "--quatsch"],
            vec!["a.wav", "b.wav"],
            vec![],
        ] {
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            assert!(argumente(args.clone()).is_err(), "muss scheitern: {args:?}");
        }
    }

    #[test]
    fn pfade_und_endungen() {
        assert_eq!(dateiname("/tmp/mit ordner/stimme.m4a"), "stimme.m4a");
        assert_eq!(dateistamm("/tmp/mit ordner/stimme.m4a"), "stimme");
        assert_eq!(mit_endung("out.it", "csv"), "out.csv");
        assert_eq!(mit_endung("out", "csv"), "out.csv");
        assert_eq!(dateistamm(""), "jamcast");
    }
}
