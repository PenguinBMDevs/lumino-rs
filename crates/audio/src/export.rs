//! 离线 WAV 导出 — 复用 AudioEngine 的渲染逻辑，不使用 cpal。
//!
//! 直接在当前线程渲染，写入 WAV 文件。

use std::path::Path;
use std::sync::Arc;

use hound::{WavSpec, WavWriter};
use lumino_midi_loader::MidiDocument;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions};
use xsynth_core::{AudioStreamParams, ChannelCount};

use crate::audio_model::prepare_model;
use crate::engine::{AudioEngine, PlayState, RenderConfig};

const EXPORT_BLOCK_FRAMES: usize = 256;
const STEREO: usize = 2;

/// 导出错误。
#[derive(Debug)]
pub enum ExportError {
    SoundfontLoad(String),
    WavWrite(String),
    EngineError(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SoundfontLoad(e) => write!(f, "音色库加载失败: {}", e),
            Self::WavWrite(e) => write!(f, "WAV 写入失败: {}", e),
            Self::EngineError(e) => write!(f, "引擎错误: {}", e),
        }
    }
}

impl std::error::Error for ExportError {}

/// 将 MIDI 文档渲染为 WAV 文件。
///
/// `progress` 回调在渲染过程中被调用，参数为 0.0..1.0 的进度。
pub fn render_to_wav(
    doc: &Arc<MidiDocument>,
    soundfont_paths: &[std::path::PathBuf],
    sample_rate: u32,
    output_path: &Path,
    progress: Option<&dyn Fn(f64)>,
) -> Result<(), ExportError> {
    // 1. 创建引擎
    let config = RenderConfig {
        sample_rate,
        block_size: EXPORT_BLOCK_FRAMES,
    };
    let mut engine = AudioEngine::new(config);

    // 2. 加载音色库
    let audio_params = AudioStreamParams::new(sample_rate, ChannelCount::Stereo);
    let soundfonts: Vec<Arc<dyn SoundfontBase>> = soundfont_paths
        .iter()
        .filter_map(|p| {
            SampleSoundfont::new(p, audio_params, SoundfontInitOptions::default())
                .ok()
                .map(|sf| Arc::new(sf) as Arc<dyn SoundfontBase>)
        })
        .collect();
    if soundfonts.is_empty() && !soundfont_paths.is_empty() {
        return Err(ExportError::SoundfontLoad(
            "无法加载任何音色库".to_string(),
        ));
    }
    engine.set_soundfonts(soundfonts);

    // 3. 加载模型
    let model = prepare_model(doc, sample_rate);
    let total_samples = model.duration_samples;
    engine.load_model(model);

    if let Some(cb) = progress {
        cb(0.0);
    }

    // 4. 开始播放
    engine.play();

    // 5. 创建 WAV writer
    let spec = WavSpec {
        channels: STEREO as u16,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(output_path, spec).map_err(|e| ExportError::WavWrite(e.to_string()))?;

    // 6. 渲染循环
    let mut scratch = vec![0.0f32; EXPORT_BLOCK_FRAMES * STEREO];
    let mut rendered = 0u64;
    let mut last_progress_update = 0u64;

    while engine.play_state == PlayState::Playing && rendered < total_samples {
        crate::engine_render::render_block(&mut engine, &mut scratch);

        // 写入 WAV (f32 → i16)
        for &sample in &scratch {
            let val = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer
                .write_sample(val)
                .map_err(|e| ExportError::WavWrite(e.to_string()))?;
        }

        rendered += EXPORT_BLOCK_FRAMES as u64;

        // 进度回调（每 1% 更新一次）
        if let Some(cb) = progress {
            let progress_interval = (total_samples / 100).max(1);
            if rendered - last_progress_update >= progress_interval {
                let pct = (rendered as f64 / total_samples as f64).min(1.0);
                cb(pct);
                last_progress_update = rendered;
            }
        }
    }

    engine.all_notes_off();

    writer
        .finalize()
        .map_err(|e| ExportError::WavWrite(e.to_string()))?;

    if let Some(cb) = progress {
        cb(1.0);
    }

    Ok(())
}
