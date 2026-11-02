use super::SynthBackend;

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_synth_backend() -> SynthBackend {
    SynthBackend::XSynth
}

pub(super) fn default_synth_buffer() -> f64 {
    // 30ms：延迟与音符时值量化（事件在渲染块边界生效）的平衡点。
    // BufferedRenderer 会保留至少约半块音频富余，30ms 窗口 ≈ 15ms 抗抖动余量。
    30.0
}
pub(super) fn default_synth_sample_rate() -> u32 {
    44100
}
pub(super) fn default_synth_threads() -> i32 {
    // 已废弃字段的兼容默认值；线程策略由后端按机器核数强制决定（见 api::xsynth）
    0
}
pub(super) fn default_max_voices_per_key() -> Option<usize> {
    // 4 与 xsynth 引擎默认 / GPU 后端默认一致：更高的值会让渲染负载线性增加
    Some(4)
}
pub(super) fn default_lgs_sample_rate() -> u32 {
    64_000
}
pub(super) fn default_lgs_block_size() -> usize {
    512
}
/// REF-LGS-009：离线导出的 GPU 渲染块长（帧）。
///
/// 离线导出对延迟不敏感，2048 是实测收益/内存的平衡点（见
/// `docs/2026-10-31-REF-LGS-009-离线默认块长.md`）：比 512 快 2.5×，
/// 而 4096/8192 的额外收益只有 ~20–30%、每块内存与尾延迟翻倍。
pub(super) fn default_lgs_offline_block_size() -> usize {
    2048
}
pub(super) fn default_lgs_max_voices_per_key() -> usize {
    4
}
pub(super) fn default_lgs_soft_nps_gate() -> bool {
    true
}
pub(super) fn default_automation_line_thickness() -> f32 {
    2.0
}

/// Tempo 面板 BPM 绘制上限默认值
pub(super) fn default_tempo_max_bpm() -> f64 {
    512.0
}

pub(super) fn default_monitor_refresh_interval_ms() -> f32 {
    100.0
}

pub(super) fn default_log_retention_count() -> usize {
    10
}

pub(super) fn default_velocity_filter_threshold() -> u8 {
    1
}

pub(super) fn default_lgs_velocity_filter_threshold() -> u8 {
    1
}

pub(super) fn default_hires_measures_per_group() -> u32 {
    4
}
pub(super) fn default_hires_tile_width() -> u32 {
    1920
}
pub(super) fn default_hires_cooldown() -> u64 {
    10
}
pub(super) fn default_hires_gpu_mem_limit() -> u32 {
    512
}

pub(super) fn default_history_total_limit() -> usize {
    100
}

pub(super) fn default_history_entry_limit() -> usize {
    1000
}

pub(super) fn default_merge_window_ms() -> u64 {
    300
}

/// 复音软目标比例默认值：1-1/e ≈ 0.632（一阶系统目标，留约 37% 暂态余量）
pub(super) fn default_xsynth_voice_target_ratio() -> f64 {
    1.0 - 1.0 / std::f64::consts::E
}

/// 自动保存间隔默认值（分钟）：默认功能关闭，该值仅在用户开启后生效。
pub(super) fn default_autosave_interval_mins() -> u32 {
    5
}
