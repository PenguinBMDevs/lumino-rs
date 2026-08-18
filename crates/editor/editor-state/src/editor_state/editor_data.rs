//! 编辑器数据层（结构定义 + 构造 + 测试）
//!
//! 方法实现已拆分为同级子模块：
//! - `automation`：自动化 lane 管理、编辑与导出
//! - `notes`：音符 CRUD、分割、合并、选择框
//! - `history`：Undo/Redo 历史记录

use std::collections::HashSet;
use std::sync::Arc;

use lumino_note_core::arrange_selection::ArrangeSelection;
use lumino_note_core::automation::AutomationLane;
use lumino_note_core::event::{
    ChordEvent, KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent,
};
use lumino_note_core::history::History;
use lumino_note_core::midi_types::{CcData, TempoPoint};
use lumino_note_core::note::Note;

pub(crate) mod accessors;
pub(crate) mod async_commit;
pub(crate) mod async_commit_streaming;
mod automation;
mod construct;
mod history;
mod note_store_ops;
mod notes;
mod reset;
mod tempo;
#[cfg(test)]
mod tests_automation;
#[cfg(test)]
mod tests_basics;
#[cfg(test)]
mod tests_build_points;
#[cfg(test)]
mod tests_history;
#[cfg(test)]
mod tests_note_delta;
#[cfg(test)]
mod tests_note_ops;

/// 编辑器数据
#[derive(Debug)]
pub struct EditorData {
    pub current_track: usize,
    /// 递增版本号，音符数据每次变化时 bump。
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
    /// 音符唯一权威源（独占所有权，2026-08 单一权威源改造）
    ///
    /// 原 `notes` / `track_notes` 冗余层已删除，音符数据只存在于 `document`。
    /// - 读取：`current_track_notes()` / `track_notes(track_id)`（见 accessors.rs）
    /// - 写入：`insert_note` / `remove_note` / `update_note` / `replace_track_notes`
    /// - tick 精度：UI 编辑用 f32，写回时无损转换（fract==0 → as u32）
    pub document: Option<lumino_midi_model::MidiDocument>,
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
    /// 工程走带视图的选择范围
    pub arrange_selection: ArrangeSelection,
    /// 主音轨 GPU 增量事件队列（自上次 UI 消费以来的编辑操作）
    ///
    /// 卷帘编辑增量（2026-08-05）：等长修改操作（拖动/变速/翻转/批量编辑）在
    /// 数据层 API 内记录索引级增量事件，UI 层每帧消费并转 `NoteEvent` 增量发送，
    /// 避免每帧全量重建上传可见音符。
    ///
    /// 索引语义：`start_index` 基于「前序事件已应用后的 notes 状态」，
    /// 事件按记录顺序应用，GPU buffer 顺序与 notes 顺序一致。
    pub note_delta_events: Vec<NoteDeltaEvent>,
    /// 是否有「未记录事件」的当前音轨变化（散改/undo/加载/切轨）
    ///
    /// `true` = 渲染层必须全量兜底重建（事件队列不可信）。由 `mark_*` 默认置位，
    /// 事件记录 API 在记录完成后显式清除（见 `record_update_ranges`）。
    pub note_delta_dirty: bool,
    /// 视觉位置 → 文档音轨索引 映射
    ///
    /// `track_visual_order[i]` 返回视觉位置 i 对应的文档音轨索引。
    /// 侧边栏音轨按原始序号排列，视觉位置 i 等于文档音轨索引（恒等映射）。
    /// 此映射仍保留供 arrangement 操作统一使用。若将来侧边栏顺序与文档顺序
    /// 不一致（如拖动排序），只需更新此映射即可。
    pub track_visual_order: Vec<usize>,
}

/// 主音轨 GPU 增量事件（数据层 → UI 渲染层）
///
/// 仅支持**等长**修改（不增删音符）：拖动/变速/翻转/批量编辑。
/// 增删音符、undo/redo、切轨、未知变化走 `note_delta_dirty` 全量兜底——
/// 等长修改是卷帘高频热路径（拖动每帧），增量收益最大。
#[derive(Debug, Clone)]
pub enum NoteDeltaEvent {
    /// 等长区间更新：从 `start_index` 起逐个替换为 `notes` 中的音符
    ///
    /// `notes` 顺序 = notes 索引顺序（连续区间）。合并连续索引后生成。
    UpdateRange {
        start_index: usize,
        notes: Vec<Note>,
    },
    /// 在指定索引处插入单个音符（保持 notes 升序索引语义）
    InsertAt { index: usize, note: Note },
    /// 从指定索引起删除连续 `count` 个音符（保持 notes 升序索引语义）
    RemoveAt { index: usize, count: usize },
}

// `impl Default` / `impl EditorData` 分散在子模块中：
// - `construct` — new() + Default
// - `reset` — reset()
// - `accessors` — mark_track_notes_changed, visual_position_of, current_track_notes
// - `automation` — automation lane 操作
// - `notes` — 音符 CRUD
// - `history` — Undo/Redo
// - `note_store_ops` — NoteStore 集成操作
