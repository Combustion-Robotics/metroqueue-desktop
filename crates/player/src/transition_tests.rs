//! Real libmpv regressions: fixtures end in signal, not the silence used by the tail tests.
use super::*;
use std::{collections::HashMap, path::Path, time::Instant};

const RATE: usize = 48_000;

fn wait_for(label: &str, timeout: f64, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs_f64(timeout);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out: {label}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wav(path: &Path, seconds: f64, signal: impl Fn(f64) -> f64) {
    let frames = (seconds * RATE as f64).round() as u32;
    let bytes = frames * 4;
    let mut data = Vec::with_capacity(bytes as usize + 44);
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(bytes + 36).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x02\0");
    data.extend_from_slice(&(RATE as u32).to_le_bytes());
    data.extend_from_slice(&(RATE as u32 * 4).to_le_bytes());
    data.extend_from_slice(b"\x04\0\x10\0data");
    data.extend_from_slice(&bytes.to_le_bytes());
    for frame in 0..frames {
        let value = (signal(frame as f64 / RATE as f64) * 8000.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
        data.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, data).unwrap();
}

fn tone(hz: f64, t: f64) -> f64 {
    (std::f64::consts::TAU * hz * t).sin()
}

fn effects() -> Reverb {
    Reverb { enabled: true, preset: 3, decay: REVERB_DECAYS[2], wet: 25, ..Reverb::default() }
}

fn pcm(mpv: &Mpv, output: &Path) {
    mpv.set_property("ao", "pcm").unwrap();
    mpv.set_property("ao-pcm-file", output.to_str().unwrap()).unwrap();
    mpv.set_property("ao-pcm-waveheader", false).unwrap();
    mpv.set_property("audio-format", "s16").unwrap();
    mpv.set_property("audio-samplerate", RATE as i64).unwrap();
    mpv.set_property("audio-channels", "stereo").unwrap();
    mpv.set_property("pause", true).unwrap();
}

fn samples(path: &Path) -> Vec<f64> {
    std::fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|s| i16::from_le_bytes([s[0], s[1]]) as f64 / 32768.0)
        .collect()
}

fn render(dir: &Path, input: &Path, speed: f64, pitch: i32, room: Reverb) -> Vec<f64> {
    let p = Player::new(dir.to_str().unwrap()).unwrap();
    let output = dir.join("render.pcm");
    pcm(p.mpv(), &output);
    p.mpv().set_property("keep-open", true).unwrap();
    p.set_playback_params(speed, pitch, room).unwrap();
    p.load(input.to_str().unwrap(), &HashMap::new(), None, None).unwrap();
    p.play().unwrap();
    wait_for("render EOF", 10.0, || p.mpv().get_property::<bool>("eof-reached").unwrap_or(false));
    p.stop().unwrap();
    wait_for("render unload", 2.0, || p.mpv().get_property::<String>("path").is_err());
    samples(&output)
}

// Energy in a narrow band. Hann windows reject the other tracks' tones; 75% overlap keeps a
// short note's energy independent of its alignment with the analysis windows.
fn band_energy(samples: &[f64], hz: f64) -> f64 {
    let kernel: Vec<_> = (0..480)
        .map(|i| {
            let weight = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / 480.0).cos();
            let phase = std::f64::consts::TAU * hz * i as f64 / RATE as f64;
            (weight * phase.cos(), weight * phase.sin())
        })
        .collect();
    samples
        .windows(480)
        .step_by(120)
        .map(|window| {
            let (mut re, mut im) = (0.0, 0.0);
            for (value, (cos, sin)) in window.iter().zip(&kernel) {
                re += value * cos;
                im += value * sin;
            }
            (re * re + im * im) / 480.0
        })
        .sum()
}

