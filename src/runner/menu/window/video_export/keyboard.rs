//! 视频导出键盘渲染
//!
//! 包含：静态键盘贴图生成、按键颜色增量计算、带演奏高亮的键盘合成。
//!
//! 拆分原因：原 `video_export.rs` 超过 400 行限制，
//! 键盘相关逻辑独立成子模块，便于维护和测试。

/// 导出视频使用的按键颜色缓冲区大小
///
/// 与编辑器 `playback_key_colors` 保持一致（256 键 × 4 通道）。
pub const KEY_COLOR_BYTES: usize = 1024;

/// 导出视频固定使用标准 128 键 MIDI 键盘
const EXPORT_KEY_COUNT: usize = 128;

/// 判定 seek 阈值（单位：tick）
///
/// 超过此阈值视为非连续播放，需要全量重建活跃音符集合。
const SEEK_THRESHOLD_TICKS: u32 = 5000;

/// 洋葱皮覆盖层的不透明度（0.6 × 255）
const OVERLAY_ALPHA: u8 = 153;

/// 播放键色增量扫描状态
///
/// 与编辑器 `PlaybackScanState` 等价，避免视频导出每帧 O(N) 全量扫描。
/// 视频导出帧按时间顺序生成，正常路径为增量扫描；仅当出现回退或跳变时重建。
#[derive(Default)]
pub struct PlaybackKeyColorState {
    /// 上次扫描到的 tick
    pub last_tick: u32,
    /// 每条音轨上次扫描到的索引
    pub scan_idx: Vec<usize>,
    /// 当前活跃音符缓存：(end_tick, key_color_offset, color)
    pub active_notes: Vec<(u32, usize, [u8; 4])>,
}

mod texture;
mod update;

#[cfg(test)]
mod tests;

pub use texture::{composite_keyboard, generate_keyboard_texture};
pub use update::{update_playback_key_colors, update_playback_key_colors_from_notes};
