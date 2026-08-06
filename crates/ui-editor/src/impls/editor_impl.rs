//! Editor 核心方法
//!
//! 包含：构造函数、内存分析、远端光标、音频动作、撤销重做拦截。
//!
//! 注：`update_playback_key_colors` 见 `impls::playback`，
//! `update_selection_box_animation` 见 `impls::selection_box_anim`。

use crate::note::Note;
use crate::velocity::VelocityPanel;
use crate::{Editor, EditorMemory, SpatialIndexState, grid};
use iced_widget::canvas;
use lumino_ui_core::message::AudioAction;
use std::cell::Cell;

impl Editor {
    /// 创建新的编辑器实例
    pub fn new() -> Self {
        // 使用 UI 内存标签包裹编辑器初始化，便于内存监控归因
        lumino_memtrace::with_tag(lumino_memtrace::AllocTag::Ui, || {
            Self {
                editor_state: crate::editor_state::EditorState::new(),
                grid_cache: canvas::Cache::new(),
                keyboard_cache: canvas::Cache::new(),
                ruler_cache: canvas::Cache::new(),
                spatial: SpatialIndexState::default(),
                remote_cursors: std::collections::HashMap::new(),
                playback_position: 0.0,
                playback_key_colors: [0u8; 1024], // 256 keys × 4 bytes
                playback_key_colors_enabled: false,
                loop_range: Some(grid::LoopRange::new()),
                notes_changed: false,
                pending_drag_state: None,
                velocity_panel: VelocityPanel::new(),
                selection_box_anim: Cell::new(None),
                cached_selection_bounds: Cell::new(None),
                context_menu: crate::context_menu::PianoRollContextMenuState::default(),
                selected_bounds: Cell::new(None),
                playback_scan_state: crate::impls::PlaybackScanState::default(),
                ctrl_pressed: false,
            }
        })
    }

    /// 收集编辑器各组件的内存占用快照
    pub fn memory_breakdown(&self) -> EditorMemory {
        let d = &self.editor_state.data;
        let note_size = std::mem::size_of::<Note>();

        // 2026-08 单一权威源：`notes` / `track_notes` 缓存已删除，
        // 音符统计全部从 document（唯一权威）读取。
        let notes_len = d.current_track_note_count();
        let notes_bytes = notes_len * note_size;

        // 全量音符统计（document 各轨之和）
        let track_notes_entries = d.document.as_ref().map(|doc| doc.notes.len()).unwrap_or(0);
        let mut track_notes_count = 0usize;
        let mut track_notes_bytes = 0usize;
        if let Some(doc) = &d.document {
            for notes in &doc.notes {
                track_notes_count += notes.len();
                track_notes_bytes += notes.len() * note_size;
            }
        }

        // document notes (NoteEvent=16B, (u32,f32)=8B)
        let doc_is_some = d.document.is_some();
        let doc_notes_cap: usize = d
            .document
            .as_ref()
            .map(|d| d.notes.iter().map(|v| v.capacity()).sum())
            .unwrap_or(0);
        let doc_events_bytes = d
            .document
            .as_ref()
            .map(|doc| {
                doc_notes_cap * std::mem::size_of::<lumino_midi_loader::NoteEvent>() // NoteEvent
                    + doc.tempo_changes.capacity() * 8 // (u32, f32)
            })
            .unwrap_or(0);

        tracing::info!(
            "[MEMORY_DEBUG] document={}, notes_cap={}, notes_len={}, track_notes_entries={}, track_notes_count={}",
            doc_is_some,
            doc_notes_cap,
            notes_len,
            track_notes_entries,
            track_notes_count,
        );

        EditorMemory {
            notes_bytes,
            track_notes_count,
            track_notes_bytes,
            track_notes_entries,
            document_events_bytes: doc_events_bytes,
        }
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        self.remote_cursors.insert(
            user_id.to_string(),
            (
                iced_core::Point::new(x, y),
                color.to_string(),
                username.to_string(),
            ),
        );
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
    }

    /// 记录 Ctrl 键按下状态（窗口级 `CtrlKeyChanged` 消息驱动）
    ///
    /// ruler/键盘区 Ctrl+滚轮缩放依赖此字段，走 host 层可靠通道，
    /// 避免 canvas 内 `ModifiersChanged` 事件因焦点问题不送达。
    pub fn set_ctrl_pressed(&mut self, pressed: bool) {
        self.ctrl_pressed = pressed;
    }

