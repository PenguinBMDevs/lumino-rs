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
                context_menu: crate::context_menu::PianoRollContextMenuState::default(),
                playback_scan_state: crate::impls::PlaybackScanState::default(),
            }
        })
    }

    /// 收集编辑器各组件的内存占用快照
    pub fn memory_breakdown(&self) -> EditorMemory {
        let d = &self.editor_state.data;
        let note_size = std::mem::size_of::<Note>();

        // editor.notes
        let notes_len = d.notes.len();
        let notes_bytes = notes_len * note_size;

        // track_notes
        let track_notes_entries = d.track_notes.len();
        let mut track_notes_count = 0usize;
        let mut track_notes_bytes = 0usize;
        for notes in d.track_notes.values() {
            track_notes_count += notes.len();
            track_notes_bytes += notes.len() * note_size;
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
        self.pending_drag_state.is_some()
    }

    /// 提交 pending 批量拖动到 `data.notes`
    ///
    /// 在以下场景调用：
    /// - 用户点击空白处取消框选时
    /// - `commit_current_edit()` 自动提交（Save/Play/Export 前的 fallback）
    ///
    /// 返回 `true` 表示有数据被提交。如果 pending_drag_state 为 None 或 delta 为零，返回 false。
    pub fn commit_pending_drag(&mut self) -> bool {
        let Some(drag_state) = self.pending_drag_state.take() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending drag delta 为零，跳过提交");
            return false;
        }
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let modified = self
            .editor_state
            .data
            .apply_drag_state_streaming(&drag_state, max_key);
        if modified > 0 {
            self.mark_notes_changed();
            tracing::info!("Editor: 提交 pending 批量拖动 - 修改 {} 个音符", modified);
        }
        modified > 0
    }

    /// 提交当前编辑（Save/Play/Export 前自动调用）
    ///
    /// 如果用户正在编辑（ghost 拖动/绘制/调整大小），先提交到 `data.notes`。
    /// 等价于"模拟用户松开鼠标"。返回 `true` 表示有数据被提交。
    ///
    /// **延迟提交方案**：`DraggingSelection` 的 `handle_released` 只把 delta 保存到
    /// `pending_drag_state`，不真正 apply。这里必须再调 `commit_pending_drag`，
    /// 否则 Save/Play/Export 时数据会丢失。
    pub fn commit_current_edit(&mut self) -> bool {
        if !self.is_editing() {
            return false;
        }
        let before = self.editor_state.data.notes.len();
        // handle_released: Dragging/Drawing/Resizing 直接 apply；DraggingSelection 保存到 pending
        self.handle_released();
        // 延迟提交方案：如果 handle_released 产生了 pending_drag_state，立即提交
        // （Save/Play/Export 前的 fallback，等价于"点击空白处"）
        let pending_committed = self.commit_pending_drag();
        let after = self.editor_state.data.notes.len();
        tracing::debug!(
            "Editor: 自动提交编辑（commit_current_edit），notes len {} -> {}, pending_committed={}",
            before,
            after,
            pending_committed
        );
        true
    }

    /// Undo the last action
    ///
    /// **拦截策略**：如果当前正在编辑（Dragging/Drawing/Resizing 等），拦截并返回 `false`。
    /// 调用方应提示用户"请先完成当前编辑"。
    pub fn undo(&mut self) -> bool {
        if self.is_editing() {
            tracing::warn!("Editor: 拦截 Undo —— 当前正在编辑，请先完成当前编辑");
            return false;
        }
        if self.editor_state.data.undo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            true
        } else {
            false
        }
    }

    /// Redo the last undone action
    ///
    /// **拦截策略**：同 `undo()`，编辑中拦截。
    pub fn redo(&mut self) -> bool {
        if self.is_editing() {
            tracing::warn!("Editor: 拦截 Redo —— 当前正在编辑，请先完成当前编辑");
            return false;
        }
        if self.editor_state.data.redo() {
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("重做操作成功");
            true
        } else {
            tracing::info!("没有可重做的操作");
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
