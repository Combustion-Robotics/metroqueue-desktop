//! Tempo, beat grid, key and level structure (silence, intro, outro) of mono PCM. A port of
//! MetroQueue's `MixAnalyzer.kt`, constants unchanged so both apps plan alike; the waveform it also
//! draws for the transition editor is left out, the desktop has no editor.

use std::f64::consts::PI;

use super::planner::{MixAnalysis, MusicalKey};

const ENERGY_FRAME_MS: usize = 50;
const SILENCE_DB: f64 = -50.0;
const SUSTAIN_FRAMES: usize = 6;
const MIN_AUDIBLE_FRAMES: usize = 20;
const SMOOTH_RADIUS_FRAMES: usize = 10;
// Below the full level by this much counts as a fade or quiet ending.
const OUTRO_DROP_DB: f64 = 6.0;
const FADE_TOP_DB: f64 = 1.5;
const FADE_SMOOTH_RADIUS_FRAMES: usize = 20;
const FADE_STEP_FRAMES: usize = 10;
const FADE_MIN_RISE_DB: f64 = 0.2;
const MAX_FADE_WALK_FRAMES: usize = 300;

const FFT_SIZE: usize = 512;
const HOP: usize = 128;
const LOG_COMPRESSION: f64 = 1e4;
const LOCAL_MEAN_FRAMES: usize = 12;
// A transient shows up in the flux once it enters the window, i.e. before the window's centre.
const ONSET_LATENCY_SAMPLES: usize = FFT_SIZE * 5 / 8;

const MIN_BPM: f64 = 70.0;
const MAX_BPM: f64 = 180.0;
const BPM_STEP: f64 = 0.05;
const HARMONICS: usize = 4;
const PRIOR_BPM: f64 = 120.0;
const PRIOR_WIDTH_OCTAVES: f64 = 1.0;
const CONFIDENCE_FULL_SCALE: f64 = 0.3;
const BEAT_TOLERANCE_MS: f64 = 40.0;
const REFINE_STEPS: i32 = 20;
const REFINE_STEP: f64 = 0.0005;

// ~1.4 Hz bins at the analysis rate: enough to tell semitones apart down to A1 (55 Hz).
const KEY_FFT_SIZE: usize = 8192;
const KEY_HOP: usize = 4096;
const KEY_LOWEST_NOTE: usize = 33; // A1
const KEY_HIGHEST_NOTE: usize = 95; // B6
const KEY_LOG_COMPRESSION: f64 = 1e3;
const KEY_FLOOR_BINS: usize = 12;
const KEY_MARGIN_FULL_SCALE: f64 = 0.1;

