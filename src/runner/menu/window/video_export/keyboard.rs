//! 视频导出键盘渲染
//!
//! 包含：静态键盘贴图生成、带演奏高亮的键盘合成。
//!
//! 按键颜色不再由 CPU 增量扫描：渲染线程用 GPU 活跃键内核按 tick 推导
//! 128 键色（1KB 回读）并随 `VideoFrameMessage` 同序送达，导出线程零扫描。
//!
//! 拆分原因：原 `video_export.rs` 超过 400 行限制，
//! 键盘相关逻辑独立成子模块，便于维护和测试。

/// 导出视频使用的按键颜色缓冲区大小
///
/// 与编辑器 `playback_key_colors` 保持一致（256 键 × 4 通道）。
pub const KEY_COLOR_BYTES: usize = 1024;

/// 导出视频固定使用标准 128 键 MIDI 键盘
const EXPORT_KEY_COUNT: usize = 128;

/// 洋葱皮覆盖层的不透明度（0.6 × 255）
const OVERLAY_ALPHA: u8 = 153;

mod texture;

#[cfg(test)]
mod tests;

pub use texture::{composite_keyboard, generate_keyboard_texture};
// CPU 键色增量扫描（`update_playback_key_colors` + `PlaybackKeyColorState` +
// `SEEK_THRESHOLD_TICKS`）已由渲染线程 GPU 活跃键内核取代并删除。
//
// 证据边界（诚实声明，勿再指向无关测试）：
// - 「按 tick 选中哪些键、输出什么色」由 gfx
//   `global_bucket::cull_tests::test_cull_active_keys_match_cpu_reference` 对拍锁定
//   （`bucket_cull.wgsl` 的 `compute_active`，alpha=0xFF）；
// - 「键色 → 像素混合」由本模块 `tests::test_composite_keyboard_colors_blend`
//   锁定（overlay 60% 叠加）；
// - 端到端导出像素等价仍需真机回归（两段各有单测，链路组合未在 CI 覆盖）。
