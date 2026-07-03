//! Worker 线程任务 — 在后台预计算 PreparedModel 和加载 SoundFont。
//!
//! 避免在 renderer 线程做重活（解析模型、加载 SF2），防止音频卡顿。

use std::path::PathBuf;
use std::sync::Arc;

use lumino_midi_loader::MidiDocument;
use xsynth_core::AudioStreamParams;
use xsynth_core::ChannelCount;
use xsynth_core::soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions};

use crate::audio_model::{prepare_export_model, prepare_playback_model};

/// Worker 线程的结果。
pub(crate) enum WorkerResult {
    ModelPrepared {
        model: crate::audio_model::PreparedModel,
        soundfonts: Vec<Arc<dyn SoundfontBase>>,
    },
    Error(String),
}

/// 在 worker 线程中执行**离线导出**预计算 — 包含完整音符索引。
///
/// 构建完整的 `PreparedModel`（含 `notes_by_key`），用于 WAV 导出。
/// 对于 160M 音符的 MIDI 文件，此函数会消耗大量内存和时间。
pub(crate) fn run_worker_export(
    doc: Arc<MidiDocument>,
    soundfont_paths: Vec<PathBuf>,
    sample_rate: u32,
) -> WorkerResult {
    let model = prepare_export_model(&doc, sample_rate);
    let soundfonts = load_soundfonts(&soundfont_paths, sample_rate);
    WorkerResult::ModelPrepared { model, soundfonts }
}

/// 在 worker 线程中执行**实时播放**预计算 — 轻量级，零音符拷贝。
///
/// 只提取 tempo + CC 数据（`notes_by_key = None`），不拷贝任何音符数据。
/// 实时播放的事件通过 MIDI-stream（PlaybackManager → AudioCommandAdapter）直接注入
/// ChannelGroup，不需要 PreparedModel 的按 key 分桶索引。
///
/// 对于 160M 音符的文件，此函数仅需 ~毫秒级时间（只处理 tempo + CC，跳过音符）。
pub(crate) fn run_worker_playback(
    doc: Arc<MidiDocument>,
    soundfont_paths: Vec<PathBuf>,
    sample_rate: u32,
) -> WorkerResult {
    let model = prepare_playback_model(&doc, sample_rate);
    let soundfonts = load_soundfonts(&soundfont_paths, sample_rate);
    WorkerResult::ModelPrepared { model, soundfonts }
}

/// 从文件路径列表加载音色库。
///
/// 被 `run_worker_export` 和 `run_worker_playback` 共享。
fn load_soundfonts(soundfont_paths: &[PathBuf], sample_rate: u32) -> Vec<Arc<dyn SoundfontBase>> {
    let audio_params = AudioStreamParams::new(sample_rate, ChannelCount::Stereo);
    soundfont_paths
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
        .collect()
}