// Krumhansl-Kessler key profiles, from the tonic up in semitones.
const MAJOR_PROFILE: [f64; 12] =
    [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR_PROFILE: [f64; 12] =
    [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

/// Analyses `samples` (mono, -1..1) that start at `start_ms` of a track; `None` if too short or
/// silent.
pub fn analyze(
    samples: &[f32],
    sample_rate: usize,
    start_ms: i64,
    track_duration_ms: i64,
) -> Option<MixAnalysis> {
    let frame_len = sample_rate * ENERGY_FRAME_MS / 1000;
    if frame_len == 0 {
        return None;
    }
    let frames = samples.len() / frame_len;
    if frames < MIN_AUDIBLE_FRAMES {
        return None;
    }
    let power: Vec<f64> = samples
        .chunks_exact(frame_len)
        .map(|f| f.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / frame_len as f64)
        .collect();
    let audible: Vec<bool> = power.iter().map(|&p| to_db(p) > SILENCE_DB).collect();
    let smooth = smoothed_db(&power, SMOOTH_RADIUS_FRAMES);
    // Levels over ~1 s, so beats and the gaps between them don't skew them.
    let mut levels: Vec<f64> = (0..frames).filter(|&i| audible[i]).map(|i| smooth[i]).collect();
    if levels.len() < MIN_AUDIBLE_FRAMES {
        return None;
    }
    levels.sort_by(f64::total_cmp);
    let body_db = levels[levels.len() * 3 / 4];
    let loudness_db = levels[levels.len() / 2];
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));

    let sustained = |from: usize, to: usize| (from..to).all(|i| audible[i]);
    let first = (0..=frames - SUSTAIN_FRAMES).find(|&i| sustained(i, i + SUSTAIN_FRAMES))?;
    let last = (first + SUSTAIN_FRAMES - 1..frames)
        .rev()
        .find(|&i| sustained(i + 1 - SUSTAIN_FRAMES, i + 1))?;

    let full = body_db - OUTRO_DROP_DB;
    let intro_end = (first..=last).find(|&i| smooth[i] >= full).unwrap_or(first);
    let mut outro_last = (first..=last).rev().find(|&i| smooth[i] >= full).unwrap_or(last);
    // A fade crosses the threshold late (halfway, for a linear one); follow the decline back to its
    // top, in steps on a slower envelope so the beat's ripple doesn't stop the walk.
    let slow = smoothed_db(&power, FADE_SMOOTH_RADIUS_FRAMES);
    let walk_limit = outro_last.saturating_sub(MAX_FADE_WALK_FRAMES).max(first);
    while outro_last >= walk_limit + FADE_STEP_FRAMES
        && slow[outro_last] < body_db - FADE_TOP_DB
        && slow[outro_last - FADE_STEP_FRAMES] > slow[outro_last] + FADE_MIN_RISE_DB
    {
        outro_last -= FADE_STEP_FRAMES;
    }

    let frame_ms = |f: usize| start_ms + (f * ENERGY_FRAME_MS) as i64;
    let audio_start_ms = frame_ms(first);
    let tempo = estimate_tempo(samples, sample_rate, first * frame_len, (last + 1) * frame_len);
    let (beat_phase_ms, first_beat_ms) = match &tempo {
        Some(t) => {
            let phase = start_ms as f64 + t.phase_ms;
            let beat = 60_000.0 / t.bpm;
            let beats = ((audio_start_ms as f64 - BEAT_TOLERANCE_MS - phase) / beat).ceil();
            (phase.round() as i64, ((phase + beats * beat).round() as i64).max(start_ms))
        }
        None => (audio_start_ms, audio_start_ms),
    };

    Some(MixAnalysis {
        track_duration_ms,
        region_start_ms: start_ms,
        region_end_ms: start_ms + (samples.len() * 1000 / sample_rate) as i64,
        bpm: tempo.as_ref().map_or(0.0, |t| t.bpm),
        bpm_confidence: tempo.as_ref().map_or(0.0, |t| t.confidence),
        beat_phase_ms,
        first_beat_ms,
        audio_start_ms,
        intro_end_ms: frame_ms(intro_end),
        outro_start_ms: frame_ms(outro_last + 1),
        end_of_audio_ms: frame_ms(last + 1),
        loudness_db,
        body_db,
        peak_db: 20.0 * (peak as f64 + 1e-9).log10(),
        musical_key: Some(estimate_key(samples, sample_rate)),
    })
}

/// Correlates a chromagram of `samples` with the Krumhansl-Kessler profiles in all 24 keys. The
/// confidence is how far the best key leads the runner-up: 0 without a clear tonal centre.
pub fn estimate_key(samples: &[f32], sample_rate: usize) -> MusicalKey {
    let chroma = chromagram(samples, sample_rate);
    let scores: Vec<f64> = (0..24)
        .map(|key| {
            let profile = if key < 12 { &MAJOR_PROFILE } else { &MINOR_PROFILE };
            correlation(&chroma, key % 12, profile)
        })
        .collect();
    let mut order: Vec<usize> = (0..24).collect();
    // A flat chroma (silence) scores NaN in every key; its margin is then 0.
    order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
    let best = order[0];
    let margin = if scores[best].is_nan() { 0.0 } else { scores[best] - scores[order[1]] };
    MusicalKey {
        tonic: (best % 12) as i32,
        minor: best >= 12,
        confidence: (margin / KEY_MARGIN_FULL_SCALE).clamp(0.0, 1.0),
    }
}

