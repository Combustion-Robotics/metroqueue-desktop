//! Transition planning, ported from MetroQueue's `playback/mix/` (TransitionPlan.kt, MusicalKey.kt,
//! ManualSkipPolicy.kt, SmartMixPlanner.kt, CustomTransitionPlanner.kt). Pure, all times in ms of
//! track position, so the Kotlin unit tests port one to one.

use serde::{Deserialize, Serialize};

use crate::mqsync::Transition as CustomTransition;
use player::Style;

/// How the engine moves from the current track into the next one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionPlan {
    /// Outgoing position at which the transition starts.
    pub start_position_ms: i64,
    /// Length of the overlap, in media time of the outgoing track.
    pub duration_ms: i64,
    /// Where the incoming track starts; `None` starts it from the top.
    pub incoming_start_ms: Option<i64>,
    pub style: Style,
    pub echo_delay_ms: Option<i64>,
    /// Speed of the incoming track during the overlap (tempo match).
    pub incoming_speed: f64,
}

/// What [`super::analyzer::analyze`] found in one region of a song (`MixAnalysis` in MetroQueue).
/// Absolute track positions in ms; the analysed region is `[region_start_ms, region_end_ms)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixAnalysis {
    pub track_duration_ms: i64,
    pub region_start_ms: i64,
    pub region_end_ms: i64,
    pub bpm: f64,
    pub bpm_confidence: f64,
    pub beat_phase_ms: i64,
    pub first_beat_ms: i64,
    pub audio_start_ms: i64,
    pub intro_end_ms: i64,
    pub outro_start_ms: i64,
    pub end_of_audio_ms: i64,
    pub loudness_db: f64,
    pub body_db: f64,
    pub peak_db: f64,
    #[serde(default)]
    pub musical_key: Option<MusicalKey>,
}

const CONFIDENT_TEMPO: f64 = 0.5;

impl MixAnalysis {
    pub fn has_confident_tempo(&self) -> bool {
        self.bpm > 0.0 && self.bpm_confidence >= CONFIDENT_TEMPO
    }

    pub fn beat_ms(&self) -> f64 {
        if self.bpm > 0.0 {
            60_000.0 / self.bpm
        } else {
            0.0
        }
    }
}

/// How well two keys sound together while they overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harmony {
    Compatible,
    Neutral,
    Clash,
}

/// `tonic` as a pitch class (0 = C … 11 = B), major or minor, and how clear it is (0..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MusicalKey {
    pub tonic: i32,
    pub minor: bool,
    pub confidence: f64,
}

const CONFIDENT_KEY: f64 = 0.5;

impl MusicalKey {
    pub fn is_confident(&self) -> bool {
        self.confidence >= CONFIDENT_KEY
    }

    /// Position on the Camelot wheel, 1..12 (C major = 8B, A minor = 8A).
    pub fn camelot_number(&self) -> i32 {
        let relative_major = if self.minor { (self.tonic + 3) % 12 } else { self.tonic };
        (relative_major * 7 + 7) % 12 + 1
    }

    #[cfg(test)]
    fn camelot_code(&self) -> String {
        format!("{}{}", self.camelot_number(), if self.minor { 'A' } else { 'B' })
    }

    pub fn harmony_with(&self, other: &MusicalKey) -> Harmony {
        let d = (self.camelot_number() - other.camelot_number()).abs();
        let steps = d.min(12 - d);
        let same_mode = self.minor == other.minor;
        if steps == 0 || (steps == 1 && same_mode) {
            Harmony::Compatible
        } else if steps == 1 || (steps == 2 && same_mode) {
            Harmony::Neutral
        } else {
            Harmony::Clash
        }
    }
}

/// How a user-initiated next moves to its target when "Crossfade on manual skip" is on.
pub mod manual_skip {
    /// Longest blend a skip gets, whatever the configured crossfade duration.
    pub const MAX_BLEND_MS: i64 = 1_000;
    /// A skip this soon after the previous one cuts straight to its target.
    pub const RAPID_SKIP_WINDOW_MS: i64 = 1_500;

    pub fn should_blend(
        now_ms: i64,
        last_skip_at_ms: Option<i64>,
        transition_running: bool,
    ) -> bool {
        !transition_running
            && last_skip_at_ms.map_or(true, |last| now_ms - last >= RAPID_SKIP_WINDOW_MS)
    }

    pub fn blend_duration_ms(configured_ms: i64) -> i64 {
        configured_ms.clamp(0, MAX_BLEND_MS)
    }
}

/// Why smart mix picked a style, in the order its rules are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleReason {
    FadesOut,
    TempoMatch,
    TempoClash,
    KeyClash,
    LevelClash,
    AbruptEndStrongStart,
    AbruptEnd,
    Default,
}

