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
        use lumino_note_core::{AutomationEdit, AutomationTarget, SegmentShape};

        // 每次加载新文档时重建自动化 lane，避免旧数据残留
        self.editor.editor_state.data.automation_lanes.clear();

        for ev in &doc.control_events {
            match ev.kind {
                // CC: param 高 8 位为控制器编号，低 8 位为值。
                0 => {
                    let controller = (ev.param >> 8) as u8;
                    let value = ev.param & 0xFF;
                    let edit = AutomationEdit::Add {
                        track_idx: ev.track,
                        target: AutomationTarget::CC { controller },
                        channel: ev.channel,
                        tick: ev.tick,
                        value,
                        shape: SegmentShape::Step,
                    };
                    self.editor.editor_state.data.apply_automation_edit(edit);
                }
                // PitchBend: param 为 14-bit 值（0–16383）。
                2 => {
                    let edit = AutomationEdit::Add {
                        track_idx: ev.track,
                        target: AutomationTarget::PitchBend,
                        channel: ev.channel,
                        tick: ev.tick,
                        value: ev.param,
                        shape: SegmentShape::Step,
                    };
                    self.editor.editor_state.data.apply_automation_edit(edit);
                }
                _ => {}
            }
        }

        // 单一权威源：文档独占存入 EditorData
        self.editor.editor_state.data.document = Some(doc);
    }
}