/// Per frame and semitone (A1..B6), the strongest log magnitude above the local spectral floor,
/// weighted down towards the semitone's edges; folded to 12 pitch classes (0 = C).
fn chromagram(samples: &[f32], sample_rate: usize) -> [f64; 12] {
    let bin_hz = sample_rate as f64 / KEY_FFT_SIZE as f64;
    let bins =
        (KEY_FFT_SIZE / 2 + 1).min((midi_hz(KEY_HIGHEST_NOTE as f64 + 0.5) / bin_hz) as usize + 1);
    let mut bin_note = vec![usize::MAX; bins];
    let mut bin_weight = vec![0.0; bins];
    for k in 1..bins {
        let midi = 69.0 + 12.0 * (k as f64 * bin_hz / 440.0).log2();
        let note = midi.round();
        if (KEY_LOWEST_NOTE as f64..=KEY_HIGHEST_NOTE as f64).contains(&note) {
            bin_note[k] = note as usize;
            bin_weight[k] = (PI * (midi - note)).cos().powi(2);
        }
    }
    let hann = hann(KEY_FFT_SIZE);
    let mut chroma = [0.0; 12];
    let mut notes = vec![0.0; KEY_HIGHEST_NOTE + 1];
    let mut log_mag = vec![0.0; bins];
    let mut windowed = vec![0.0; KEY_FFT_SIZE];
    let mut offset = 0;
    while offset + KEY_FFT_SIZE <= samples.len() {
        for i in 0..KEY_FFT_SIZE {
            windowed[i] = samples[offset + i] as f64 * hann[i];
        }
        let power = rfft_power(&windowed);
        for k in 0..bins {
            log_mag[k] = (1.0 + KEY_LOG_COMPRESSION * power[k].sqrt()).ln();
        }
        notes.fill(0.0);
        let (mut floor_sum, mut floor_start, mut floor_end) = (0.0, 0, 0);
        for k in 1..bins {
            while floor_end < bins && floor_end <= k + KEY_FLOOR_BINS {
                floor_sum += log_mag[floor_end];
                floor_end += 1;
            }
            while floor_start + KEY_FLOOR_BINS < k {
                floor_sum -= log_mag[floor_start];
                floor_start += 1;
            }
            let note = bin_note[k];
            if note == usize::MAX {
                continue;
            }
            let level = bin_weight[k] * (log_mag[k] - floor_sum / (floor_end - floor_start) as f64);
            if level > notes[note] {
                notes[note] = level;
            }
        }
        for note in KEY_LOWEST_NOTE..=KEY_HIGHEST_NOTE {
            chroma[note % 12] += notes[note];
        }
        offset += KEY_HOP;
    }
    chroma
}

/// Pearson correlation of `chroma` read from `tonic` on with `profile`; NaN for a flat chroma.
fn correlation(chroma: &[f64; 12], tonic: usize, profile: &[f64; 12]) -> f64 {
    let cm = chroma.iter().sum::<f64>() / 12.0;
    let pm = profile.iter().sum::<f64>() / 12.0;
    let (mut product, mut cs, mut ps) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        let c = chroma[(tonic + i) % 12] - cm;
        let p = profile[i] - pm;
        product += c * p;
        cs += c * c;
        ps += p * p;
    }
    product / (cs * ps).sqrt()
}

fn midi_hz(note: f64) -> f64 {
    440.0 * 2f64.powf((note - 69.0) / 12.0)
}

struct Tempo {
    bpm: f64,
    confidence: f64,
    /// Relative to the start of the analysed samples.
    phase_ms: f64,
}

