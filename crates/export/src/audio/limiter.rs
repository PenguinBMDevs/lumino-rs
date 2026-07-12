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
    fn new(sample_rate: f32, threshold: f32, ratio: f32, attack_ms: f32, release_ms: f32, lookahead_ms: f32) -> Self {
        let attack_coeff = if attack_ms > 0.0 {
            (-1.0 / (attack_ms * 0.001 * sample_rate)).exp()
        } else {
            1.0
        };
        let release_coeff = if release_ms > 0.0 {
            (-1.0 / (release_ms * 0.001 * sample_rate)).exp()
        } else {
            1.0
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
        // Lookahead delay line
        self.delay_buffer[self.write_idx] = input;
        let delayed_input = self.delay_buffer[self.read_idx];

        // Envelope detection
        let rectified = input.abs();
        if rectified > self.envelope {
            self.envelope = self.attack_coeff * self.envelope + (1.0 - self.attack_coeff) * rectified;
        } else {
            self.envelope = self.release_coeff * self.envelope + (1.0 - self.release_coeff) * rectified;
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
}

impl AudioLimiter {
    const RATIO: f32 = 1000.0;
    const ATTACK_MS: f32 = 10.0;
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
        }
    }

    /// 处理一批 interleaved 样本
    pub fn process(&mut self, samples: &mut [f32]) {
        for chunk in samples.chunks_mut(self.num_channels) {
            for (ch, sample) in chunk.iter_mut().enumerate() {
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
}
