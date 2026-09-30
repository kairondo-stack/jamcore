//! Mikrofon-BPM-Schätzung: Onset-Erkennung + Intervall-Histogramm.
//! Reine Funktionen, damit ohne Audiohardware testbar.

/// Fensterweise Energie im Hop-Abstand; Onset, wenn Energie deutlich über
/// dem lokalen Mittel liegt. Rückgabe: Onset-Zeitpunkte (Sekunden).
pub fn detect_onsets(samples: &[f32], sample_rate: u32, hop: usize) -> Vec<f32> {
    if hop == 0 || sample_rate == 0 || samples.len() < hop * 2 {
        return Vec::new();
    }
    let frame = hop * 2;
    let mut energies: Vec<f32> = Vec::new();
    let mut pos = 0;
    while pos + frame <= samples.len() {
        let e: f32 = samples[pos..pos + frame].iter().map(|s| s * s).sum::<f32>()
            / frame as f32;
        energies.push(e);
        pos += hop;
    }
    let mut onsets = Vec::new();
    let mut last_onset = f32::NEG_INFINITY;
    for i in 8..energies.len() {
        let mean: f32 = energies[i - 8..i].iter().sum::<f32>() / 8.0;
        let e = energies[i];
        let start = i * hop;
        let frame_samples = &samples[start..start + frame];
        let weight: f32 = frame_samples.iter().map(|s| s * s).sum();
        let centroid = if weight > 0.0 {
            frame_samples
                .iter()
                .enumerate()
                .map(|(k, s)| k as f32 * s * s)
                .sum::<f32>()
                / weight
        } else {
            0.0
        };
        let t = (start as f32 + centroid) / sample_rate as f32;
        if e > 1e-4 && e > mean * 2.0 && t - last_onset > 0.12 {
            onsets.push(t);
            last_onset = t;
        }
    }
    onsets
}

/// BPM durch Verdoppeln/Halbieren in [min_bpm, max_bpm] falten.
pub fn fold_bpm(bpm: f32, min_bpm: f32, max_bpm: f32) -> f32 {
    if !bpm.is_finite() || bpm <= 0.0 || min_bpm <= 0.0 || max_bpm < min_bpm {
        return bpm;
    }
    let mut v = bpm;
    while v < min_bpm {
        v *= 2.0;
    }
    while v > max_bpm {
        v /= 2.0;
    }
    v
}

/// BPM-Schätzung aus Onset-Zeiten: IOI-Histogramm, Oktav-Faltung,
/// Rückgabe (bpm, Konfidenz 0..1).
pub fn bpm_from_onsets(onsets: &[f32], min_bpm: f32, max_bpm: f32) -> Option<(f32, f32)> {
    if onsets.len() < 4 {
        return None;
    }
    let mut hist: std::collections::HashMap<i32, Vec<f32>> = std::collections::HashMap::new();
    let mut total = 0usize;
    for w in onsets.windows(2) {
        let ioi = w[1] - w[0];
        if ioi <= 0.05 || ioi > 2.0 {
            continue;
        }
        let bpm = fold_bpm(60.0 / ioi, min_bpm, max_bpm);
        if !(min_bpm..=max_bpm).contains(&bpm) {
            continue;
        }
        hist.entry(bpm.round() as i32).or_default().push(bpm);
        total += 1;
    }
    if total == 0 {
        return None;
    }
    let best_bin = *hist.iter().max_by_key(|(_, v)| v.len()).map(|(k, _)| k)?;
    let mut vals: Vec<f32> = Vec::new();
    for b in (best_bin - 2)..=(best_bin + 2) {
        if let Some(v) = hist.get(&b) {
            vals.extend(v);
        }
    }
    if vals.is_empty() {
        return None;
    }
    let bpm = vals.iter().sum::<f32>() / vals.len() as f32;
    Some((bpm, vals.len() as f32 / total as f32))
}

/// Sammelt die letzten ~4 s und schätzt daraus das Tempo.
pub struct BpmEstimator {
    rate: u32,
    window: Vec<f32>,
}

impl BpmEstimator {
    pub fn new(sample_rate: u32) -> Self {
        BpmEstimator {
            rate: sample_rate,
            window: Vec::with_capacity(sample_rate as usize * 4),
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        self.window.extend_from_slice(samples);
        let max = self.rate as usize * 4;
        if self.window.len() > max {
            let drop = self.window.len() - max;
            self.window.drain(..drop);
        }
    }

    pub fn estimate(&self) -> Option<(f32, f32)> {
        let onsets = detect_onsets(&self.window, self.rate, 512);
        bpm_from_onsets(&onsets, 60.0, 200.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clicks(rate: u32, seconds: f32, bpm: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        let mut v = vec![0.0f32; n];
        let period = (60.0 / bpm * rate as f32) as usize;
        let click_len = (rate as f32 * 0.01) as usize;
        let mut i = period / 2;
        while i < n {
            for k in 0..click_len {
                if i + k < n {
                    v[i + k] = 0.9;
                }
            }
            i += period;
        }
        v
    }

    fn estimate_for(bpm: f32) -> (f32, f32) {
        let samples = clicks(44100, 4.0, bpm);
        let mut est = BpmEstimator::new(44100);
        est.push(&samples);
        est.estimate().expect("Schätzung fehlt")
    }

    #[test]
    fn estimates_common_tempi() {
        for target in [90.0f32, 120.0, 128.0, 175.0] {
            let (bpm, conf) = estimate_for(target);
            assert!(
                (bpm - target).abs() < 2.5,
                "target {target}, got {bpm} (conf {conf})"
            );
            assert!(conf > 0.5, "target {target}, conf {conf}");
        }
    }

    #[test]
    fn silence_and_noise_yield_no_confident_estimate() {
        let mut quiet = BpmEstimator::new(44100);
        quiet.push(&vec![0.0f32; 44100 * 4]);
        assert!(quiet.estimate().is_none());

        let mut state = 0x12345678u32;
        let noise: Vec<f32> = (0..44100 * 4)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32) * 0.2 - 0.1
            })
            .collect();
        let mut noisy = BpmEstimator::new(44100);
        noisy.push(&noise);
        if let Some((_, conf)) = noisy.estimate() {
            assert!(conf < 0.6, "Rauschen darf nicht überzeugend sein: {conf}");
        }
    }

    #[test]
    fn fold_bpm_brings_into_range() {
        assert_eq!(fold_bpm(240.0, 60.0, 200.0), 120.0);
        assert_eq!(fold_bpm(45.0, 60.0, 200.0), 90.0);
        assert_eq!(fold_bpm(128.0, 60.0, 200.0), 128.0);
    }

    #[test]
    fn bpm_from_onsets_needs_enough_onsets() {
        assert!(bpm_from_onsets(&[0.0, 0.5], 60.0, 200.0).is_none());
        let onsets: Vec<f32> = (0..9).map(|i| i as f32 * 0.5).collect();
        let (bpm, conf) = bpm_from_onsets(&onsets, 60.0, 200.0).unwrap();
        assert!((bpm - 120.0).abs() < 1.0);
        assert!(conf > 0.9);
    }
}
