//! Worker 线程任务 — 在后台预计算 PreparedModel 和加载 SoundFont。
//!
//! 避免在 renderer 线程做重活（解析模型、加载 SF2），防止音频卡顿。

use std::path::PathBuf;
use std::sync::Arc;

use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions};
use xsynth_core::AudioStreamParams;
use xsynth_core::ChannelCount;
use lumino_midi_loader::MidiDocument;

use crate::audio_model::prepare_model;

/// Worker 线程的结果。
pub(crate) enum WorkerResult {
    ModelPrepared {
        model: crate::audio_model::PreparedModel,
        soundfonts: Vec<Arc<dyn SoundfontBase>>,
    },
    Error(String),
}

/// 在 worker 线程中执行预计算。
pub(crate) fn run_worker(
    doc: Arc<MidiDocument>,
    soundfont_paths: Vec<PathBuf>,
    sample_rate: u32,
) -> WorkerResult {
    // 预计算模型
    let model = prepare_model(&doc, sample_rate);

    // 加载音色库
    let audio_params = AudioStreamParams::new(sample_rate, ChannelCount::Stereo);
    let soundfonts: Vec<Arc<dyn SoundfontBase>> = soundfont_paths
        .iter()
        .filter_map(|path| {
            match SampleSoundfont::new(path, audio_params, SoundfontInitOptions::default()) {
                Ok(sf) => Some(Arc::new(sf) as Arc<dyn SoundfontBase>),
                Err(e) => {
                    tracing::warn!("加载音色库失败 {:?}: {}", path, e);
                    None
                }
            }
        })
        .collect();

    WorkerResult::ModelPrepared {
        model,
        soundfonts,
    }
}
