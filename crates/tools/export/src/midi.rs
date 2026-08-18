//! MIDI 文件导出功能
//!
//! 该模块已拆分为以下子模块：
//! - `export`: 主导出逻辑（export_midi, export_midi_to_bytes）
//! - `tracks`: 音轨构建（build_midi_smf, 轨道事件收集）
//! - `encoding`: 编码辅助（bpm_to_tempo, tempo_to_bpm 重导出）
//! - `calc`: 计算辅助（预留）

mod calc;
mod encoding;
mod export;
mod extract;
mod tracks;

pub use encoding::{bpm_to_tempo, tempo_to_bpm};
pub use export::{export_midi, export_midi_to_bytes};
pub use extract::extract_pc_cc_events;

mod types;
pub use types::*;

// ── 测试 ──────────────────────────────────────────────────

#[cfg(test)]
mod tests;
