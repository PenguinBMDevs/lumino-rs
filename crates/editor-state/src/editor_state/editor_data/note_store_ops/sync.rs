//! NoteStore 同步操作（降级兼容层）
//!
//! NoteStore 已删除，这两个函数保留签名以兼容下游调用，内部为 no-op。
//! 下游重接到 MidiDocument 后本文件整体删除。

use super::super::EditorData;

impl EditorData {
    /// 同步 notes → note_store（降级 no-op）
    ///
    /// NoteStore 冗余层已删除（单一权威源改造），音符始终以 `notes` 为权威，
    /// 无需再同步 SoA 镜像。保留签名兼容下游调用。
    pub fn sync_note_store(&mut self) {
        // NoteStore 已删除：权威源为 notes（后续迁移为 MidiDocument）
    }

    /// 从 note_store 回写到 notes（降级 no-op）
    ///
    /// 同上，NoteStore 已删除，`notes` 即为权威，无需回写。
    pub fn sync_notes_from_store(&mut self) {
        // NoteStore 已删除：notes 即权威源
    }
}