/// Autocorrelates a spectral-flux onset envelope over 70-180 BPM. Each candidate is scored on its
/// first few beat multiples, so double and half tempo score alike; a mild prior around 120 BPM
/// settles those octave ambiguities.
fn estimate_tempo(samples: &[f32], sample_rate: usize, from: usize, to: usize) -> Option<Tempo> {
    let onset = onset_envelope(samples, from, to);
    let hop_ms = HOP as f64 * 1000.0 / sample_rate as f64;
    let max_period = 60_000.0 / MIN_BPM / hop_ms;
    let max_lag = (max_period * HARMONICS as f64).ceil() as usize + 1;
    if onset.len() < max_lag * 2 {
        return None;
    }
    let mean = onset.iter().sum::<f64>() / onset.len() as f64;
    let centered: Vec<f64> = onset.iter().map(|v| v - mean).collect();
    let mut acf: Vec<f64> = (0..=max_lag)
        .map(|lag| {
            let n = centered.len() - lag;
            (0..n).map(|t| centered[t] * centered[t + lag]).sum::<f64>() / n as f64
        })
        .collect();
    if acf[0] <= 1e-12 {
        return None;
    }
    let zero = acf[0];
    acf.iter_mut().for_each(|v| *v /= zero);

    let candidates = ((MAX_BPM - MIN_BPM) / BPM_STEP) as usize + 1;
    let mut scores = vec![0.0; candidates];
    let (mut best, mut best_weighted) = (0, f64::NEG_INFINITY);
    for (c, score) in scores.iter_mut().enumerate() {
        let bpm = MIN_BPM + c as f64 * BPM_STEP;
        let period = 60_000.0 / bpm / hop_ms;
        *score = (1..=HARMONICS).map(|m| interpolate(&acf, m as f64 * period)).sum::<f64>()
            / HARMONICS as f64;
        let octaves = (bpm / PRIOR_BPM).log2();
        let weighted = *score * (-0.5 * octaves * octaves / PRIOR_WIDTH_OCTAVES.powi(2)).exp();
        if weighted > best_weighted {
            best_weighted = weighted;
            best = c;
        }
    }
    let mut sorted = scores.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[candidates / 2];
    let confidence = ((scores[best] - median) / CONFIDENCE_FULL_SCALE).clamp(0.0, 1.0);

    // The ACF peak is only as sharp as one hop; refine period and phase together by fitting the
    // beat grid to the whole (lightly smoothed) onset envelope.
    let last = onset.len() - 1;
    let smooth: Vec<f64> = (0..onset.len())
        .map(|t| {
            0.25 * onset[t.saturating_sub(1)] + 0.5 * onset[t] + 0.25 * onset[(t + 1).min(last)]
        })
        .collect();
    let coarse = 60_000.0 / (MIN_BPM + best as f64 * BPM_STEP) / hop_ms;
    let (mut period, mut phase, mut best_fit) = (coarse, 0.0, f64::NEG_INFINITY);
    for step in -REFINE_STEPS..=REFINE_STEPS {
        let candidate = coarse * (1.0 + step as f64 * REFINE_STEP);
        let (p, fit) = fit_phase(&smooth, candidate);
        if fit > best_fit {
            best_fit = fit;
            period = candidate;
            phase = p;
        }
    }
    Some(Tempo {
        bpm: 60_000.0 / (period * hop_ms),
        confidence,
        phase_ms: (from as f64 + phase * HOP as f64 + ONSET_LATENCY_SAMPLES as f64) * 1000.0
            / sample_rate as f64,
    })
}

/// The grid offset (in frames) that collects the most onset strength per beat, and that mean.
fn fit_phase(onset: &[f64], period: f64) -> (f64, f64) {
    let means: Vec<f64> = (0..period.ceil() as usize)
        .map(|phase| {
            let (mut sum, mut beats, mut pos) = (0.0, 0, phase as f64);
            while pos < (onset.len() - 1) as f64 {
                sum += interpolate(onset, pos);
                beats += 1;
                pos += period;
            }
            if beats > 0 {
                sum / beats as f64
            } else {
                0.0
            }
        })
        .collect();
    // First of equal maxima, like Kotlin's maxByOrNull.
    let best = (0..means.len()).fold(0, |b, i| if means[i] > means[b] { i } else { b });
    let n = means.len();
    let left = means[(best + n - 1) % n];
    let right = means[(best + 1) % n];
    let curvature = left - 2.0 * means[best] + right;
    let offset = if curvature < 0.0 { 0.5 * (left - right) / curvature } else { 0.0 };
    (best as f64 + offset, means[best])
}