#[test]
fn reverb_and_speed_adapter_preserve_rubberband_sample_count() {
    let dir = tempfile::tempdir().unwrap();
    let source = "av://lavfi:anoisesrc=color=pink:amplitude=0.05:seed=42:d=0.4,apad=pad_dur=4";
    for speed in [0.5, 0.8, 1.2, 2.0] {
        let mut lengths = [0; 2];
        for (index, room) in [false, true].into_iter().enumerate() {
            let p = Player::new(dir.path().to_str().unwrap()).unwrap();
            let output = dir.path().join(format!("duration-{speed}-{room}.pcm"));
            pcm(p.mpv(), &output);
            p.mpv().set_property("gapless-audio", "no").unwrap();
            p.mpv().set_property("keep-open", true).unwrap();
            if room {
                p.set_playback_params(speed, 0, effects()).unwrap();
            } else {
                // Run exactly the same FFmpeg stretcher alone, at mpv speed 1 so its
                // automatic pitch correction cannot stretch the reference a second time.
                let chain = format!("lavfi=[{}anull]", lavfi_stretch(speed, 0));
                set_filter_chain(p.mpv(), &chain).unwrap();
            }
            // Player::load reapplies the session filters; load the reference directly.
            p.mpv().command("loadfile", &[&quoted(source), "replace"]).unwrap();
            p.play().unwrap();
            wait_for("duration render EOF", 5.0, || {
                p.mpv().get_property::<bool>("eof-reached").unwrap_or(false)
            });
            p.stop().unwrap();
            wait_for("duration render unload", 2.0, || {
                p.mpv().get_property::<String>("path").is_err()
            });
            lengths[index] = samples(&output).len();
        }
        let [bare, room] = lengths;
        eprintln!("tempo {speed}: Rubber Band={bare} frames, reverb/adapter={room} frames");
        assert!(
            bare.abs_diff(room) <= 1,
            "reverb or the speed adapter added/lost samples at {speed}x: {bare} -> {room}"
        );
    }
}

#[test]
fn final_note_survives_unpadded_eof() {
    let dir = tempfile::tempdir().unwrap();
    let short = dir.path().join("eof.wav");
    let padded = dir.path().join("reference.wav");
    for note_length in [0.02, 0.08] {
        let signal = |t: f64| {
            if t < 0.1 {
                tone(440.0, t)
            } else if ((2.0 - note_length)..2.0).contains(&t) {
                tone(1760.0, t)
            } else {
                0.0
            }
        };
        wav(&short, 2.0, signal);
        wav(&padded, 3.0, signal);
        for speed in [0.5, 0.8, 1.0, 1.2, 2.0] {
            for room in [Reverb::default(), effects()] {
                let pitch = 3;
                let actual = render(dir.path(), &short, speed, pitch, room);
                let reference = render(dir.path(), &padded, speed, pitch, room);
                let frequency = 1760.0 * 2f64.powf(pitch as f64 / 12.0);
                let energy = band_energy(&actual, frequency);
                // Compare only the common output interval: a padded file can emit a longer reverb
                // tail, which is a different question from losing the note before EOF.
                let expected =
                    band_energy(&reference[..actual.len().min(reference.len())], frequency);
                let ratio = energy / expected;
                let seconds = actual.len() as f64 / RATE as f64;
                eprintln!("EOF note={note_length}s, speed={speed}, reverb={}: duration={seconds:.4}, final note={ratio:.3}", room.enabled);
                assert!(expected > 0.01, "reference marker is inaudible");
                assert!(
                    (0.9..1.1).contains(&ratio),
                    "lost/distorted EOF marker at {speed}x: {ratio}"
                );
                assert!(
                    (seconds * speed - 2.0).abs() < 0.12,
                    "truncated or duplicated audio: {seconds}s at {speed}x"
                );
            }
        }
    }
}

