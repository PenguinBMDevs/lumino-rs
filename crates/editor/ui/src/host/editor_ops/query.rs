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

    /// 音符总量显示位置（UI-015；工程设置对话框注入与互斥判断用）
    pub fn note_count_display(&self) -> lumino_core::storage::config::NoteCountDisplay {
        self.root.settings.display.note_count_display
    }

    /// 获取当前选中的音符（用于"导出为素材"）
    ///
    /// # 视图仲裁（已收口，见 [`crate::root::Root::active_selection`]）
    ///
    /// 旧实现在此直接判两套选区，犯了两个错：
    /// 1. `if has_selection() { return; }` 让卷帘选区无条件优先，走带选区被忽略——
    ///    而菜单启用条件是 `卷帘非空 || 走带非空`，**能点却导出另一套选区**。
    /// 2. 走带分支把文档音轨索引当视觉轨传进 `ArrangeSelection::contains`，
    ///    `track_visual_order` 非恒等时判定全错。
    ///
    /// 现在只做 **NoteData 类型转换**：取视图、解析选区、主选区空时的回退语义
    /// 全部由 `Editor::resolve_selection` 统一负责（该逻辑在 ui-editor 内有单测覆盖）。
    ///
    /// 返回 `(track_idx, [(tick, key, length, velocity, channel)])`（仅含选中音符的音轨）。
    pub fn get_selected_notes(&self) -> Vec<(usize, Vec<NoteData>)> {
        self.root
            .active_selection()
            .into_tracks()
            .into_iter()
            .map(|(track_idx, notes)| {
                let converted: Vec<NoteData> = notes
                    .into_iter()
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
                (track_idx, converted)
            })
            .collect()
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
