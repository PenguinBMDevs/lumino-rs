//! lumino-midi-model — MIDI 数据模型层（纯数据类型，无 I/O、无音频后端依赖）
//!
//! 该 crate 从 `lumino-midi-io`（I/O + 合成器后端）中剥离出纯数据表示，
//! 使数据层（`lumino-midi-loader`）与导出层（`lumino-export`）无需依赖重型音频后端。
//!
//! 分层目标：
//! - `lumino-midi-model`（叶子）：`CompactEvent` / `EventKind` 等纯数据。
//! - `lumino-midi-io`（I/O 后端）：`pub use` 重新导出，保持 `lumino_midi_io::compact` 兼容。
//! - `lumino-midi-loader` / `lumino-export`：仅依赖 `lumino-midi-model`，切断对 `midi-io` 的穿透。

pub mod compact;

pub use compact::{CompactEvent, EventKind};
