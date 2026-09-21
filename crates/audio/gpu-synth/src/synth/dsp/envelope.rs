/// A volume envelope descriptor in seconds / percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeDescriptor {
    /// Start level (0-1).
    pub start_percent: f32,
    /// Delay in seconds.
    pub delay: f32,
    /// Attack in seconds.
    pub attack: f32,
    /// Hold in seconds.
    pub hold: f32,
    /// Decay in seconds.
    pub decay: f32,
    /// Sustain level (0-1).
    pub sustain_percent: f32,
    /// Release in seconds.
    pub release: f32,
}

/// Envelope curve selection, mirroring XSynth's `EnvelopeOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvelopeCurveConfig {
    /// Attack curve. Default: `Exponential` (linear in amplitude).
    pub attack_curve: CurveKind,
    /// Decay curve. Default: `Linear` (concave in amplitude).
    pub decay_curve: CurveKind,
    /// Release curve. Default: `Linear` (concave in amplitude).
    pub release_curve: CurveKind,
}

/// A curve kind for an envelope stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    /// Linear curve in amplitude space.
    Linear,
    /// Exponential curve in dB space (convex attack / concave decay/release
    /// in amplitude space).
    Exponential,
}

impl Default for EnvelopeCurveConfig {
    fn default() -> Self {
        Self {
            attack_curve: CurveKind::Exponential,
            decay_curve: CurveKind::Linear,
            release_curve: CurveKind::Linear,
        }
    }
}

/// One envelope stage as consumed by the WGSL kernel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvStage {
    /// 0 = linear lerp, 1 = concave `(1-f)^8`, 2 = convex `f^8`, 3 = hold.
    pub kind: u32,
    /// Target value of the stage.
    pub target: f32,
    /// Duration in samples.
    pub duration: u32,
}

impl EnvStage {
    pub const LERP: u32 = 0;
    pub const CONCAVE: u32 = 1;
    pub const CONVEX: u32 = 2;
    pub const HOLD: u32 = 3;
}

/// Evaluates one envelope stage at `f` (0-1 progress), exactly like the
/// XSynth lerpers.
pub fn eval_stage(stage: &EnvStage, from: f32, f: f32) -> f32 {
    match stage.kind {
        EnvStage::LERP => from + (stage.target - from) * f,
        EnvStage::CONCAVE => (from - stage.target) * (1.0 - f).powi(8) + stage.target,
        EnvStage::CONVEX => from + (stage.target - from) * f.powi(8),
        _ => stage.target,
    }
}

/// A prepared GPU envelope: the collapsed stage list plus the indices of the
/// attack and release stages within it (used for CC72/CC73 modifications).
#[derive(Debug, Clone, PartialEq)]
pub struct GpuEnvelope {
    /// Collapsed stages in playback order.
    pub stages: Vec<EnvStage>,
    /// Index of the attack stage (if it survived collapsing).
    pub attack_idx: Option<usize>,
    /// Index of the release stage (if it survived collapsing).
    pub release_idx: Option<usize>,
}

