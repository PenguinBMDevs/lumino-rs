//! 音频限制器 — 参考 OmniConverter 的 Limiter / Compressor
//!
//! 基于 LoudMax 算法实现的简单限幅器，防止音频削波。
//! 支持多声道处理，每个声道独立压缩。

/// 单声道压缩器
struct Compressor {
    threshold: f32,
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,

    /// 延迟缓冲区（lookahead）
    delay_buffer: Vec<f32>,
    write_idx: usize,
    read_idx: usize,

    envelope: f32,
    gain: f32,
}

impl Compressor {
    fn new(
        sample_rate: f32,
        threshold: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        lookahead_ms: f32,
    ) -> Self {
        // attack_ms = 0 → 包络瞬时跟随（coeff = 0.0），配合 lookahead 实现
        // 真正的 brickwall：增益在峰值样本到达输出前就已压下。
        // 注意不能用 `1.0`（coeff=1 表示永不更新，包络冻结）。
        let attack_coeff = if attack_ms > 0.0 {
            (-1.0 / (attack_ms * 0.001 * sample_rate)).exp()
        } else {
            0.0
        };
        let release_coeff = if release_ms > 0.0 {
            (-1.0 / (release_ms * 0.001 * sample_rate)).exp()
        } else {
            0.0
        };

        let buf_size = (lookahead_ms * 0.001 * sample_rate).ceil() as usize;
        let buf_size = buf_size.max(1);

        Compressor {
            threshold,
            ratio,
            attack_coeff,
            release_coeff,
            delay_buffer: vec![0.0; buf_size],
            write_idx: 0,
            read_idx: 1 % buf_size,
            envelope: 0.0,
            gain: 1.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        // 非有限样本（NaN/Inf）按 0 处理：上游一次数值污染会通过包络状态
        // 永久旁路限幅器（实测首个 NaN 后输出峰值飙到 15 倍满幅）。
        let input = if input.is_finite() { input } else { 0.0 };

        // Lookahead delay line
        self.delay_buffer[self.write_idx] = input;
        let delayed_input = self.delay_buffer[self.read_idx];

        // 防御：状态被污染时复位，保证限幅器不会永久失效
        if !self.envelope.is_finite() {
            self.envelope = 0.0;
        }
        if !self.gain.is_finite() {
            self.gain = 1.0;
        }

        // Envelope detection
        let rectified = input.abs();
        if rectified > self.envelope {
            self.envelope =
                self.attack_coeff * self.envelope + (1.0 - self.attack_coeff) * rectified;
        } else {
            self.envelope =
                self.release_coeff * self.envelope + (1.0 - self.release_coeff) * rectified;
        }

        // Gain computation
        let target_gain = if self.envelope > self.threshold {
            (self.threshold + (self.envelope - self.threshold) / self.ratio) / self.envelope
        } else {
            1.0
        };

        // Gain application (instant attack, smooth release)
        if target_gain < self.gain {
            self.gain = target_gain;
        } else {
            self.gain = self.release_coeff * self.gain + (1.0 - self.release_coeff) * target_gain;
        }

        let output = delayed_input * self.gain;

        // Update buffer indices
        self.write_idx = (self.write_idx + 1) % self.delay_buffer.len();
        self.read_idx = (self.read_idx + 1) % self.delay_buffer.len();

        output
    }
}

/// 多声道音频限制器
///
/// 参考 OmniConverter 的 `Limiter` 类，基于 LoudMax 算法。
/// 每个声道独立压缩，防止音频削波。
pub struct AudioLimiter {
    compressors: Vec<Compressor>,
    num_channels: usize,
    /// 处理过程中遇到并已按静音处理的非有限样本数（NaN/Inf 诊断）。
    non_finite_samples: u64,
}

impl AudioLimiter {
    const RATIO: f32 = 1000.0;
    /// 瞬时 attack（包络不滞后）+ 10ms lookahead：在峰值样本到达输出前完成增益下压，
    /// 实测修复前 attack=10ms 与 lookahead 相等，瞬态每音符先原样通过（峰值 1.5–4×）。
    const ATTACK_MS: f32 = 0.0;
    const RELEASE_MS: f32 = 50.0;
    const LOOKAHEAD_MS: f32 = 10.0;