#[test]
fn consecutive_gapless_tracks_keep_their_boundary_notes() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..3).map(|i| dir.path().join(format!("track-{i}.wav"))).collect();
    let frequencies = [400.0, 800.0, 1200.0];
    let signal = |i: usize, t: f64| {
        let hz = if t < 0.04 {
            600.0 + 400.0 * i as f64
        } else if t >= 0.96 {
            1800.0 + 800.0 * i as f64
        } else {
            frequencies[i]
        };
        tone(hz, t)
    };
    for (i, path) in paths.iter().enumerate() {
        wav(path, 1.0, |t| signal(i, t));
    }
    for speed in [0.8, 1.0, 1.2] {
        let mut p = Player::new(dir.path().to_str().unwrap()).unwrap();
        let mut events = p.take_events().unwrap();
        let output = dir.path().join("gapless.pcm");
        pcm(p.mpv(), &output);
        p.set_playback_params(speed, 3, effects()).unwrap();
        p.load(paths[0].to_str().unwrap(), &HashMap::new(), None, None).unwrap();
        for path in &paths[1..] {
            p.enqueue(path.to_str().unwrap()).unwrap();
        }
        let mut ended = 0;
        p.play().unwrap();
        wait_for("three gapless EOF events", 10.0, || {
            while let Ok(event) = events.try_recv() {
                match event {
                    PlayerEvent::TrackEnded => ended += 1,
                    PlayerEvent::Error(error)
                    | PlayerEvent::TrackFailed(error)
                    | PlayerEvent::LookaheadFailed(error) => panic!("gapless playback: {error}"),
                    PlayerEvent::AudioDeviceLost => panic!("gapless audio output lost"),
                    _ => {}
                }
            }
            ended >= 3
        });
        assert_eq!(ended, 3, "one queue advance per track");
        p.stop().unwrap();
        wait_for("gapless unload", 2.0, || p.mpv().get_property::<String>("path").is_err());
        let audio = samples(&output);
        let duration = audio.len() as f64 / RATE as f64;
        eprintln!("gapless {speed}x: {duration:.4}s");
        assert!((duration * speed - 3.0).abs() < 0.15, "gapless duration: {duration}");
        for (i, hz) in frequencies.iter().enumerate() {
            let start = ((i as f64 + 0.15) / speed * RATE as f64) as usize;
            let end = ((i as f64 + 0.85) / speed * RATE as f64) as usize;
            assert!(
                band_energy(&audio[start..end], hz * 2f64.powf(3.0 / 12.0)) > 0.1,
                "missing track {i}"
            );
        }
        // Compare against separately rendered tracks. A continuous source has different Rubber
        // Band history at each join; it cannot be a sample/energy reference for decoder startup.
        for (i, path) in paths.iter().enumerate() {
            let reference = render(dir.path(), path, speed, 3, effects());
            for marker in [600.0 + 400.0 * i as f64, 1800.0 + 800.0 * i as f64] {
                let hz = marker * 2f64.powf(3.0 / 12.0);
                let expected = band_energy(&reference, hz);
                let ratio = band_energy(&audio, hz) / expected;
                eprintln!("gapless {speed}x, marker {marker}: energy ratio={ratio:.3}");
                assert!(
                    expected > 0.01 && (0.9..1.1).contains(&ratio),
                    "missing/distorted boundary marker {marker} on track {i}: {ratio}"
                );
            }
        }
        // No artificial silence at either join. Use short RMS windows rather than individual
        // samples (sine zero crossings are expected); tolerate at most 30 ms of filter startup.
        for boundary in [1.0, 2.0] {
            let center = (boundary / speed * RATE as f64) as usize;
            let mut silent = 0;
            for chunk in audio[center - 9600..(center + 9600).min(audio.len())].chunks(240) {
                let rms = (chunk.iter().map(|v| v * v).sum::<f64>() / chunk.len() as f64).sqrt();
                silent = if rms < 0.002 { silent + 1 } else { 0 };
                assert!(silent <= 6, "gap at boundary {boundary}, speed {speed}");
            }
        }
    }
}

