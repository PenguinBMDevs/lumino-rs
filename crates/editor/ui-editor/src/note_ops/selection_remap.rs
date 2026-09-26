//! 主选择索引重映射：结构编辑后按音符值恢复选中
//!
//! 主选择（`selected_notes`）以**当前轨索引**存储（索引位图模式保留，不做修改）。
//! 任何改变当前轨音符列表的操作——远端协作增删移、undo/redo 回放、
//! razor 切割、拖动提交写回、绘制插入——都会位移索引，使选中集指向
//! 错误音符（用户报告：A 端选中一段后 B 端在同轨头部插入音符，
//! A 端再按 Delete 会命中错误音符）。
//!
//! 使用模式（guard，结构编辑前后各一次调用）：
//! ```ignore
//! let identity = editor.capture_selection_identity(); // 结构编辑前
//! // ...mutate（增/删/移/整轨替换）...
//! editor.remap_selection_by_identity(&identity);       // 结构编辑后
//! ```
//!
//! 身份为音符值快照（`NoteEvent` 全字段），经
//! [`ChunkedList::position_of`] 窗口二分精确重定位（O(log N + 同 tick 数)，
//! 无全扫兜底）；未命中的音符（被删/被替换）自动取消选中。
//! 选中集为空时捕获为空快照，重映射为零开销空操作。
//! 同值多份按份数重建（逐个窗口定位，删一即任一，集合语义等价）。
//!
//! **维护约束**：新增任何「改变当前轨音符列表」的代码路径时，必须用
//! 本 guard 包裹，否则主选择会再次漂移。
//!
//! [`ChunkedList::position_of`]: lumino_midi_model::ChunkedList::position_of

use super::Editor;

/// 逐音符捕获上限：超过则放弃捕获（防超大选中集内存/耗时失控），
/// 重映射时保守清空选择（宁可丢选中，不可选错音符）。
const MAX_CAPTURE_ENTRIES: usize = 1 << 18; // 262_144

/// 结构编辑前捕获的主选择身份快照（见模块文档）
#[derive(Debug)]
pub struct SelectionIdentity {
    /// 捕获时的当前轨：仅该轨的结构编辑会影响选中索引
    track: usize,
    /// 选中音符值快照；
    /// `None` = 选中集过大未逐音符捕获 → 重映射时保守清空
    entries: Option<Vec<lumino_midi_model::NoteEvent>>,
}

impl SelectionIdentity {
    /// 捕获轨（供跨 crate 协作 Move 专路判断当前轨一致性）。
    pub fn track(&self) -> usize {
        self.track
    }

    /// 值快照引用（供跨 crate Move 专路计算新值目标，无全扫）。
    pub fn entries(&self) -> Option<&Vec<lumino_midi_model::NoteEvent>> {
        self.entries.as_ref()
    }

    /// 兼容旧名（去 ID 后按值，供协作 Move 专路调用）。
    pub fn entries_for_move(&self) -> Option<&Vec<lumino_midi_model::NoteEvent>> {
        self.entries.as_ref()
    }
}

impl Editor {
    /// 结构编辑前捕获主选择身份（无选中返回空快照，重映射为空操作）
    pub fn capture_selection_identity(&self) -> SelectionIdentity {
        let track = self.editor_state.data.current_track;
        let interaction = &self.editor_state.interaction;
        let notes = self.editor_state.data.track_notes(track);

        if interaction.selected_notes.len() > MAX_CAPTURE_ENTRIES {
            return SelectionIdentity {
                track,
                entries: None,
            };
        }
        // 按值捕获（窗口定位用，无 ID）
        let entries: Vec<lumino_midi_model::NoteEvent> = interaction
            .selected_notes
            .iter()
            .filter_map(|i| notes.get(i).copied())
            .collect();
        SelectionIdentity {
            track,
            entries: Some(entries),
        }
    }

    /// 结构编辑后按值重映射主选择。
    ///
    /// - 捕获轨已非当前轨（选择已被清空/重建）→ 不动；
    /// - 捕获时无选中 → 不动（避免误清空编辑期间新建的选择）；
    /// - 超大选中集未逐音符捕获 → 保守清空；
    /// - 其余：清空后按值窗口重定位重建（未命中者取消选中，无全扫）。
    pub fn remap_selection_by_identity(&mut self, identity: &SelectionIdentity) {
        if identity.track != self.editor_state.data.current_track {
            return;
        }
        let Some(ref entries) = identity.entries else {
            // 超大选中集：保守清空（宁可丢选中，不可选错音符）
            self.selection_clear();
            return;
        };
        if entries.is_empty() {
            return;
        }

        // 逐个窗口定位（O(k log N)，无全扫；大批量亦然，禁止单次全轨扫描兜底）。
        self.selection_clear();
        for ev in entries {
            let found = self
                .editor_state
                .data
                .track_notes(identity.track)
                .position_of(ev);
            if let Some(idx) = found {
                self.selection_insert(idx);
            }
        }
    }
}