    /// 创建新的限制器
    ///
    /// # 参数
    /// - `sample_rate`: 采样率（Hz）
    /// - `num_channels`: 声道数
    /// - `threshold`: 阈值（0.0 ~ 1.0），超过此值的信号将被压缩
    pub fn new(sample_rate: u32, num_channels: u16, threshold: f32) -> Self {
        let num_channels = num_channels as usize;
        let compressors = (0..num_channels)
            .map(|_| {
                Compressor::new(
                    sample_rate as f32,
                    threshold,
                    Self::RATIO,
                    Self::ATTACK_MS,
                    Self::RELEASE_MS,
                    Self::LOOKAHEAD_MS,
                )
            })
            .collect();

        AudioLimiter {
            compressors,
            num_channels,
            non_finite_samples: 0,
        }
    }

    /// 本实例处理过的非有限样本（NaN/Inf）数量（已按静音处理）。
    pub fn non_finite_samples(&self) -> u64 {
        self.non_finite_samples
    }

    /// 处理一批 interleaved 样本
    pub fn process(&mut self, samples: &mut [f32]) {
        for chunk in samples.chunks_mut(self.num_channels) {
            for (ch, sample) in chunk.iter_mut().enumerate() {
                if !sample.is_finite() {
                    self.non_finite_samples += 1;
                    *sample = 0.0;
                }
                if ch < self.compressors.len() {
                    *sample = self.compressors[ch].process(*sample);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_stereo_does_not_crash() {
        let mut limiter = AudioLimiter::new(44100, 2, 0.1);
        let mut samples = vec![0.5f32; 1024];
        // 应该不会崩溃
        limiter.process(&mut samples);
        // 输出应该没有 NaN
        for &s in &samples {
            assert!(s.is_finite(), "样本包含 NaN 或 Inf");
        }
    }

    #[test]
    fn test_limiter_reduces_clipping() {
        let mut limiter = AudioLimiter::new(1000, 1, 0.1);
        let mut samples = vec![1.0f32; 200]; // 全削波，足够覆盖lookahead延迟
        limiter.process(&mut samples);
        // 后半部分的峰值应该被降低
        let tail: Vec<f32> = samples.iter().skip(100).copied().collect();
        let max_val = tail.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(max_val < 1.0, "限制器应降低削波峰值: {max_val}");
        assert!(max_val > 0.0, "限制器不应完全静音: {max_val}");
    }

    /// 密集瞬态（±10，每 64 样本翻转）必须被压到阈值附近。
    ///
    /// 回归：修复前 attack=10ms 与 lookahead 相等，每个瞬态先原样通过
    /// 10ms，实测导出 WAV 峰值 1.5–4×（用户报告“限幅器没生效”）。
    #[test]
    fn test_limiter_catches_dense_transients() {
        let mut limiter = AudioLimiter::new(48_000, 1, 0.95);
        let mut samples: Vec<f32> = (0..48_000)
            .map(|i| if (i / 64) % 2 == 0 { 10.0 } else { -10.0 })
            .collect();
        limiter.process(&mut samples);
        let tail = &samples[1_000..];
        let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "瞬态峰值应被限制到 ≤1.0，实际 {peak}");
        assert!(peak > 0.5, "不应完全静音，实际 {peak}");
        assert_eq!(limiter.non_finite_samples(), 0);
    }

    /// NaN 不得毒死限幅器状态：污染段之后必须恢复限幅且输出全部有限。
    ///
    /// 回归：修复前一次 NaN 令 envelope 永久 NaN → target_gain 恒 1.0 →
    /// 限幅器永久旁路（实测首个 NaN 后峰值飙到 15×）。
    #[test]
    fn test_limiter_survives_nan_pollution() {
        let mut limiter = AudioLimiter::new(48_000, 1, 0.95);
        let mut samples: Vec<f32> = vec![f32::NAN; 100];
        samples.extend(vec![10.0f32; 10_000]);
        limiter.process(&mut samples);
        let tail = &samples[2_000..];
        assert!(tail.iter().all(|s| s.is_finite()), "输出必须全部有限");
        let peak = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0, "NaN 之后限幅器应仍生效，实际峰值 {peak}");
        assert_eq!(limiter.non_finite_samples(), 100, "应记录并净化 100 个 NaN");
    }
}