/// Turns an [`EnvelopeDescriptor`] into a compact stage list for the GPU,
/// mirroring XSynth's `to_envelope_params`.
///
/// Stages with zero duration are collapsed (the stage is skipped and its
/// target becomes the next stage's start), exactly like XSynth's
/// `get_stage_data` recursion.
pub fn to_gpu_stages(
    env: &EnvelopeDescriptor,
    sample_rate: u32,
    curves: EnvelopeCurveConfig,
) -> GpuEnvelope {
    let sr = sample_rate as f32;
    let to_samples = |secs: f32| (secs * sr) as u32;

    let attack = match curves.attack_curve {
        CurveKind::Linear => EnvStage {
            kind: EnvStage::CONVEX,
            target: 1.0,
            duration: to_samples(env.attack),
        },
        CurveKind::Exponential => EnvStage {
            kind: EnvStage::LERP,
            target: 1.0,
            duration: to_samples(env.attack),
        },
    };
    let decay = match curves.decay_curve {
        CurveKind::Exponential => EnvStage {
            kind: EnvStage::LERP,
            target: env.sustain_percent,
            duration: to_samples(env.decay),
        },
        CurveKind::Linear => EnvStage {
            kind: EnvStage::CONCAVE,
            target: env.sustain_percent,
            duration: to_samples(env.decay),
        },
    };
    let release = match curves.release_curve {
        CurveKind::Exponential => EnvStage {
            kind: EnvStage::LERP,
            target: 0.0,
            duration: to_samples(env.release),
        },
        CurveKind::Linear => EnvStage {
            kind: EnvStage::CONCAVE,
            target: 0.0,
            duration: to_samples(env.release),
        },
    };

    // Original 7 stages; remember their spec indices for attack (1) and
    // release (5) so CC modifications can target them after collapsing.
    let raw: [(u8, EnvStage); 7] = [
        (
            0,
            EnvStage {
                kind: EnvStage::LERP,
                target: env.start_percent,
                duration: to_samples(env.delay),
            },
        ),
        (1, attack),
        (
            2,
            EnvStage {
                kind: EnvStage::LERP,
                target: 1.0,
                duration: to_samples(env.hold),
            },
        ),
        (3, decay),
        (
            4,
            EnvStage {
                kind: EnvStage::HOLD,
                target: env.sustain_percent,
                duration: 0,
            },
        ),
        (5, release),
        (
            6,
            EnvStage {
                kind: EnvStage::HOLD,
                target: 0.0,
                duration: 0,
            },
        ),
    ];

    // Collapse zero-duration lerp stages (XSynth skips them recursively,
    // starting the next stage from the skipped stage's target).
    let mut out: Vec<EnvStage> = Vec::with_capacity(7);
    let mut attack_idx = None;
    let mut release_idx = None;
    for (spec_idx, stage) in raw {
        if (stage.kind == EnvStage::LERP
            || stage.kind == EnvStage::CONCAVE
            || stage.kind == EnvStage::CONVEX)
            && stage.duration == 0
        {
            continue;
        }
        let idx = out.len();
        if spec_idx == 1 {
            attack_idx = Some(idx);
        }
        if spec_idx == 5 {
            release_idx = Some(idx);
        }
        out.push(stage);
    }
    GpuEnvelope {
        stages: out,
        attack_idx,
        release_idx,
    }
}

/// Modifies envelope stages with CC73 (attack) / CC72 (release) values,
/// mirroring XSynth's `get_modified_envelope`:
///
/// - attack:  duration scaled by `(v/64)^5` (v <= 64) or
///   `1 + ((v-64)/64)^3 * 15` (v > 64)
/// - release: same curve, but the duration is clamped to a minimum of
///   20 ms (`max(0.02)`)
pub fn modify_env_stages(
    envelope: &mut GpuEnvelope,
    sample_rate: u32,
    attack: Option<u8>,
    release: Option<u8>,
) {
    let curve = |value: u8, duration: f32| -> f32 {
        if value <= 64 {
            (value as f32 / 64.0).powi(5) * duration
        } else {
            duration + ((value as f32 - 64.0) / 64.0).powi(3) * 15.0
        }
    };

    if let (Some(attack_val), Some(ai)) = (attack, envelope.attack_idx)
        && let Some(stage) = envelope.stages.get_mut(ai)
    {
        let old = stage.duration as f32 / sample_rate as f32;
        stage.duration = (curve(attack_val, old) * sample_rate as f32) as u32;
    }

    if let (Some(release_val), Some(ri)) = (release, envelope.release_idx)
        && let Some(stage) = envelope.stages.get_mut(ri)
    {
        let old = stage.duration as f32 / sample_rate as f32;
        let dur = curve(release_val, old).max(0.02) * sample_rate as f32;
        stage.duration = dur as u32;
    }

    // Re-collapse curve stages whose duration became zero (e.g. CC73 = 0),
    // mirroring XSynth's recursive skip in `get_stage_data`. A zero-duration
    // curve stage would otherwise divide by zero in the GPU kernel (NaN
    // output that never decays to silence) and its target value is exactly
    // what the next stage starts from anyway.
    //
    // Note: the release stage can never collapse here (it has a 20 ms floor,
    // see above). If it *was* already collapsed by `to_gpu_stages`, the
    // release index points at the terminal hold stage and `None`/the caller's
    // `stages.len() - 1` fallback are equivalent.
    let old_attack = envelope.attack_idx;
    let old_release = envelope.release_idx;
    let mut collapsed: Vec<EnvStage> = Vec::with_capacity(envelope.stages.len());
    let mut new_attack = None;
    let mut new_release = None;
    for (i, stage) in envelope.stages.iter().enumerate() {
        let is_curve = matches!(
            stage.kind,
            EnvStage::LERP | EnvStage::CONCAVE | EnvStage::CONVEX
        );
        if is_curve && stage.duration == 0 {
            // Skipped: the next stage effectively starts from this target.
            continue;
        }
        let ni = collapsed.len();
        if Some(i) == old_attack {
            new_attack = Some(ni);
        }
        if Some(i) == old_release {
            new_release = Some(ni);
        }
        collapsed.push(*stage);
    }
    envelope.stages = collapsed;
    envelope.attack_idx = new_attack;
    envelope.release_idx = new_release;
}
