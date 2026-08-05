//! 编辑器数据层（结构定义 + 构造 + 测试）
//!
//! 方法实现已拆分为同级子模块：
//! - `automation`：自动化 lane 管理、编辑与导出
//! - `notes`：音符 CRUD、分割、合并、选择框
//! - `history`：Undo/Redo 历史记录

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lumino_note_core::arrange_selection::ArrangeSelection;
use lumino_note_core::automation::AutomationLane;
use lumino_note_core::event::{
    ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent,
};
use lumino_note_core::history::History;
use lumino_note_core::midi_types::{CcData, TempoPoint};
use lumino_note_core::note::Note;
use lumino_note_core::note_store::NoteStore;

mod accessors;
pub(crate) mod async_commit;
pub(crate) mod async_commit_streaming;
mod automation;
mod construct;
mod event_ops;
mod history;
mod note_store_ops;
mod notes;
mod reset;
mod shape_convert;
#[cfg(test)]
mod tests_automation;
#[cfg(test)]
mod tests_basics;
#[cfg(test)]
mod tests_build_points;
#[cfg(test)]
mod tests_event_ops;
#[cfg(test)]
mod tests_history;
#[cfg(test)]
mod tests_note_ops;

/// 编辑器数据
#[derive(Debug)]
pub struct EditorData {
    pub notes: im::Vector<Note>,
    pub current_track: usize,
    pub track_notes: HashMap<usize, im::Vector<Note>>,
    /// 递增版本号，track_notes 每次变化时 bump。
    /// 用于 NoteWorker 快照的 Arc 缓存失效检测，避免每帧全量克隆 HashMap。
    pub track_notes_gen: u64,
    /// 被编辑过的音轨集合（用于协作同步，记录需要广播变更的所有音轨）
    pub edited_tracks: HashSet<usize>,
    /// 最近一次 `track_notes_gen` 变化明确影响的音轨集合（洋葱皮增量判断用）
    ///
    /// - `Some(set)`：本次变化明确只影响这些音轨（如 `mark_current_track_changed`）
    /// - `None`：变化来源未知或影响全部音轨（保守，触发洋葱皮全量重建）
    ///
    /// 洋葱皮只显示「非当前音轨 + 非静音音轨」：当 `Some(set)` 且集合全部落在
    /// 洋葱皮跳过范围内时，`stream_onion_skin_instances` 可豁免全量重建上传，
    /// 避免编辑主音轨（最常见热路径，拖动音符每帧触发）导致其他音轨全量重传。
    pub onion_dirty_tracks: Option<HashSet<usize>>,
    pub document: Option<Arc<lumino_midi_model::MidiDocument>>,
    pub history: History,
    /// 异步提交的待完成状态（MoveOp 后台应用）
    pub(crate) pending_commit: Option<async_commit::PendingCommit>,
    pub cc_data: CcData,
    /// 自动化 lane 列表。`Arc` 使撤销快照可 O(1) 共享未修改的 lane；
    /// 修改 lane 前必须经 `Arc::make_mut`（见 editor_data/automation.rs）。
    /// lane 数量通常 ≤50，`Vec` 索引写 O(1)。
    pub automation_lanes: Vec<Arc<AutomationLane>>,
    pub tempo_points: Vec<TempoPoint>,
    /// 拍号变化列表（tick, 分子, 分母）。分母为人类可读值，如 4、8。
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 调号事件列表
    pub key_signatures: Vec<KeySignatureEvent>,
    /// 标记事件列表
    pub markers: Vec<MarkerEvent>,
    /// 歌词事件列表
    pub lyrics: Vec<LyricsEvent>,
    /// 和弦事件列表
    pub chords: Vec<ChordEvent>,
    /// 音色变换事件列表
    pub program_changes: Vec<ProgramChangeEvent>,
    /// 高性能 SoA 音符存储（与 `notes` 并存，用于批量操作热路径）
    ///
    /// 当音符数超过 `NOTE_STORE_THRESHOLD` 时自动启用：
    /// - 批量移动走 `batch_move_parallel`（8 线程并行，16M 50% 18ms）
    /// - 批量删除走 `delete_selected`（O(N) 单次遍历）
    /// - 批量插入走 `insert_bulk`（无 realloc，1ms/1000 音符）
    ///
    /// 启用后 `notes` 仍作为权威源，`note_store` 通过 `sync_note_store()` 同步。
    /// 后续迁移完成后 `notes` 将退化为 `note_store` 的视图。
    pub note_store: NoteStore,
    /// note_store 启用阈值（音符数低于此值时不启用，避免小数据量开销）
    pub note_store_enabled: bool,
    /// note_store 被修改后尚未同步到 `notes`（避免每次拖动提交都做 O(N) to_im_vector）
    pub note_store_dirty: bool,
    /// 工程走带视图的选择范围
    pub arrange_selection: ArrangeSelection,
    /// 视觉位置 → 文档音轨索引 映射
    ///
    /// `track_visual_order[i]` 返回视觉位置 i 对应的文档音轨索引。
    /// 侧边栏音轨按原始序号排列，视觉位置 i 等于文档音轨索引（恒等映射）。
    /// 此映射仍保留供 arrangement 操作统一使用。若将来侧边栏顺序与文档顺序
    /// 不一致（如拖动排序），只需更新此映射即可。
    pub track_visual_order: Vec<usize>,
}

/// NoteStore 启用阈值：音符数超过此值时自动启用 SoA 批量操作
pub const NOTE_STORE_THRESHOLD: usize = 10_000;

// `impl Default` / `impl EditorData` 分散在子模块中：
// - `construct` — new() + Default
// - `reset` — reset()
// - `accessors` — mark_track_notes_changed, visual_position_of, current_track_notes
// - `automation` — automation lane 操作
// - `notes` — 音符 CRUD
// - `history` — Undo/Redo
// - `note_store_ops` — NoteStore 集成操作