#[test]
fn automatic_crossfade_runs_both_effected_decks_in_real_time() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("outgoing.wav");
    let second = dir.path().join("incoming.wav");
    wav(&first, 5.0, |t| tone(440.0, t));
    wav(&second, 5.0, |t| tone(880.0, t));
    for speed in [0.8, 1.0, 1.2] {
        let mut p = Player::new(dir.path().to_str().unwrap()).unwrap();
        let mut events = p.take_events().unwrap();
        let incoming = p.idle_mpv().unwrap();
        for m in [p.mpv(), incoming.as_ref()] {
            m.set_property("ao", "null").unwrap();
            m.set_property("ao-null-untimed", false).unwrap();
            m.set_property("pause", true).unwrap();
        }
        p.set_volume(100).unwrap();
        p.set_playback_params(speed, 3, effects()).unwrap();
        p.set_crossfade(Some(1.5));
        p.load(first.to_str().unwrap(), &HashMap::new(), None, None).unwrap();
        p.enqueue(second.to_str().unwrap()).unwrap();
        wait_for("incoming preload", 5.0, || p.decks.preloaded.load(Ordering::Acquire));
        p.play().unwrap();
        wait_for("natural crossfade start", 9.0, || p.decks.fading.load(Ordering::Acquire));
        let started = Instant::now();
        let out = p.decks.mpv(0).unwrap();
        let remaining = (5.0 - out.get_property::<f64>("time-pos").unwrap()) / speed;
        eprintln!("crossfade {speed}x: remaining listening seconds {remaining:.3}");
        assert!((1.1..1.7).contains(&remaining), "fade scheduled in source time: {remaining}");
        assert_eq!(p.decks.active.load(Ordering::Acquire), 1);
        let mut overlap_samples = 0;
        let mut first_positions = None;
        let mut last_positions = None;
        while p.decks.fading.load(Ordering::Acquire) {
            assert!(started.elapsed().as_secs_f64() < 2.0, "fade did not finish");
            let a = out.get_property::<f64>("volume").unwrap();
            let b = incoming.get_property::<f64>("volume").unwrap();
            let outputs_ready = [out.as_ref(), incoming.as_ref()]
                .iter()
                .all(|m| m.get_property::<MpvNode>("audio-out-params").is_ok());
            let positions = out
                .get_property::<f64>("time-pos")
                .ok()
                .zip(incoming.get_property::<f64>("time-pos").ok());
            // At the final ramp step the outgoing deck is asynchronously unloaded. Only count
            // samples where both decoders still report an output and a position.
            if let Some(positions) = positions.filter(|_| a > 10.0 && b > 10.0 && outputs_ready) {
                let power = (a / 100.0).powi(6) + (b / 100.0).powi(6);
                assert!((power - 1.0).abs() < 0.12, "unequal fade power: {power}");
                first_positions.get_or_insert(positions);
                last_positions = Some(positions);
                overlap_samples += 1;
                for m in [out.as_ref(), incoming.as_ref()] {
                    assert!(!m.get_property::<bool>("pause").unwrap());
                    assert!(
                        !m.get_property::<bool>("eof-reached").unwrap_or(false),
                        "deck ended during overlap"
                    );
                    let chain = m.get_property::<String>("af").unwrap();
                    assert!(
                        chain.contains("afir") && chain.contains("rubberband"),
                        "effects dropped during fade: {chain}"
                    );
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(overlap_samples >= 20, "no sustained actual overlap");
        let (first_a, first_b) = first_positions.unwrap();
        let (last_a, last_b) = last_positions.unwrap();
        assert!(
            last_a - first_a > 0.3 * speed && last_b - first_b > 0.3 * speed,
            "both decoders must advance while their outputs overlap"
        );
        assert!(started.elapsed().as_secs_f64() > 1.0, "fade ended too early");
        wait_for("outgoing deck unloaded", 2.0, || out.get_property::<String>("path").is_err());
        assert!((incoming.get_property::<f64>("volume").unwrap() - 100.0).abs() < 0.01);
        let mut advances = 0;
        while let Ok(event) = events.try_recv() {
            match event {
                PlayerEvent::TrackEnded => advances += 1,
                PlayerEvent::Error(error)
                | PlayerEvent::TrackFailed(error)
                | PlayerEvent::LookaheadFailed(error) => panic!("crossfade playback: {error}"),
                PlayerEvent::AudioDeviceLost => panic!("crossfade audio output lost"),
                _ => {}
            }
        }
        assert_eq!(advances, 1, "queue must advance once at fade start");
        p.stop().unwrap();
    }
}

#[test]
fn analysis_decode_is_mono_resampled_and_faster_than_real_time() {
    let dir = tempfile::tempdir().unwrap();
    let track = dir.path().join("track.wav");
    wav(&track, 20.0, |t| tone(440.0, t));
    let p = Player::new(dir.path().to_str().unwrap()).unwrap();
    let url = track.to_str().unwrap();
    let started = Instant::now();
    let tail = p.decode(url, true, 6.0, 11_025).unwrap();
    let head = p.decode(url, false, 4.0, 11_025).unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!("decoded 10 s in {elapsed:.3} s");
    assert!(elapsed < 5.0, "decoding ran in real time: {elapsed}");
    assert_eq!(tail.rate, 11_025);
    assert!((tail.duration - 20.0).abs() < 0.05, "duration {}", tail.duration);
    let secs = |pcm: &Pcm| pcm.samples.len() as f64 / pcm.rate as f64;
    assert!((secs(&tail) - 6.0).abs() < 0.1, "tail {}", secs(&tail));
    assert!((tail.start - 14.0).abs() < 0.1, "tail start {}", tail.start);
    assert!((secs(&head) - 4.0).abs() < 0.1, "head {}", secs(&head));
    assert_eq!(head.start, 0.0);
    // 8000/32768 peak per channel; the downmix weighs each of the two by 1/√2.
    let peak = head.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    assert!((peak - 0.345).abs() < 0.02, "peak {peak}");
    assert!(p
        .decode(&dir.path().join("missing.wav").to_string_lossy(), false, 4.0, 11_025)
        .is_err());
    assert!(std::fs::read_dir(dir.path())
        .unwrap()
        .all(|e| { !e.unwrap().file_name().to_string_lossy().starts_with("mix-") }));
}

#[test]
fn transition_effects_are_accepted_and_retuned_by_mpv() {
    let dir = tempfile::tempdir().unwrap();
    let p = Player::new(dir.path().to_str().unwrap()).unwrap();
    p.mpv().set_property("ao", "null").unwrap();
    p.set_gain(Some(-3.0)).unwrap();
    p.load("av://lavfi:sine=f=440:d=30", &HashMap::new(), Some(-3.0), None).unwrap();
    p.play().unwrap();
    wait_for("playback", 5.0, || p.mpv().get_property::<MpvNode>("audio-out-params").is_ok());
    for style in [Style::Filter, Style::Echo, Style::Rise] {
        for outgoing in [true, false] {
            let fx = Fx { style, outgoing, echo_ms: 375 };
            if fx_graph(fx).is_none() {
                assert!(fx_commands(style, outgoing, 0.5).is_empty());
                continue;
            }
            set_fx(&p.decks, 0, Some(fx));
            let chain = p.mpv().get_property::<String>("af").unwrap();
            assert!(chain.contains("mixfx") && chain.contains("volume=-3dB"), "{chain}");
            // The graph is built on the next audio frame; commands only reach it from then on.
            let (target, cmd, value) = fx_commands(style, outgoing, 0.0)[0];
            wait_for("graph built", 2.0, || {
                p.mpv().command("af-command", &[FX_LABEL, cmd, &value.to_string(), target]).is_ok()
            });
            for progress in [0.0, 0.5, 1.0] {
                for (target, cmd, value) in fx_commands(style, outgoing, progress) {
                    p.mpv()
                        .command("af-command", &[FX_LABEL, cmd, &value.to_string(), target])
                        .unwrap_or_else(|e| panic!("{style:?} {cmd}={value}: {e}"));
                }
            }
            set_fx(&p.decks, 0, None);
            let chain = p.mpv().get_property::<String>("af").unwrap();
            assert!(!chain.contains("mixfx") && chain.contains("volume=-3dB"), "{chain}");
        }
    }
    p.stop().unwrap();
}

#[test]
fn effect_envelopes_end_where_they_hand_back() {
    let f = |style, outgoing, progress| fx_commands(style, outgoing, progress)[0].2;
    // Outgoing filters close in, incoming ones open up to (nearly) nothing.
    assert!(f(Style::Filter, true, 0.0) < f(Style::Filter, true, 1.0));
    assert!((f(Style::Filter, false, 0.9) - 20_000.0).abs() < 1.0);
    assert!((f(Style::Rise, false, 0.5) - 250.0).abs() < 1e-9, "bass held back until the drop");
    assert!((f(Style::Rise, false, 0.85) - 20.0).abs() < 1e-9);
    assert!((fx_commands(Style::Rise, true, 1.0)[1].2 - 1.2589).abs() < 1e-9);
    // The echo's dry signal is untouched at first and gone by 60%.
    assert_eq!(f(Style::Echo, true, 0.2), 1.0);
    assert_eq!(f(Style::Echo, true, 0.6), 0.0);
    assert!(fx_commands(Style::Fade, true, 0.5).is_empty());
}

#[test]
fn planned_transition_starts_at_its_point_and_enters_the_next_track_where_asked() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("outgoing.wav");
    let second = dir.path().join("incoming.wav");
    wav(&first, 6.0, |t| tone(440.0, t));
    wav(&second, 6.0, |t| tone(880.0, t));
    let mut p = Player::new(dir.path().to_str().unwrap()).unwrap();
    let mut events = p.take_events().unwrap();
    let incoming = p.idle_mpv().unwrap();
    for m in [p.mpv(), incoming.as_ref()] {
        m.set_property("ao", "null").unwrap();
        m.set_property("ao-null-untimed", false).unwrap();
    }
    p.set_crossfade(Some(5.0));
    p.load(first.to_str().unwrap(), &HashMap::new(), None, None).unwrap();
    p.enqueue(second.to_str().unwrap()).unwrap();
    p.set_transition(Some(Transition {
        start: Some(2.0),
        fade: 1.0,
        incoming_start: 1.5,
        style: Style::Filter,
        incoming_speed: 1.03,
        ..Transition::default()
    }));
    wait_for("incoming preload", 5.0, || p.decks.preloaded.load(Ordering::Acquire));
    p.play().unwrap();
    wait_for("planned start", 5.0, || p.decks.fading.load(Ordering::Acquire));
    let out = p.decks.mpv(0).unwrap();
    let at = out.get_property::<f64>("time-pos").unwrap();
    assert!((2.0..2.4).contains(&at), "started at {at}");
    wait_for("effects on", 1.0, || {
        incoming.get_property::<String>("af").unwrap().contains("mixfx")
    });
    assert!(out.get_property::<String>("af").unwrap().contains("highpass"));
    let entered = incoming.get_property::<f64>("time-pos").unwrap();
    // mpv lands a seek on the frame just before the target, e.g. 1.499985.
    assert!((1.49..2.0).contains(&entered), "incoming at {entered}");
    assert_eq!(incoming.get_property::<f64>("speed").unwrap(), 1.03);
    wait_for("fade end", 3.0, || !p.decks.fading.load(Ordering::Acquire));
    assert!(!incoming.get_property::<String>("af").unwrap().contains("mixfx"));
    assert_eq!(incoming.get_property::<f64>("speed").unwrap(), 1.03, "eased back at once");
    let ended = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|e| matches!(e, PlayerEvent::TrackEnded))
        .count();
    assert_eq!(ended, 1);
    // A skip back to a track the user picked plays at the session tempo.
    p.load(first.to_str().unwrap(), &HashMap::new(), None, None).unwrap();
    assert_eq!(p.mpv().get_property::<f64>("speed").unwrap(), 1.0);
    p.stop().unwrap();
}