impl StyleReason {
    pub fn style(self) -> Style {
        match self {
            Self::FadesOut | Self::Default => Style::Fade,
            Self::TempoMatch => Style::Filter,
            Self::TempoClash | Self::KeyClash | Self::LevelClash => Style::Echo,
            Self::AbruptEndStrongStart => Style::Cut,
            Self::AbruptEnd => Style::Rise,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoFit {
    Matched,
    Aligned,
    TooFar,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmartMixReason {
    pub style: Option<StyleReason>,
    pub tempo: Option<TempoFit>,
    pub harmony: Option<Harmony>,
    pub with_incoming: bool,
}

const DURATION_TOLERANCE_MS: i64 = 2_000;
const CUT_END_MARGIN_MS: i64 = 150;
const FADE_OUT_MIN_MS: i64 = 3_000;
const ABRUPT_END_MAX_MS: i64 = 1_500;
const STRONG_START_MS: i64 = 1_000;
const TEMPO_MATCH_TOLERANCE: f64 = 0.06;
const MIN_TEMPO_NUDGE: f64 = 0.005;
const MAX_TEMPO_NUDGE: f64 = 0.05;
const OCTAVE_FOLD: f64 = 1.414;
const LEVEL_CLASH_DB: f64 = 8.0;
const MAX_BLEND_MS: i64 = 16_000;
const CLASH_MAX_MS: i64 = 4_000;
const MIN_DURATION_MS: i64 = 1_000;
const END_OVERHANG_MS: i64 = 500;
const MAX_OUTRO_SKIP_MS: i64 = 8_000;
const BEATS_PER_BAR: f64 = 4.0;
const ECHO_BEATS: f64 = 0.75;
const INTRO_SKIP_MIN_MS: i64 = 500;
const INTRO_SKIP_MAX_MS: i64 = 10_000;
const INTRO_SKIP_PREROLL_MS: i64 = 100;

/// Smart mix: the analysis of the outgoing song's end (and, when known, the incoming song's start)
/// turned into a transition. Style, first rule that applies: a fade-out is faded; matching tempos
/// get a beat-aligned Filter blend; clashing tempos, levels or keys echo out; an abrupt end cuts
/// into a strong start or rises into the next song; anything else fades. `None` when `outgoing`
/// belongs to another version of the track.
pub fn smart_mix(
    outgoing: &MixAnalysis,
    incoming: Option<&MixAnalysis>,
    track_duration_ms: i64,
    configured_duration_ms: i64,
    style: Option<Style>,
    allow_tempo_nudge: bool,
) -> Option<(TransitionPlan, SmartMixReason)> {
    if track_duration_ms <= 0
        || (outgoing.track_duration_ms - track_duration_ms).abs() > DURATION_TOLERANCE_MS
    {
        return None;
    }
    let end_of_audio_ms = outgoing.end_of_audio_ms.min(track_duration_ms);
    let latest_start_ms = track_duration_ms - CUT_END_MARGIN_MS;
    let earliest_start_ms = outgoing.region_start_ms.min(latest_start_ms);
    let outro_ms = end_of_audio_ms - outgoing.outro_start_ms;
    let fades_out = outro_ms >= FADE_OUT_MIN_MS;
    let abrupt_end = outro_ms < ABRUPT_END_MAX_MS;

    let tempo_ratio = incoming.and_then(|i| tempo_ratio(outgoing, i));
    let tempo_match = tempo_ratio.is_some_and(|r| (r - 1.0).abs() <= TEMPO_MATCH_TOLERANCE);
    let tempo_clash = tempo_ratio.is_some() && !tempo_match;
    let level_clash =
        incoming.is_some_and(|i| (outgoing.body_db - i.body_db).abs() >= LEVEL_CLASH_DB);
    let starts_strong =
        incoming.is_some_and(|i| i.intro_end_ms - i.audio_start_ms <= STRONG_START_MS);
    let harmony = incoming.and_then(|i| harmony(outgoing, i));
    let keys_match = harmony == Some(Harmony::Compatible);
    let key_clash = harmony == Some(Harmony::Clash);

    let style_reason = match () {
        _ if style.is_some() => None,
        _ if fades_out => Some(StyleReason::FadesOut),
        _ if tempo_match => Some(StyleReason::TempoMatch),
        _ if tempo_clash => Some(StyleReason::TempoClash),
        _ if key_clash => Some(StyleReason::KeyClash),
        _ if level_clash => Some(StyleReason::LevelClash),
        _ if abrupt_end && starts_strong => Some(StyleReason::AbruptEndStrongStart),
        _ if abrupt_end => Some(StyleReason::AbruptEnd),
        _ => Some(StyleReason::Default),
    };
    let chosen = style.or(style_reason.map(StyleReason::style)).unwrap_or(Style::Fade);
    let incoming_start_ms = incoming.map(intro_skip_ms);

    if chosen == Style::Cut {
        let plan = TransitionPlan {
            start_position_ms: end_of_audio_ms.clamp(earliest_start_ms, latest_start_ms),
            duration_ms: 0,
            incoming_start_ms,
            style: chosen,
            echo_delay_ms: None,
            incoming_speed: 1.0,
        };
        // Nothing overlaps, so the tempos don't meet.
        let reason = SmartMixReason {
            style: style_reason,
            tempo: None,
            harmony,
            with_incoming: incoming.is_some(),
        };
        return Some((plan, reason));
    }

    let target_ms = if tempo_clash || key_clash {
        configured_duration_ms.min(CLASH_MAX_MS)
    } else if tempo_match && keys_match {
        (configured_duration_ms * 3).min(MAX_BLEND_MS)
    } else if tempo_match || keys_match {
        (configured_duration_ms * 2).min(MAX_BLEND_MS)
    } else {
        configured_duration_ms
    };
    let beat_ms = outgoing.beat_ms();
    let mut duration_ms = if tempo_match {
        // Whole bars (or beats, for short blends) of the outgoing song.
        let bar = beat_ms * BEATS_PER_BAR;
        let unit = if target_ms as f64 >= bar { bar } else { beat_ms };
        ((target_ms as f64 / unit).floor().max(1.0) * unit).round() as i64
    } else {
        target_ms
    };

    // A fade-out is where the song is already leaving; skip at most part of it.
    let mut start_ms = if fades_out {
        outgoing.outro_start_ms.max(end_of_audio_ms - duration_ms - MAX_OUTRO_SKIP_MS)
    } else {
        end_of_audio_ms - duration_ms
    };
    let incoming_speed = if allow_tempo_nudge { nudge_speed(tempo_ratio) } else { 1.0 };

    if let (true, Some(incoming)) = (tempo_match && beat_ms > 0.0, incoming) {
        // Land the incoming first beat on a beat of the outgoing one; rounding down keeps the
        // overlap within the outgoing audio.
        let lead_in = (incoming.first_beat_ms - incoming_start_ms.unwrap_or(0)).max(0) as f64
            / incoming_speed;
        let beats = ((start_ms as f64 + lead_in - outgoing.beat_phase_ms as f64) / beat_ms).floor();
        start_ms = (outgoing.beat_phase_ms as f64 + beats * beat_ms - lead_in).round() as i64;
    }
    start_ms = start_ms.clamp(earliest_start_ms, latest_start_ms);
    // Don't keep the incoming song fading in long after the outgoing one has ended.
    duration_ms =
        duration_ms.min(end_of_audio_ms - start_ms + END_OVERHANG_MS).max(MIN_DURATION_MS);

    let plan = TransitionPlan {
        start_position_ms: start_ms,
        duration_ms,
        incoming_start_ms,
        style: chosen,
        echo_delay_ms: echo_delay_ms(outgoing),
        incoming_speed,
    };
    let tempo = if tempo_match && incoming_speed != 1.0 {
        Some(TempoFit::Matched)
    } else if tempo_match {
        Some(TempoFit::Aligned)
    } else if tempo_clash {
        Some(TempoFit::TooFar)
    } else {
        None
    };
    let reason =
        SmartMixReason { style: style_reason, tempo, harmony, with_incoming: incoming.is_some() };
    Some((plan, reason))
}

/// Outgoing over incoming tempo, folded by octaves to the nearest 1.0, or `None` unless both
/// tempos are confident.
pub fn tempo_ratio(outgoing: &MixAnalysis, incoming: &MixAnalysis) -> Option<f64> {
    if !outgoing.has_confident_tempo() || !incoming.has_confident_tempo() {
        return None;
    }
    let mut ratio = outgoing.bpm / incoming.bpm;
    while ratio > OCTAVE_FOLD {
        ratio /= 2.0;
    }
    while ratio < 1.0 / OCTAVE_FOLD {
        ratio *= 2.0;
    }
    Some(ratio)
}

/// How the songs' keys go together, or `None` unless both are confident.
pub fn harmony(outgoing: &MixAnalysis, incoming: &MixAnalysis) -> Option<Harmony> {
    let a = outgoing.musical_key.filter(MusicalKey::is_confident)?;
    let b = incoming.musical_key.filter(MusicalKey::is_confident)?;
    Some(a.harmony_with(&b))
}

/// Incoming speed that matches `ratio`, or 1 when too far apart or already matching.
pub fn nudge_speed(ratio: Option<f64>) -> f64 {
    match ratio {
        Some(r) if (MIN_TEMPO_NUDGE..=MAX_TEMPO_NUDGE).contains(&(r - 1.0).abs()) => r,
        _ => 1.0,
    }
}

/// Echo repeats synced to the outgoing tempo, when it is known.
pub fn echo_delay_ms(outgoing: &MixAnalysis) -> Option<i64> {
    outgoing.has_confident_tempo().then(|| (outgoing.beat_ms() * ECHO_BEATS).round() as i64)
}

/// Only real leading silence is skipped, never half the song.
pub fn intro_skip_ms(incoming: &MixAnalysis) -> i64 {
    let silence = incoming.audio_start_ms;
    if silence < INTRO_SKIP_MIN_MS || silence >= incoming.track_duration_ms / 2 {
        return 0;
    }
    (silence - INTRO_SKIP_PREROLL_MS).min(INTRO_SKIP_MAX_MS)
}

/// `Style` from a sync/settings name; `None` for `AUTO` and anything unknown.
pub fn style_named(name: &str) -> Option<Style> {
    Some(match name {
        "FADE" => Style::Fade,
        "FILTER" => Style::Filter,
        "ECHO" => Style::Echo,
        "RISE" => Style::Rise,
        "CUT" => Style::Cut,
        _ => return None,
    })
}

const CUSTOM_MIN_MS: i64 = 1_000;
const CUSTOM_MAX_MS: i64 = 16_000;

/// The user's transition for this pair of songs as a plan; analysis, when it matches the track,
/// fills in what is left on automatic. User positions are never moved.
pub fn custom(
    transition: &CustomTransition,
    track_duration_ms: i64,
    outgoing: Option<&MixAnalysis>,
    incoming: Option<&MixAnalysis>,
    allow_tempo_nudge: bool,
) -> Option<TransitionPlan> {
    if track_duration_ms <= CUT_END_MARGIN_MS {
        return None;
    }
    let tail = outgoing
        .filter(|a| (a.track_duration_ms - track_duration_ms).abs() <= DURATION_TOLERANCE_MS);
    let latest_start_ms = track_duration_ms - CUT_END_MARGIN_MS;
    let end_ms = transition
        .outgoing_end_ms
        .or(tail.map(|t| t.end_of_audio_ms))
        .unwrap_or(track_duration_ms)
        .clamp(0, track_duration_ms);
    let incoming_start_ms =
        transition.incoming_start_ms.map(|s| s.max(0)).or(incoming.map(intro_skip_ms));
    let style = style_named(&transition.style).unwrap_or(Style::Fade);

    if style == Style::Cut {
        return Some(TransitionPlan {
            start_position_ms: end_ms.min(latest_start_ms),
            duration_ms: 0,
            incoming_start_ms,
            style,
            echo_delay_ms: None,
            incoming_speed: 1.0,
        });
    }
    let duration_ms = transition.duration_ms.clamp(CUSTOM_MIN_MS, CUSTOM_MAX_MS);
    let start_ms = (end_ms - duration_ms).clamp(0, latest_start_ms);
    let ratio = match (transition.tempo_match && allow_tempo_nudge, tail, incoming) {
        (true, Some(t), Some(i)) => tempo_ratio(t, i),
        _ => None,
    };
    Some(TransitionPlan {
        start_position_ms: start_ms,
        duration_ms: (end_ms - start_ms).max(CUSTOM_MIN_MS),
        incoming_start_ms,
        style,
        echo_delay_ms: tail.and_then(echo_delay_ms),
        incoming_speed: nudge_speed(ratio),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- MusicalKeyTest.kt ---

    fn major(tonic: i32) -> MusicalKey {
        MusicalKey { tonic, minor: false, confidence: 1.0 }
    }
    fn minor(tonic: i32) -> MusicalKey {
        MusicalKey { tonic, minor: true, confidence: 1.0 }
    }

    #[test]
    fn keys_map_onto_the_camelot_wheel() {
        assert_eq!(major(0).camelot_code(), "8B");
        assert_eq!(minor(9).camelot_code(), "8A");
        assert_eq!(major(7).camelot_code(), "9B");
        assert_eq!(major(5).camelot_code(), "7B");
        assert_eq!(major(11).camelot_code(), "1B");
        assert_eq!(major(4).camelot_code(), "12B");
        assert_eq!(minor(0).camelot_code(), "5A");
        assert_eq!(minor(6).camelot_code(), "11A");
    }

    #[test]
    fn harmony_follows_the_camelot_rules() {
        use Harmony::*;
        // 8B against: itself, its relative minor, a fifth up and down.
        assert_eq!(major(0).harmony_with(&major(0)), Compatible);
        assert_eq!(major(0).harmony_with(&minor(9)), Compatible);
        assert_eq!(major(0).harmony_with(&major(7)), Compatible);
        assert_eq!(major(0).harmony_with(&major(5)), Compatible);
        // 12B -> 1B wraps around.
        assert_eq!(major(4).harmony_with(&major(11)), Compatible);
        // Two steps (8B -> 10B) and diagonal (8B -> 9A).
        assert_eq!(major(0).harmony_with(&major(2)), Neutral);
        assert_eq!(major(0).harmony_with(&minor(4)), Neutral);
        // A semitone, a tritone, the parallel minor.
        assert_eq!(major(0).harmony_with(&major(1)), Clash);
        assert_eq!(major(0).harmony_with(&major(6)), Clash);
        assert_eq!(major(0).harmony_with(&minor(0)), Clash);
    }

    // --- ManualSkipPolicyTest.kt ---

    #[test]
    fn manual_skip_policy() {
        use manual_skip::*;
        // The first skip blends; one while a transition runs cuts.
        assert!(should_blend(10_000, None, false));
        assert!(!should_blend(10_000, None, true));
        assert!(!should_blend(60_000, Some(10_000), true));
        // Right after another skip it cuts, after a pause it blends again.
        assert!(!should_blend(10_300, Some(10_000), false));
        assert!(!should_blend(10_000 + RAPID_SKIP_WINDOW_MS - 1, Some(10_000), false));
        assert!(should_blend(10_000 + RAPID_SKIP_WINDOW_MS, Some(10_000), false));
        // The blend is capped at a short duration.
        assert_eq!(blend_duration_ms(8_000), MAX_BLEND_MS);
        assert_eq!(blend_duration_ms(600), 600);
    }

    // --- SmartMixPlannerTest.kt ---

    const TRACK_MS: i64 = 240_000;
    const CONFIGURED_MS: i64 = 6_000;
    const C_MAJOR: MusicalKey = MusicalKey { tonic: 0, minor: false, confidence: 0.9 };
    const F_SHARP_MAJOR: MusicalKey = MusicalKey { tonic: 6, minor: false, confidence: 0.9 };
    const A_MINOR: MusicalKey = MusicalKey { tonic: 9, minor: true, confidence: 0.9 };
    const E_MINOR: MusicalKey = MusicalKey { tonic: 4, minor: true, confidence: 0.9 };

    fn tail_at(bpm: f64, outro: i64, end: i64, key: Option<MusicalKey>) -> MixAnalysis {
        MixAnalysis {
            track_duration_ms: TRACK_MS,
            region_start_ms: TRACK_MS - 30_000,
            region_end_ms: TRACK_MS,
            bpm,
            bpm_confidence: if bpm > 0.0 { 0.9 } else { 0.0 },
            beat_phase_ms: TRACK_MS - 29_870,
            first_beat_ms: TRACK_MS - 29_870,
            audio_start_ms: TRACK_MS - 30_000,
            intro_end_ms: TRACK_MS - 30_000,
            outro_start_ms: outro,
            end_of_audio_ms: end,
            loudness_db: -14.0,
            body_db: -12.0,
            peak_db: -0.5,
            musical_key: key,
        }
    }

    fn tail(bpm: f64) -> MixAnalysis {
        tail_at(bpm, TRACK_MS - 3_500, TRACK_MS - 2_500, None)
    }

    fn head_at(
        bpm: f64,
        audio_start: i64,
        first_beat: i64,
        key: Option<MusicalKey>,
    ) -> MixAnalysis {
        MixAnalysis {
            track_duration_ms: 200_000,
            region_start_ms: 0,
            region_end_ms: 30_000,
            bpm,
            bpm_confidence: if bpm > 0.0 { 0.9 } else { 0.0 },
            beat_phase_ms: first_beat,
            first_beat_ms: first_beat,
            audio_start_ms: audio_start,
            intro_end_ms: audio_start + 300,
            outro_start_ms: 30_000,
            end_of_audio_ms: 30_000,
            loudness_db: -13.0,
            body_db: -11.0,
            peak_db: -0.3,
            musical_key: key,
        }
    }

    fn head(bpm: f64) -> MixAnalysis {
        head_at(bpm, 0, 40, None)
    }

    fn keyed(mut a: MixAnalysis, key: MusicalKey) -> MixAnalysis {
        a.musical_key = Some(key);
        a
    }

    fn result(
        outgoing: &MixAnalysis,
        incoming: Option<&MixAnalysis>,
        style: Option<Style>,
        nudge: bool,
    ) -> (TransitionPlan, SmartMixReason) {
        let r = smart_mix(outgoing, incoming, TRACK_MS, CONFIGURED_MS, style, nudge).unwrap();
        assert!((outgoing.region_start_ms..=TRACK_MS).contains(&r.0.start_position_ms));
        r
    }

    fn plan(o: &MixAnalysis, i: &MixAnalysis) -> TransitionPlan {
        result(o, Some(i), None, false).0
    }

    fn reason(o: &MixAnalysis, i: Option<&MixAnalysis>) -> SmartMixReason {
        result(o, i, None, false).1
    }

    #[test]
    fn matching_tempos_blend_over_whole_bars_with_the_beats_aligned() {
        let outgoing = tail(124.0);
        let incoming = head_at(122.0, 1_200, 1_350, None);
        let (plan, _) = result(&outgoing, Some(&incoming), None, true);

        assert_eq!(plan.style, Style::Filter);
        let bars = plan.duration_ms as f64 / (4.0 * 60_000.0 / 124.0);
        assert!((bars.round() - bars).abs() < 0.01);
        assert!(plan.duration_ms <= 2 * CONFIGURED_MS);
        assert!((plan.incoming_speed - 124.0 / 122.0).abs() < 1e-4);
        assert_eq!(plan.incoming_start_ms, Some(1_100));
        // The incoming first beat, after its lead-in at the nudged speed, sits on an outgoing beat.
        let at = plan.start_position_ms as f64 + (1_350.0 - 1_100.0) / plan.incoming_speed;
        let beat = outgoing.beat_ms();
        let offset = ((at - outgoing.beat_phase_ms as f64) % beat + beat) % beat;
        assert!(offset.min(beat - offset) < 2.0, "beat offset {offset}");
        assert!(plan.start_position_ms + plan.duration_ms <= outgoing.end_of_audio_ms);
    }

    #[test]
    fn no_tempo_nudge_unless_allowed() {
        assert_eq!(plan(&tail(124.0), &head(122.0)).incoming_speed, 1.0);
    }

    #[test]
    fn clashing_tempos_echo_out_quickly_in_time_with_the_outgoing_beat() {
        let plan = plan(&tail(128.0), &head(96.0));
        assert_eq!(plan.style, Style::Echo);
        assert!(plan.duration_ms <= 4_000);
        assert_eq!(plan.echo_delay_ms, Some((60_000.0_f64 / 128.0 * 0.75).round() as i64));
        assert_eq!(plan.incoming_speed, 1.0);
    }

    #[test]
    fn a_fade_out_is_faded_from_its_start() {
        let outgoing = tail_at(0.0, TRACK_MS - 14_000, TRACK_MS - 2_000, None);
        let plan = plan(&outgoing, &head(0.0));
        assert_eq!(plan.style, Style::Fade);
        assert_eq!(plan.start_position_ms, TRACK_MS - 14_000);
        assert_eq!(plan.duration_ms, CONFIGURED_MS);
    }

    #[test]
    fn an_abrupt_end_into_a_strong_start_cuts_where_the_audio_ends() {
        let outgoing = tail_at(0.0, TRACK_MS - 1_200, TRACK_MS - 900, None);
        let plan = plan(&outgoing, &head(0.0));
        assert_eq!(plan.style, Style::Cut);
        assert_eq!(plan.start_position_ms, TRACK_MS - 900);
        assert_eq!(plan.duration_ms, 0);
    }

    #[test]
    fn the_users_style_is_kept() {
        let (plan, _) = result(&tail(128.0), Some(&head(96.0)), Some(Style::Rise), false);
        assert_eq!(plan.style, Style::Rise);
    }

    #[test]
    fn an_analysis_of_another_track_length_is_ignored() {
        let mut outgoing = tail(120.0);
        outgoing.track_duration_ms = TRACK_MS - 10_000;
        assert!(smart_mix(&outgoing, None, TRACK_MS, CONFIGURED_MS, None, false).is_none());
    }

    #[test]
    fn half_time_counts_as_the_same_tempo() {
        let ratio = tempo_ratio(&tail(140.0), &head(71.0)).unwrap();
        assert!((ratio - 140.0 / 142.0).abs() < 1e-6);
    }

    #[test]
    fn matching_tempos_in_compatible_keys_blend_longest() {
        let without = plan(&tail(124.0), &head(122.0));
        let with = plan(&keyed(tail(124.0), A_MINOR), &keyed(head(122.0), E_MINOR));
        assert_eq!(with.style, Style::Filter);
        assert!(with.duration_ms > without.duration_ms);
        assert!(with.duration_ms > 2 * CONFIGURED_MS);
        assert!(with.duration_ms <= 16_000);
    }

    #[test]
    fn compatible_keys_blend_longer_without_a_tempo() {
        let outgoing = tail_at(0.0, TRACK_MS - 4_500, TRACK_MS - 2_500, Some(A_MINOR));
        let plan = plan(&outgoing, &keyed(head(0.0), C_MAJOR));
        assert_eq!(plan.style, Style::Fade);
        assert_eq!(plan.duration_ms, 2 * CONFIGURED_MS);
    }

    #[test]
    fn clashing_keys_keep_a_matching_tempo_blend_short() {
        let plan = plan(&keyed(tail(124.0), C_MAJOR), &keyed(head(122.0), F_SHARP_MAJOR));
        assert_eq!(plan.style, Style::Filter);
        assert!(plan.duration_ms <= 4_000);
    }

    #[test]
    fn clashing_keys_echo_out_quickly() {
        let outgoing = tail_at(0.0, TRACK_MS - 4_500, TRACK_MS - 2_500, Some(C_MAJOR));
        let plan = plan(&outgoing, &keyed(head(0.0), F_SHARP_MAJOR));
        assert_eq!(plan.style, Style::Echo);
        assert!(plan.duration_ms <= 4_000);
    }

    #[test]
    fn keys_are_ignored_unless_both_are_confident() {
        let unsure = MusicalKey { confidence: 0.2, ..F_SHARP_MAJOR };
        let outgoing = tail_at(0.0, TRACK_MS - 4_500, TRACK_MS - 2_500, Some(C_MAJOR));
        assert_eq!(harmony(&keyed(tail(0.0), C_MAJOR), &keyed(head(0.0), unsure)), None);
        let plan = plan(&outgoing, &keyed(head(0.0), unsure));
        assert_eq!(plan.style, Style::Fade);
        assert_eq!(plan.duration_ms, CONFIGURED_MS);
    }

    #[test]
    fn the_reason_says_how_the_tempos_fit() {
        let nudged = result(&tail(124.0), Some(&head(122.0)), None, true).1;
        let aligned = reason(&tail(124.0), Some(&head(122.0)));
        let clash = reason(&tail(128.0), Some(&head(96.0)));
        assert_eq!(nudged.style, Some(StyleReason::TempoMatch));
        assert_eq!(nudged.tempo, Some(TempoFit::Matched));
        assert_eq!(aligned.tempo, Some(TempoFit::Aligned));
        assert_eq!(clash.style, Some(StyleReason::TempoClash));
        assert_eq!(clash.tempo, Some(TempoFit::TooFar));
        assert_eq!(reason(&tail(124.0), Some(&head(0.0))).tempo, None);
    }

    #[test]
    fn the_reason_names_the_rule_that_picked_the_style() {
        let fade_out = tail_at(0.0, TRACK_MS - 14_000, TRACK_MS - 2_000, None);
        let steady = tail_at(0.0, TRACK_MS - 4_500, TRACK_MS - 2_500, None);
        let mut slow_intro = head(0.0);
        slow_intro.intro_end_ms = 5_000;
        let mut quiet = head(0.0);
        quiet.body_db = -25.0;
        assert_eq!(reason(&fade_out, Some(&head(0.0))).style, Some(StyleReason::FadesOut));
        assert_eq!(
            reason(&tail(0.0), Some(&head(0.0))).style,
            Some(StyleReason::AbruptEndStrongStart)
        );
        assert_eq!(reason(&tail(0.0), Some(&slow_intro)).style, Some(StyleReason::AbruptEnd));
        assert_eq!(reason(&steady, Some(&quiet)).style, Some(StyleReason::LevelClash));
        assert_eq!(reason(&steady, Some(&head(0.0))).style, Some(StyleReason::Default));
        assert_eq!(StyleReason::Default.style(), Style::Fade);
    }

    #[test]
    fn the_reason_carries_the_keys() {
        let outgoing = tail_at(0.0, TRACK_MS - 4_500, TRACK_MS - 2_500, Some(C_MAJOR));
        let clash = reason(&outgoing, Some(&keyed(head(0.0), F_SHARP_MAJOR)));
        let matched = reason(&keyed(tail(124.0), A_MINOR), Some(&keyed(head(122.0), E_MINOR)));
        assert_eq!(clash.style, Some(StyleReason::KeyClash));
        assert_eq!(clash.harmony, Some(Harmony::Clash));
        assert_eq!(matched.harmony, Some(Harmony::Compatible));
        assert_eq!(reason(&tail(124.0), Some(&head(122.0))).harmony, None);
    }

    #[test]
    fn the_reason_tells_a_fallback_without_the_next_song_and_a_style_the_user_chose() {
        let fallback = reason(&tail(124.0), None);
        let chosen = result(&tail(128.0), Some(&head(96.0)), Some(Style::Rise), false).1;
        assert!(!fallback.with_incoming);
        assert_eq!(fallback.tempo, None);
        assert!(chosen.with_incoming);
        assert_eq!(chosen.style, None);
        assert_eq!(chosen.tempo, Some(TempoFit::TooFar));
    }

    // --- CustomTransitionPlannerTest.kt ---

    const END_OF_AUDIO_MS: i64 = 236_000;

    fn custom_tail(bpm: f64) -> MixAnalysis {
        tail_at(bpm, END_OF_AUDIO_MS - 500, END_OF_AUDIO_MS, None)
    }

    fn custom_head(bpm: f64, audio_start: i64) -> MixAnalysis {
        head_at(bpm, audio_start, audio_start + 40, None)
    }

    fn transition(style: &str, duration_ms: i64) -> CustomTransition {
        CustomTransition {
            from_id: "a".into(),
            to_id: "b".into(),
            style: style.into(),
            duration_ms,
            outgoing_end_ms: None,
            incoming_start_ms: None,
            tempo_match: true,
            updated_at: 0,
        }
    }

    #[test]
    fn pinned_positions_are_used_as_set() {
        let t = CustomTransition {
            outgoing_end_ms: Some(200_000),
            incoming_start_ms: Some(12_000),
            ..transition("FILTER", 6_000)
        };
        let tail = custom_tail(124.0);
        let head = custom_head(122.0, 0);
        let plan = custom(&t, TRACK_MS, Some(&tail), Some(&head), true).unwrap();
        assert_eq!(plan.start_position_ms, 194_000);
        assert_eq!(plan.duration_ms, 6_000);
        assert_eq!(plan.incoming_start_ms, Some(12_000));
        assert_eq!(plan.style, Style::Filter);
    }

    #[test]
    fn automatic_positions_come_from_the_analysis_or_the_track_end_without_it() {
        let t = transition("FADE", 4_000);
        let tail = custom_tail(124.0);
        let analysed =
            custom(&t, TRACK_MS, Some(&tail), Some(&custom_head(122.0, 2_000)), true).unwrap();
        assert_eq!(analysed.start_position_ms, END_OF_AUDIO_MS - 4_000);
        assert_eq!(analysed.incoming_start_ms, Some(1_900));

        let plain = custom(&t, TRACK_MS, None, None, true).unwrap();
        assert_eq!(plain.start_position_ms, TRACK_MS - 4_000);
        assert_eq!(plain.duration_ms, 4_000);
        assert_eq!(plain.incoming_start_ms, None);
        assert_eq!(plain.incoming_speed, 1.0);

        // Analysis of another version of the song is ignored.
        let stale = custom(&t, TRACK_MS + 10_000, Some(&tail), None, true).unwrap();
        assert_eq!(stale.start_position_ms, TRACK_MS + 6_000);
    }

    #[test]
    fn tempo_is_matched_only_when_asked_for_and_allowed() {
        let t = transition("ECHO", 8_000);
        let (tail, head) = (custom_tail(124.0), custom_head(121.0, 0));
        let matched = custom(&t, TRACK_MS, Some(&tail), Some(&head), true).unwrap();
        assert!((matched.incoming_speed - 124.0 / 121.0).abs() < 1e-4);
        assert_eq!(matched.echo_delay_ms, Some((60_000.0_f64 / 124.0 * 0.75).round() as i64));

        let off = CustomTransition { tempo_match: false, ..t.clone() };
        assert_eq!(
            custom(&off, TRACK_MS, Some(&tail), Some(&head), true).unwrap().incoming_speed,
            1.0
        );
        assert_eq!(
            custom(&t, TRACK_MS, Some(&tail), Some(&head), false).unwrap().incoming_speed,
            1.0
        );
        let far = custom_head(100.0, 0);
        assert_eq!(
            custom(&t, TRACK_MS, Some(&tail), Some(&far), true).unwrap().incoming_speed,
            1.0
        );
    }

    #[test]
    fn cut_switches_at_the_end_point_and_early_ends_shorten_the_overlap() {
        let cut = CustomTransition { outgoing_end_ms: Some(150_000), ..transition("CUT", 5_000) };
        let plan = custom(&cut, TRACK_MS, None, None, true).unwrap();
        assert_eq!((plan.start_position_ms, plan.duration_ms), (150_000, 0));

        let auto = CustomTransition { outgoing_end_ms: None, ..cut };
        assert_eq!(
            custom(&auto, TRACK_MS, None, None, true).unwrap().start_position_ms,
            TRACK_MS - 150
        );

        let early = CustomTransition { outgoing_end_ms: Some(3_000), ..transition("FADE", 8_000) };
        let plan = custom(&early, TRACK_MS, None, None, true).unwrap();
        assert_eq!((plan.start_position_ms, plan.duration_ms), (0, 3_000));
    }
}