/// Half-wave rectified spectral flux of log-compressed spectra, minus its local mean.
fn onset_envelope(samples: &[f32], from: usize, to: usize) -> Vec<f64> {
    let frames = if to - from >= FFT_SIZE { (to - from - FFT_SIZE) / HOP + 1 } else { 0 };
    let hann = hann(FFT_SIZE);
    let mut flux = vec![0.0; frames];
    let mut windowed = vec![0.0; FFT_SIZE];
    let mut previous: Option<Vec<f64>> = None;
    for (f, value) in flux.iter_mut().enumerate() {
        let offset = from + f * HOP;
        for i in 0..FFT_SIZE {
            windowed[i] = samples[offset + i] as f64 * hann[i];
        }
        let mut spectrum = rfft_power(&windowed);
        spectrum.iter_mut().for_each(|v| *v = (1.0 + LOG_COMPRESSION * *v).ln());
        if let Some(prev) = &previous {
            // Skip DC.
            *value = (1..spectrum.len()).map(|k| (spectrum[k] - prev[k]).max(0.0)).sum();
        }
        previous = Some(spectrum);
    }
    let (mut sum, mut start, mut end) = (0.0, 0, 0);
    (0..frames)
        .map(|f| {
            while end < frames && end <= f + LOCAL_MEAN_FRAMES {
                sum += flux[end];
                end += 1;
            }
            while start + LOCAL_MEAN_FRAMES < f {
                sum -= flux[start];
                start += 1;
            }
            (flux[f] - sum / (end - start) as f64).max(0.0)
        })
        .collect()
}

fn smoothed_db(power: &[f64], radius: usize) -> Vec<f64> {
    (0..power.len())
        .map(|i| {
            let (s, e) = (i.saturating_sub(radius), (i + radius).min(power.len() - 1));
            to_db(power[s..=e].iter().sum::<f64>() / (e - s + 1) as f64)
        })
        .collect()
}

fn interpolate(values: &[f64], position: f64) -> f64 {
    let index = position.floor();
    if index < 0.0 {
        return values[0];
    }
    let i = index as usize;
    if i >= values.len() - 1 {
        return values[values.len() - 1];
    }
    let fraction = position - index;
    values[i] * (1.0 - fraction) + values[i + 1] * fraction
}

fn to_db(power: f64) -> f64 {
    10.0 * (power + 1e-12).log10()
}

fn hann(n: usize) -> Vec<f64> {
    (0..n).map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / n as f64).cos())).collect()
}

