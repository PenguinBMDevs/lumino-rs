use lumino_midi_loader::MidiDocument;
use lumino_note_core::{KEY_COLOR_BYTES, PlaybackKeyColorState};

/// 视频导出键盘取色
///
/// 导出是「整篇渲染」，不存在编辑器里的「主音轨」概念，故所有轨一律取调色板色
/// （`note_track_color(track, usize::MAX)` 即 `current_track_color`）。
///
/// alpha 固定 255：该色最终合成到**不透明**键盘贴图（`composite_keyboard` 按
/// `OVERLAY_ALPHA` 混合），沿用调色板自带的 alpha 会让合成结果不可预期。
fn export_track_color(track_idx: usize) -> [u8; 4] {
    let color = lumino_extras::palette::note_track_color(track_idx, usize::MAX);
    [color[0], color[1], color[2], 255]
}

/// 根据当前播放 tick 重算每个 key 的覆盖颜色
///
/// 算法本体见 `lumino_note_core::playback_key_colors`（与编辑器共用）。
/// 导出文档在整次导出期间不变，故内容世代固定传 `0`（永不失效）。
pub fn update_playback_key_colors(
    document: &MidiDocument,
    tick: u32,
    state: &mut PlaybackKeyColorState,
    out: &mut [u8; KEY_COLOR_BYTES],
) {
    state.update(document, tick, 0, &export_track_color, out);
}