#[test]
fn manual_skip_blends_only_into_a_ready_preload() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("a.wav");
    let second = dir.path().join("b.wav");
    wav(&first, 10.0, |t| tone(440.0, t));
    wav(&second, 10.0, |t| tone(880.0, t));
    let mut p = Player::new(dir.path().to_str().unwrap()).unwrap();
    let mut events = p.take_events().unwrap();
    let incoming = p.idle_mpv().unwrap();
    for m in [p.mpv(), incoming.as_ref()] {
        m.set_property("ao", "null").unwrap();
        m.set_property("ao-null-untimed", false).unwrap();
    }
    p.set_crossfade(Some(5.0));
    p.load(first.to_str().unwrap(), &HashMap::new(), None, None).unwrap();
    assert!(!p.skip_crossfade(0.5), "nothing preloaded");
    p.enqueue(second.to_str().unwrap()).unwrap();
    wait_for("incoming preload", 5.0, || p.decks.preloaded.load(Ordering::Acquire));
    p.pause().unwrap();
    assert!(!p.skip_crossfade(0.5), "paused");
    p.play().unwrap();
    wait_for("playing", 2.0, || p.mpv().get_property::<f64>("time-pos").unwrap_or(0.0) > 0.2);
    assert!(p.skip_crossfade(0.5));
    assert!(!p.skip_crossfade(0.5), "a fade is running");
    wait_for("fade end", 2.0, || !p.decks.fading.load(Ordering::Acquire));
    assert_eq!(p.decks.active.load(Ordering::SeqCst), 1);
    let ended = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|e| matches!(e, PlayerEvent::TrackEnded))
        .count();
    assert_eq!(ended, 1);
    p.stop().unwrap();
}