/// n/2+1 power values of a real FFT (n a power of two), scaled like MetroQueue's `computeRfft`
/// (|X|² / 2^17, floored at 1e-10) so the log compression constants mean the same.
fn rfft_power(input: &[f64]) -> Vec<f64> {
    let n = input.len();
    let mut re = input.to_vec();
    let mut im = vec![0.0; n];
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let (wr, wi) = ((-PI / half as f64).cos(), (-PI / half as f64).sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..half {
                let (u, v) = (start + k, start + k + half);
                let (or, oi) = (re[v] * cr - im[v] * ci, re[v] * ci + im[v] * cr);
                (re[v], im[v]) = (re[u] - or, im[u] - oi);
                re[u] += or;
                im[u] += oi;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
    (0..=n / 2).map(|k| ((re[k] * re[k] + im[k] * im[k]) / (1 << 17) as f64).max(1e-10)).collect()
}

#[cfg(test)]
mod tests {
    //! MixAnalyzerTest.kt, minus the waveform test.
    use super::*;

    const RATE: usize = 12_000;

    fn signal(duration_ms: usize, value: impl Fn(f64) -> f64) -> Vec<f32> {
        (0..RATE * duration_ms / 1000)
            .map(|i| value(i as f64 * 1000.0 / RATE as f64).clamp(-1.0, 1.0) as f32)
            .collect()
    }

    /// A sustained chord under the beat, so the signal never drops to silence between clicks.
    fn pad(ms: f64) -> f64 {
        let t = ms / 1000.0;
        0.08 * ((2.0 * PI * 220.0 * t).sin()
            + (2.0 * PI * 277.0 * t).sin()
            + (2.0 * PI * 330.0 * t).sin())
    }

    /// A decaying burst (hi-hat/kick stand-in) on every beat from `first`.
    fn click(ms: f64, first: f64, beat: f64) -> f64 {
        if ms < first {
            return 0.0;
        }
        let since = (ms - first) % beat;
        if since > 60.0 {
            return 0.0;
        }
        0.7 * (-since / 12.0).exp()
            * ((2.0 * PI * 1_700.0 * ms / 1000.0).sin()
                + 0.6 * (2.0 * PI * 90.0 * ms / 1000.0).sin())
    }

    /// Deterministic noise in -0.5..0.5 (Kotlin's `Random(seed)` stand-in).
    fn noise(seed: u64) -> impl Fn(f64) -> f64 {
        let state = std::cell::Cell::new(seed.wrapping_mul(6364136223846793005).wrapping_add(1));
        move |_| {
            let s = state.get().wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            state.set(s);
            (s >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        }
    }

    fn run(samples: &[f32], start_ms: i64) -> MixAnalysis {
        let duration = start_ms + (samples.len() * 1000 / RATE) as i64;
        analyze(samples, RATE, start_ms, duration).expect("analysis")
    }

    fn grid_error(position: i64, first: f64, beat: f64) -> f64 {
        let offset = ((position as f64 - first) % beat + beat) % beat;
        offset.min(beat - offset)
    }

    #[test]
    fn click_track_tempo_and_beat_grid_are_found() {
        for bpm in [92.0, 100.0, 120.0, 128.0, 140.0] {
            let beat = 60_000.0 / bpm;
            let a = run(&signal(30_000, |ms| pad(ms) + click(ms, 230.0, beat)), 0);
            assert!((a.bpm - bpm).abs() <= 2.0, "bpm for {bpm}: {}", a.bpm);
            assert!(a.has_confident_tempo(), "confidence for {bpm}: {}", a.bpm_confidence);
            assert!(grid_error(a.beat_phase_ms, 230.0, beat) <= 30.0, "phase {}", a.beat_phase_ms);
            assert!(grid_error(a.first_beat_ms, 230.0, beat) <= 30.0, "first {}", a.first_beat_ms);
        }
    }

    #[test]
    fn noise_has_no_confident_tempo() {
        let n = noise(7);
        assert!(!run(&signal(30_000, |ms| n(ms) * 0.4), 0).has_confident_tempo());
    }

    #[test]
    fn fade_out_and_trailing_silence_are_detected() {
        // 18 s of music, a 9 s linear fade, then 3 s of silence.
        let samples = signal(30_000, |ms| {
            let gain = if ms < 18_000.0 {
                1.0
            } else if ms < 27_000.0 {
                1.0 - (ms - 18_000.0) / 9_000.0
            } else {
                0.0
            };
            gain * (pad(ms) + click(ms, 0.0, 500.0))
        });
        let start = 150_000;
        let a = run(&samples, start);
        assert!((a.outro_start_ms - (start + 18_000)).abs() <= 2_500, "outro {}", a.outro_start_ms);
        assert!(
            (start + 24_000..=start + 27_100).contains(&a.end_of_audio_ms),
            "end {}",
            a.end_of_audio_ms
        );
        assert!(a.outro_start_ms < a.end_of_audio_ms - 3_000);
    }

    #[test]
    fn abrupt_ending_has_its_outro_at_the_end_of_the_audio() {
        let a = run(
            &signal(30_000, |ms| if ms < 28_000.0 { pad(ms) + click(ms, 0.0, 500.0) } else { 0.0 }),
            0,
        );
        assert!((a.end_of_audio_ms - 28_000).abs() <= 100);
        assert!(a.end_of_audio_ms - a.outro_start_ms <= 1_000);
    }

    #[test]
    fn leading_silence_and_a_quiet_intro_are_detected() {
        // 2.5 s silence, 4 s quiet intro (-20 dB), then the full song.
        let a = run(
            &signal(30_000, |ms| match ms {
                ms if ms < 2_500.0 => 0.0,
                ms if ms < 6_500.0 => 0.1 * pad(ms),
                ms => pad(ms) + click(ms, 6_500.0, 500.0),
            }),
            0,
        );
        assert!((a.audio_start_ms - 2_500).abs() <= 100, "start {}", a.audio_start_ms);
        assert!((a.intro_end_ms - 6_500).abs() <= 600, "intro {}", a.intro_end_ms);
        assert!((a.end_of_audio_ms - 30_000).abs() <= 100);
    }

    #[test]
    fn silence_is_not_analysed() {
        assert!(analyze(&vec![0.0; RATE * 10], RATE, 0, 10_000).is_none());
    }

    const MAJOR_CADENCE: [&[i32]; 4] =
        [&[36, 48, 52, 55, 60], &[41, 53, 57, 60], &[43, 55, 59, 62], &[36, 48, 52, 55, 60]];
    const MINOR_CADENCE: [&[i32]; 4] =
        [&[45, 57, 60, 64], &[38, 50, 53, 57], &[40, 52, 56, 59], &[45, 57, 60, 64]];

    /// Chords of MIDI notes, 2 s each, transposed; each note has a few harmonics.
    fn progression(chords: &[&[i32]; 4], transpose: i32) -> Vec<f32> {
        signal(30_000, |ms| {
            let chord = chords[(ms / 2_000.0) as usize % chords.len()];
            chord
                .iter()
                .map(|&note| {
                    let hz = 440.0 * 2f64.powf((note + transpose - 69) as f64 / 12.0);
                    (1..=4)
                        .map(|h| {
                            0.05 * 0.5f64.powi(h - 1)
                                * (2.0 * PI * h as f64 * hz * ms / 1000.0).sin()
                        })
                        .sum::<f64>()
                })
                .sum()
        })
    }

    #[test]
    fn key_of_a_major_chord_progression_is_found() {
        for tonic in [0, 3, 7] {
            let key = estimate_key(&progression(&MAJOR_CADENCE, tonic), RATE);
            assert_eq!((key.tonic, key.minor), (tonic, false), "{key:?}");
            assert!(key.is_confident(), "{key:?}");
        }
    }

    #[test]
    fn key_of_a_minor_chord_progression_is_found() {
        for tonic in [9, 0, 4] {
            let key = estimate_key(&progression(&MINOR_CADENCE, tonic - 9), RATE);
            assert_eq!((key.tonic, key.minor), (tonic, true), "{key:?}");
            assert!(key.is_confident(), "{key:?}");
        }
    }

    #[test]
    fn key_survives_drums_and_noise() {
        let chords = progression(&MAJOR_CADENCE, 7);
        let n = noise(11);
        let samples = signal(30_000, |ms| {
            chords[(ms * RATE as f64 / 1000.0) as usize] as f64
                + click(ms, 0.0, 500.0)
                + n(ms) * 0.2
        });
        let key = run(&samples, 0).musical_key.unwrap();
        assert_eq!(key.camelot_number(), 9);
        assert!(!key.minor && key.is_confident(), "{key:?}");
    }

    #[test]
    fn noise_has_no_confident_key() {
        let n = noise(3);
        assert!(!estimate_key(&signal(30_000, |ms| n(ms) * 0.4), RATE).is_confident());
        assert_eq!(estimate_key(&vec![0.0; RATE * 10], RATE).confidence, 0.0);
    }
}
