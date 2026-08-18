//! Host 音符数据查询与编辑器动作处理

use crate::host::{Host, types::NoteData};
use crate::message;

impl Host {
    /// 获取编辑器中的所有音符数据（用于保存）
    ///
    /// 返回 (track_idx, notes) 列表，其中 notes 格式为 (tick, key, length, velocity, channel)。
    /// 单一权威源：音符一律从 document 读取（2026-08 改造）。
    pub fn get_editor_notes(&self) -> Vec<(usize, Vec<NoteData>)> {
        let mut result = Vec::new();
        let Some(doc) = self.root.editor.editor_state.data.document.as_ref() else {
            return result;
        };
        for track_idx in 0..doc.track_count() {
            let notes = doc.track_notes(track_idx);
            if notes.is_empty() {
                continue;
            }
            let track_notes: Vec<NoteData> = notes
                .iter()
                .map(|n| {
                    (
                        n.start_tick as f32,
                        n.key,
                        (n.end_tick - n.start_tick) as f32,
                        n.velocity,
                        n.channel,
                    )
                })
                .collect();
            result.push((track_idx, track_notes));
        }
        result
    }

    /// 获取编辑器中的音符数量（用于判断是否有内容）
    pub fn get_editor_note_count(&self) -> usize {
        let Some(doc) = self.root.editor.editor_state.data.document.as_ref() else {
            return 0;
        };
        (0..doc.track_count())
            .map(|track_idx| doc.track_notes(track_idx).len())
            .sum()
    }

    /// 获取当前选中的音符（用于"导出为素材"）
    ///
    /// - 卷帘模式：当前音轨的选中音符索引（`selected_notes` / `selection_bitset`）；
    /// - 走带模式：`arrange_selection` 跨音轨矩形框选覆盖的音符。
    ///
    /// 返回 `(track_idx, [(tick, key, length, velocity, channel)])`（仅含选中音符的音轨）。
    pub fn get_selected_notes(&self) -> Vec<(usize, Vec<NoteData>)> {
        let mut result: Vec<(usize, Vec<NoteData>)> = Vec::new();
        let Some(doc) = self.root.editor.editor_state.data.document.as_ref() else {
            return result;
        };
        let data = &self.root.editor.editor_state.data;

        // 卷帘模式：当前轨选中音符索引
        if self.root.editor.has_selection() {
            let indices = self.root.editor.get_selected_indices();
            let notes = doc.track_notes(data.current_track);
            let selected: Vec<NoteData> = indices
                .into_iter()
                .filter_map(|idx| notes.get(idx))
                .map(|n| {
                    (
                        n.start_tick as f32,
                        n.key,
                        (n.end_tick - n.start_tick) as f32,
                        n.velocity,
                        n.channel,
                    )
                })
                .collect();
            if !selected.is_empty() {
                result.push((data.current_track, selected));
            }
            return result;
        }

        // 走带模式：跨音轨矩形框选
        let arrangement = &data.arrange_selection;
        if !arrangement.is_empty() {
            for track_idx in 0..doc.track_count() {
                let notes = doc.track_notes(track_idx);
                let selected: Vec<NoteData> = notes
                    .iter()
                    .filter(|n| arrangement.contains(track_idx as u16, n.start_tick, n.key))
                    .map(|n| {
                        (
                            n.start_tick as f32,
                            n.key,
                            (n.end_tick - n.start_tick) as f32,
                            n.velocity,
                            n.channel,
                        )
                    })
                    .collect();
                if !selected.is_empty() {
                    result.push((track_idx, selected));
                }
            }
        }
        result
    }

    /// 检查音符数据是否已变化
    pub fn has_notes_changed(&self) -> bool {
        self.root.editor.notes_changed()
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<message::AudioAction> {
        self.root.take_audio_actions()
    }

    /// 处理编辑器动作
    ///
    /// 仅在音符数据确实发生变化时才标记当前音轨贴图瀑布流为脏。
    /// 先按动作类型过滤：只有可能修改音符的动作才检查 `notes_changed()`，
    /// 避免 Moved/Released/Copy/SelectAll 等不会改音符的动作被误判为脏音轨。
    pub fn handle_action(&mut self, action: message::EditorAction) {
        puffin::profile_function!();
        let track_idx = self.root.editor.current_track() as u16;

        // 先确定该动作是否可能修改音符数据
        // 确定会改：Delete/Cut/Paste → 直接标记脏，不问 notes_changed
        // 可能改：Pressed/Released/DoubleClicked/Undo/Redo → 依赖 notes_changed 判断
        // 绝不会改：Moved/Copy/SelectAll/Scrubbed/Scrolled/IndicatorDrag → 跳过
        let is_definite_mutation = matches!(
            action,
            message::EditorAction::DeletePressed
                | message::EditorAction::Cut
                | message::EditorAction::Paste
        );
        let is_possible_mutation = matches!(
            action,
            message::EditorAction::Pressed { .. }
                | message::EditorAction::Released
                | message::EditorAction::DoubleClicked(_)
                | message::EditorAction::Undo
                | message::EditorAction::Redo
        );
        let notes_changed = self.root.handle_editor_action(action);
        if is_definite_mutation || (is_possible_mutation && notes_changed) {
            // 编辑动作确实改变了音符 → 标记当前音轨贴图瀑布流为脏
            self.mark_waterfall_dirty(track_idx);
        }
        // 仅请求重绘，不重建UI树（编辑器动作由canvas/WGPU层处理）
        self.window_ctx.window.request_redraw();
    }
}
