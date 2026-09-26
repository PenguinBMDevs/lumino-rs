//! Root MIDI 文档挂载
//!
//! 设置 MIDI 文档（独占所有权，2026-08 单一权威源）并重建自动化 lane。

use crate::root::Root;
use lumino_midi_loader::MidiDocument;

impl Root {
    /// 设置 MIDI 文档（独占所有权，供编辑/渲染/保存）
    ///
    /// 2026-08 单一权威源改造：`EditorData.document` 独占持有 `MidiDocument`，
    /// 不再以 `Arc` 共享。控制事件按音轨导入 automation_lanes（与 Yinhe 对齐）。
    pub fn set_midi_document(&mut self, doc: MidiDocument) {
        // REND-003：批量导入控制事件（CC/PB → automation lane）。
        // 旧实现逐条 `apply_automation_edit`：每条 retain + 全量 sort + 重算控制柄，
        // 单 lane 累计 O(M² log M)——实测 32.3 万条事件的 lane 会让加载长时间卡死；
        // 批量路径按 (track, target) 分组单遍构建，语义见方法文档。
        self.editor
            .editor_state
            .data
            .import_control_events_from_document(&doc);

        // 单一权威源：文档独占存入 EditorData
        self.editor.editor_state.data.document = Some(doc);
    }
}
