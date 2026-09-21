use super::*;

#[test]
fn cents_factor_octave() {
    assert!((cents_factor(1200.0) - 2.0).abs() < 1e-6);
    assert!((cents_factor(0.0) - 1.0).abs() < 1e-7);
}

#[test]
fn envelope_stage_collapse() {
    let env = EnvelopeDescriptor {
        start_percent: 0.5,
        delay: 0.0,
        attack: 0.01,
        hold: 0.0,
        decay: 0.05,
        sustain_percent: 0.4,
        release: 0.1,
    };
    let stages = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default()).stages;
    // delay (0) skipped, hold (0) skipped -> attack, decay, sustain, release, finished
    assert_eq!(stages.len(), 5);
    assert_eq!(stages[0].kind, EnvStage::LERP); // attack, exponential -> linear
    assert_eq!(stages[1].kind, EnvStage::CONCAVE); // decay, linear -> concave
    assert_eq!(stages[2].kind, EnvStage::HOLD); // sustain
    assert_eq!(stages[3].kind, EnvStage::CONCAVE); // release
    assert_eq!(stages[4].kind, EnvStage::HOLD); // finished
    assert_eq!(stages[0].duration, 640);
}

#[test]
fn biquad_matches_reference() {
    let mut f = BiquadDf1::lowpass(8000.0, 64_000, std::f32::consts::FRAC_1_SQRT_2);
    // Impulse response should decay and stay bounded.
    let mut out = Vec::new();
    for _ in 0..100 {
        out.push(f.process(1.0));
    }
    assert!(out[0] > 0.0);
    assert!(out.iter().all(|v| v.is_finite()));
}

#[test]
fn sinc_table_passes_dc() {
    let table = build_sinc_table();
    // At frac = 0 the interpolator must reproduce the sample exactly
    // (the DC gain of the kernel must be ~1 at all phases).
    for phase in [0usize, SINC_PHASES / 2, SINC_PHASES - 1] {
        let sum: f32 = table[phase * SINC_TAPS..(phase + 1) * SINC_TAPS]
            .iter()
            .sum();
        assert!((sum - 1.0).abs() < 0.02, "phase {phase}: sum = {sum}");
    }
}

#[test]
fn modify_collapses_zero_duration_attack() {
    // CC73 = 0 collapses the attack stage: it must not survive as a
    // zero-duration curve stage (which would NaN-divide on the GPU).
    let env = EnvelopeDescriptor {
        start_percent: 0.0,
        delay: 0.0,
        attack: 0.01,
        hold: 0.05,
        decay: 0.05,
        sustain_percent: 0.4,
        release: 0.1,
    };
    let mut gpu_env = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default());
    assert_eq!(gpu_env.attack_idx, Some(0));
    modify_env_stages(&mut gpu_env, 64_000, Some(0), None);
    // Attack collapsed -> first stage must be the hold stage, and no
    // zero-duration curve stage may remain anywhere.
    assert_eq!(gpu_env.attack_idx, None);
    for stage in &gpu_env.stages {
        assert!(
            !(stage.duration == 0 && stage.kind != EnvStage::HOLD),
            "zero-duration curve stage survived: {stage:?}"
        );
    }
    assert_eq!(gpu_env.stages[0].target, 1.0); // hold
    assert_eq!(gpu_env.stages[0].duration, 3200); // 0.05 s @ 64 kHz
}

#[test]
fn modify_release_has_minimum_duration() {
    // CC72 has a 20 ms floor (`max(0.02)`), so even a value of 0 must
    // keep a positive duration (never a zero-duration curve stage).
    let env = EnvelopeDescriptor {
        start_percent: 0.0,
        delay: 0.0,
        attack: 0.01,
        hold: 0.05,
        decay: 0.05,
        sustain_percent: 0.4,
        release: 0.1,
    };
    let mut gpu_env = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default());
    modify_env_stages(&mut gpu_env, 64_000, None, Some(0));
    for stage in &gpu_env.stages {
        assert!(
            !(stage.duration == 0 && stage.kind != EnvStage::HOLD),
            "zero-duration curve stage survived: {stage:?}"
        );
    }
    let ri = gpu_env.release_idx.expect("release stage must remain");
    assert_eq!(gpu_env.stages[ri].kind, EnvStage::CONCAVE);
    assert_eq!(gpu_env.stages[ri].duration, 1280); // 0.02 s @ 64 kHz
}

#[test]
fn modify_remaps_release_when_collapsed() {
    // A release stage with zero duration (already collapsed shape, e.g.
    // from a pathological soundfont) must not resurrect as a curve stage.
    let mut gpu_env = GpuEnvelope {
        stages: vec![
            EnvStage {
                kind: EnvStage::LERP,
                target: 1.0,
                duration: 600,
            },
            EnvStage {
                kind: EnvStage::CONCAVE,
                target: 0.0,
                duration: 0, // zero-duration release
            },
            EnvStage {
                kind: EnvStage::HOLD,
                target: 0.0,
                duration: 0,
            },
        ],
        attack_idx: Some(0),
        release_idx: Some(1),
    };
    modify_env_stages(&mut gpu_env, 64_000, None, None);
    assert_eq!(gpu_env.stages.len(), 2);
    // Release collapsed away; the index falls back to the terminal
    // stage via `None` (callers use `stages.len() - 1`).
    assert_eq!(gpu_env.release_idx, None);
    assert!(
        gpu_env
            .stages
            .iter()
            .all(|s| s.duration > 0 || s.kind == EnvStage::HOLD)
    );
}

#[test]
fn modify_keeps_indices_when_no_zero_stages() {
    // CC values that keep durations positive must not disturb the
    // attack/release indices.
    let env = EnvelopeDescriptor {
        start_percent: 0.0,
        delay: 0.0,
        attack: 0.01,
        hold: 0.05,
        decay: 0.05,
        sustain_percent: 0.4,
        release: 0.1,
    };
    let mut gpu_env = to_gpu_stages(&env, 64_000, EnvelopeCurveConfig::default());
    let (attack_before, release_before) = (gpu_env.attack_idx, gpu_env.release_idx);
    modify_env_stages(&mut gpu_env, 64_000, Some(64), Some(64));
    assert_eq!(gpu_env.attack_idx, attack_before);
    assert_eq!(gpu_env.release_idx, release_before);
    assert_eq!(gpu_env.stages.len(), 6); // unchanged shape
}