    /// 当前 Ctrl 键是否按下（可靠通道）
    pub fn ctrl_pressed(&self) -> bool {
        self.ctrl_pressed
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let actions = self.editor_state.interaction.take_audio_actions();
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }

    /// Push current state to history
    pub fn push_history(&mut self) {
        self.editor_state.data.push_history();
    }

    /// 检查当前是否处于编辑状态（拦截 Undo/Redo/Save/Play/Export 用）
    ///
    /// 返回 `true` 当用户正在进行音符编辑（拖动/绘制/调整大小），
    /// 或有未提交的批量拖动（pending_drag_state）。
    pub fn is_editing(&self) -> bool {
        use crate::EditState;
        self.pending_drag_state.is_some()
            || self.editor_state.data.has_pending_commit()
            || matches!(
                self.editor_state.interaction.edit_state,
                EditState::Dragging { .. }
                    | EditState::DraggingSelection { .. }
                    | EditState::PendingDrag { .. }
                    | EditState::Drawing { .. }
                    | EditState::ResizingStart { .. }
                    | EditState::ResizingEnd { .. }
                    | EditState::ResizingSelectionStart { .. }
                    | EditState::ResizingSelectionEnd { .. }
            )
    }

    /// 是否有未提交的批量拖动（pending commit 状态）
    pub fn has_pending_drag(&self) -> bool {
        self.pending_drag_state.is_some() || self.editor_state.data.has_pending_commit()
    }

    /// 丢弃未提交的批量拖动（不含异步提交中的 pending commit）
    ///
    /// 图片转 MIDI √ 写入后调用：写入改变了 document 音符数量与顺序，
    /// 残留的 `pending_drag_state.selected` 是写入前的全局索引，继续保留会
    /// 导致后续 resize/拖动按旧索引取位、误伤周围音符（连带改变长度）。
    pub fn clear_pending_drag(&mut self) {
        self.pending_drag_state = None;
    }

    /// 提交 pending 批量拖动到 document（音符唯一权威）
    ///
    /// 在以下场景调用：
    /// - 用户点击空白处取消框选时
    /// - `commit_current_edit()` 自动提交（Save/Play/Export 前的 fallback）
    ///
    /// 返回 `true` 表示已启动异步提交。如果 pending_drag_state 为 None 或 delta 为零，
    /// 返回 false。
    ///
    /// **异步提交**：实际数据更新在后台线程执行，UI 层需每帧调用 `poll_async_commit`
    /// 获取结果。pending_drag_state 会保留到异步提交完成，以维持 ghost 视觉位置。
    pub fn commit_pending_drag(&mut self) -> bool {
        crate::puffin_profiler::commit_pending_drag();
        let Some(drag_state) = self.pending_drag_state.as_ref() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending drag delta 为零，跳过提交");
            self.pending_drag_state = None;
            return false;
        }
        // 避免重复提交
        if self.editor_state.data.has_pending_commit() {
            return true;
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        let ops = self.editor_state.data.move_ops_from_drag_state(drag_state);
        match self.editor_state.data.apply_move_ops_async(ops, max_key) {
            Ok(true) => {
                tracing::info!("Editor: 已启动 pending 批量拖动异步提交");
                true
            }
            Ok(false) => {
                self.pending_drag_state = None;
                false
            }
            Err(e) => {
                tracing::error!("Editor: 异步提交 MoveOp 失败: {}", e);
                self.pending_drag_state = None;
                false
            }
        }
    }

    /// 轮询异步提交结果
    ///
    /// 若完成：应用结果到 data，清空 pending_drag_state，并返回修改数。
    /// 若未完成：返回 `None`。
    pub fn poll_async_commit(&mut self) -> Option<usize> {
        crate::puffin_profiler::poll_async_commit();
        match self.editor_state.data.poll_async_commit() {
            Some(Ok(modified)) => {
                if modified > 0 {
                    self.mark_notes_changed();
                    tracing::info!("Editor: 异步提交完成 - 修改 {} 个音符", modified);
                }
                self.pending_drag_state = None;
                Some(modified)
            }
            Some(Err(e)) => {
                tracing::error!("Editor: 异步提交结果处理失败: {}", e);
                self.pending_drag_state = None;
                None
            }
            None => None,
        }
    }

