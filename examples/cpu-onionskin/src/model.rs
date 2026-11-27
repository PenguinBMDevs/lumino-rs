//! 派生索引：直接复用共享内核 `lumino_cpu_onion::index`。
//!
//! T-32c 起示例不再维护分叉索引：`TrackIndex` 的构建、增量维护、长音符
//! 候选与 key 掩码全部由共享 crate 提供，示例只在 LOD 判定处保留
//! `Camera`（UI 边界：键盘栏 / 标尺 / total_ticks 裁剪）适配。

pub use lumino_cpu_onion::index::{LOOKBACK_TICKS, TrackIndex};
