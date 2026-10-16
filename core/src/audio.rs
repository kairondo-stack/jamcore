//! Audio laden: gängige Formate über symphonia (reines Rust, Desktop **und**
//! Android) zu Mono-Samples in −1 … 1.
//!
//! Unterstützt werden WAV, MP3, Ogg/Vorbis, FLAC, M4A/AAC und AIFF
//! ([`AUDIO_ENDUNGEN`]). Mehrkanaliges Material wird zu Mono gemittelt;
//! die Abtastrate kommt aus der Datei.

use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::conv::IntoSample;
use symphonia::core::errors::Error as SymphoniaFehler;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::Sample;

/// Endungen, die als Audio gelten (für Datei-Browser).
pub const AUDIO_ENDUNGEN: [&str; 8] = ["wav", "mp3", "ogg", "oga", "flac", "m4a", "aac", "aiff"];

/// Geladenes Audio: Mono-Samples in −1 … 1 und Abtastrate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    pub proben: Vec<f32>,
    pub rate: u32,
}

/// Lädt eine Audiodatei (wav/mp3/ogg/flac/m4a/aiff) über symphonia.
///
/// Mehrkanaliges Material wird zu Mono gemittelt (Mittelwert der Kanäle),
/// Fehler kommen als `Err` mit Dateiname und Grund zurück.
pub fn laden(pfad: &str) -> Result<Audio, String> {
    let datei = std::fs::File::open(pfad).map_err(|e| format!("{pfad}: {e}"))?;
    let quelle = MediaSourceStream::new(Box::new(datei), Default::default());

    // Die Endung hilft beim Erkennen (Inhalt bleibt maßgeblich).
    let mut hinweis = Hint::new();
    if let Some(endung) = std::path::Path::new(pfad).extension().and_then(|e| e.to_str()) {
        hinweis.with_extension(endung);
    }

    let erkannt = symphonia::default::get_probe()
        .format(
            &hinweis,
            quelle,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("{pfad}: unbekanntes Audioformat ({e})"))?;
    let mut format = erkannt.format;

    let spur = format
        .default_track()
        .ok_or_else(|| format!("{pfad}: keine Tonspur"))?;
    let spur_id = spur.id;
    let mut rate = spur.codec_params.sample_rate.unwrap_or(0);

    let mut decoder = symphonia::default::get_codecs()
        .make(&spur.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("{pfad}: kein Decoder für diese Spur ({e})"))?;

    let mut proben: Vec<f32> = Vec::new();
    loop {
        let paket = match format.next_packet() {
            Ok(paket) => paket,
            // Normales Ende der Datei (auch bei verketteten Ogg-Strömen).
            Err(SymphoniaFehler::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaFehler::ResetRequired) => break,
            Err(e) => return Err(format!("{pfad}: Lesefehler ({e})")),
        };
        if paket.track_id() != spur_id {
            continue;
        }
        match decoder.decode(&paket) {
            Ok(puffer) => {
                if rate == 0 {
                    rate = puffer.spec().rate;
                }
                sammle_puffer(&puffer, &mut proben);
            }
            // Ein kaputtes Paket überspringen: der Rest bleibt brauchbar.
            Err(SymphoniaFehler::DecodeError(_)) => continue,
            Err(e) => return Err(format!("{pfad}: Dekodierfehler ({e})")),
        }
    }

    if proben.is_empty() {
        return Err(format!("{pfad}: keine Audio-Daten"));
    }
    if rate == 0 {
        return Err(format!("{pfad}: Abtastrate fehlt"));
    }
    Ok(Audio { proben, rate })
}

/// Prüft eine Dateiendung gegen [`AUDIO_ENDUNGEN`] (Groß-/Kleinschreibung
/// egal).
pub fn ist_audio(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((_, endung)) => AUDIO_ENDUNGEN
            .iter()
            .any(|e| e.eq_ignore_ascii_case(endung)),
        None => false,
    }
}

/// Kanäle eines Puffers zu Mono mischen (Mittelwert) und an `ziel` anhängen.
/// Werte werden auf −1 … 1 geklemmt.
fn mische_puffer<S: Sample + IntoSample<f32>>(puffer: &AudioBuffer<S>, ziel: &mut Vec<f32>) {
    let kanaele = puffer.spec().channels.count();
    if kanaele == 0 {
        return;
    }
    let frames = puffer.frames();
    ziel.reserve(frames);
    for f in 0..frames {
        let mut summe = 0.0f32;
        for k in 0..kanaele {
            summe += puffer.chan(k)[f].into_sample();
        }
        ziel.push((summe / kanaele as f32).clamp(-1.0, 1.0));
    }
}

/// Ein dekodierter Puffer, egal welcher Sample-Typ.
fn sammle_puffer(puffer: &AudioBufferRef, ziel: &mut Vec<f32>) {
    match puffer {
        AudioBufferRef::U8(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::U16(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::U24(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::U32(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::S8(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::S16(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::S24(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::S32(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::F32(puffer) => mische_puffer(puffer, ziel),
        AudioBufferRef::F64(puffer) => mische_puffer(puffer, ziel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::{Channels, SignalSpec};

    #[test]
    fn audio_endungen() {
        assert_eq!(AUDIO_ENDUNGEN.len(), 8);
        for name in [
            "aufnahme.wav",
            "song.MP3",
            "beat.flac",
            "stimme.m4a",
            "loop.ogg",
            "loop.oga",
            "drum.aac",
            "take.aiff",
            "/pfad/mit/ordner/Take.WAV",
        ] {
            assert!(ist_audio(name), "{name} ist Audio");
        }
        for name in ["bild.png", "modul.it", "ohneendung", "", "wav", "archiv.wav.zip"] {
            assert!(!ist_audio(name), "{name} ist kein Audio");
        }
    }

    #[test]
    fn stereo_wird_gemischt() {
        let spec = SignalSpec::new(44100, Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
        let mut puffer = AudioBuffer::<f32>::new(4, spec);
        puffer.render_reserved(Some(4));
        puffer.chan_mut(0).copy_from_slice(&[1.0, 0.5, -1.0, 0.0]);
        puffer.chan_mut(1).copy_from_slice(&[0.0, 0.5, 1.0, 0.0]);
        let mut ziel = Vec::new();
        mische_puffer(&puffer, &mut ziel);
        assert_eq!(ziel, vec![0.5, 0.5, 0.0, 0.0], "Mittelwert der Kanäle");

        // 16-Bit-PCM (so kommt WAV an) wird auf −1 … 1 skaliert.
        let spec = SignalSpec::new(16000, Channels::FRONT_LEFT);
        let mut puffer = AudioBuffer::<i16>::new(3, spec);
        puffer.render_reserved(Some(3));
        puffer
            .chan_mut(0)
            .copy_from_slice(&[i16::MAX, 0, i16::MIN]);
        let mut ziel = Vec::new();
        mische_puffer(&puffer, &mut ziel);
        assert_eq!(ziel.len(), 3);
        assert!(ziel[0] > 0.99 && ziel[0] <= 1.0, "Maximum: {}", ziel[0]);
        assert_eq!(ziel[1], 0.0);
        assert_eq!(ziel[2], -1.0, "Minimum");
    }
}