    /// 阻塞等待所有异步提交完成
    ///
    /// 用于 Save/Play/Export 等需要立即可用数据的场景。
    /// 返回 `true` 表示有数据被修改。
    pub fn drain_async_commit(&mut self) -> bool {
        let mut any_modified = false;
        while self.editor_state.data.has_pending_commit() {
            match self.editor_state.data.poll_async_commit() {
                Some(Ok(modified)) => {
                    if modified > 0 {
                        self.mark_notes_changed();
                        any_modified = true;
                    }
                    self.pending_drag_state = None;
                }
                Some(Err(e)) => {
                    tracing::error!("Editor: drain 异步提交失败: {}", e);
                    self.pending_drag_state = None;
                }
                None => {
                    // 避免忙等：让出时间片
                    std::thread::yield_now();
                }
            }
        }
        any_modified
    }

    /// 提交当前编辑（Save/Play/Export 前自动调用）
    ///
    /// 如果用户正在编辑（ghost 拖动/绘制/调整大小），先提交到 document。
    /// 等价于"模拟用户松开鼠标"。返回 `true` 表示有数据被提交。
    ///
    /// **延迟提交方案**：`DraggingSelection` 的 `handle_released` 只把 delta 保存到
    /// `pending_drag_state`，不真正 apply。这里必须再调 `commit_pending_drag`，
    /// 否则 Save/Play/Export 时数据会丢失。
    ///
    /// **异步提交**：Save/Play/Export 前会调用 `drain_async_commit` 确保数据已落盘。
    pub fn commit_current_edit(&mut self) -> bool {
        if !self.is_editing() {
            return false;
        }
        let before = self.editor_state.data.current_track_note_count();
        // handle_released: Dragging/Drawing/Resizing 直接 apply；DraggingSelection 保存到 pending
        self.handle_released();
        // 延迟提交方案：如果 handle_released 产生了 pending_drag_state，启动异步提交
        let pending_committed = self.commit_pending_drag();
        // Save/Play/Export 前必须等待异步提交完成
        let drained = self.drain_async_commit();
        let after = self.editor_state.data.current_track_note_count();
        tracing::debug!(
            "Editor: 自动提交编辑（commit_current_edit），notes len {} -> {}, pending_committed={}, drained={}",
            before,
            after,
            pending_committed,
            drained
        );
        true
    }

    /// Undo the last action
    ///
    /// **拦截策略**：如果当前正在主动编辑（Dragging/Drawing/Resizing 等），拦截并返回 `false`。
    /// 若存在未完成的 pending 批量拖动（异步提交中），先阻塞等待其完成，再执行 undo，
    /// 否则 undo 会操作旧数据并提示"未退出编辑状态"。
    pub fn undo(&mut self) -> bool {
        // 先排空异步提交，避免 pending 状态阻塞 undo
        if self.has_pending_drag() {
            tracing::info!("Editor: Undo 前发现 pending 异步提交，先 drain");
            self.drain_async_commit();
        }
        if self.is_editing() {
            tracing::warn!("Editor: 拦截 Undo —— 当前正在编辑，请先完成当前编辑");
            return false;
        }
        if self.editor_state.data.undo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("Editor: Undo 成功");
            true
        } else {
            tracing::info!("Editor: 没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    ///
    /// **拦截策略**：同 `undo()`，编辑中拦截；存在 pending 时先 drain。
    pub fn redo(&mut self) -> bool {
        if self.has_pending_drag() {
            tracing::info!("Editor: Redo 前发现 pending 异步提交，先 drain");
            self.drain_async_commit();
        }
        if self.is_editing() {
            tracing::warn!("Editor: 拦截 Redo —— 当前正在编辑，请先完成当前编辑");
            return false;
        }
        if self.editor_state.data.redo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("Editor: Redo 成功");
            true
        } else {
            tracing::info!("Editor: 没有可重做的操作");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.editor_state.data.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.editor_state.data.history.can_redo()
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctrl_pressed_defaults_false() {
        // 可靠通道（窗口级 CtrlKeyChanged）默认未按下，与 canvas 内状态互相兜底
        let editor = Editor::new();
        assert!(!editor.ctrl_pressed());
    }

    #[test]
    fn test_ctrl_pressed_set_and_get() {
        let mut editor = Editor::new();
        editor.set_ctrl_pressed(true);
        assert!(editor.ctrl_pressed());
        editor.set_ctrl_pressed(false);
        assert!(!editor.ctrl_pressed());
    }
}
