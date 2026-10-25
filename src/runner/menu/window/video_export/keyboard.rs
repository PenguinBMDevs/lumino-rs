//! 视频导出键盘渲染
//!
//! 包含：静态键盘贴图生成、按键颜色增量计算、带演奏高亮的键盘合成。
//!
//! 拆分原因：原 `video_export.rs` 超过 400 行限制，
//! 键盘相关逻辑独立成子模块，便于维护与测试。
//!
//! 2026-10-25：按键颜色扫描状态改用 `lumino_note_core::PlaybackKeyColorState`，
//! 与编辑器实时播放**共用唯一实现**（此前是两份已漂移的拷贝：一份用
//! `iter_window`、一份用 `iter().skip()`；一份维护活跃集合、一份用每键聚合）。

/// 导出视频使用的按键颜色缓冲区大小
///
/// 与编辑器 `playback_key_colors` 保持一致（256 键 × 4 通道）。
pub use lumino_note_core::KEY_COLOR_BYTES;

/// 播放键色增量扫描状态（与编辑器共用同一实现）
pub use lumino_note_core::PlaybackKeyColorState;

/// 导出视频固定使用标准 128 键 MIDI 键盘
const EXPORT_KEY_COUNT: usize = 128;

/// 洋葱皮覆盖层的不透明度（0.6 × 255）
const OVERLAY_ALPHA: u8 = 153;

mod texture;
mod update;

#[cfg(test)]
mod tests;

pub use texture::{composite_keyboard, generate_keyboard_texture};
pub use update::update_playback_key_colors;
// `update_playback_key_colors_from_notes`（流式读取模式专用：直接消费磁盘音符列表
// 着色）已随审计 §八 的流式导出链删除——它只服务该路径，且构成音符的第三条来源。
