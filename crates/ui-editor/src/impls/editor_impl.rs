//! Editor 核心方法
//!
//! 包含构造函数、内存统计、远程光标、音频动作、编辑状态、提交相关。
//! 撤销/重做（路径历史优先）在 `impls::editor_impl::history`。
//!
//! 注意：`update_playback_key_colors` 在 `impls::playback`，
//! `update_selection_box_animation` 在 `impls::selection_box_anim`。

mod history;

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
                pending_copy_drag_state: None,
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
    /// 或有未提交的批量拖动/批量复制（pending_drag_state / pending_copy_drag_state），
    /// 或正在进行曲线路径编辑（锚点/控制柄拖动）。
    pub fn is_editing(&self) -> bool {
        use crate::EditState;
        self.pending_drag_state.is_some()
            || self.pending_copy_drag_state.is_some()
            || self.editor_state.data.has_pending_commit()
            || self.editor_state.line_tool.interaction
                != lumino_editor_state::LineToolInteraction::None
            || matches!(
                self.editor_state.interaction.edit_state,
                EditState::Dragging { .. }
                    | EditState::DraggingSelection { .. }
                    | EditState::DraggingSelectionCopy { .. }
                    | EditState::PendingDrag { .. }
                    | EditState::Drawing { .. }
                    | EditState::ResizingStart { .. }
                    | EditState::ResizingEnd { .. }
                    | EditState::ResizingSelectionStart { .. }
                    | EditState::ResizingSelectionEnd { .. }
            )
    }

    /// 是否有未提交的批量拖动（pending commit 状态，含批量复制）
    pub fn has_pending_drag(&self) -> bool {
        self.pending_drag_state.is_some()
            || self.pending_copy_drag_state.is_some()
            || self.editor_state.data.has_pending_commit()
    }

    /// 丢弃未提交的批量拖动/批量复制（不含异步提交中的 pending commit）
    ///
    /// 图片转 MIDI √ 写入后调用：写入改变了 document 音符数量与顺序，
    /// 残留的 `pending_drag_state.selected` / `pending_copy_drag_state.selected`
    /// 是写入前的全局索引，继续保留会导致后续 resize/拖动按旧索引取位、
    /// 误伤周围音符（连带改变长度）。
    pub fn clear_pending_drag(&mut self) {
        self.pending_drag_state = None;
        self.pending_copy_drag_state = None;
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

    /// 提交 pending 批量复制到 document（音符唯一权威）
    ///
    /// 在以下场景调用：
    /// - 用户点击空白处取消框选时（`flush_pending_drag`）
    /// - `commit_current_edit()` 自动提交（Save/Play/Export 前的 fallback）
    ///
    /// 复制模式：将选中音符按 `pending_copy_drag_state.delta` 偏移后
    /// `batch_insert_notes` 写入内存层，并**只选中新插入的副本**
    /// （最新件框选；原件不再保留框选状态）。返回 `true` 表示已写入。
    /// 如果 pending 为 None 或 delta 为零，返回 false。
    pub fn commit_pending_copy(&mut self) -> bool {
        crate::puffin_profiler::commit_pending_copy();
        let Some(drag_state) = self.pending_copy_drag_state.as_ref() else {
            return false;
        };
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: pending 复制 delta 为零，跳过提交");
            self.pending_copy_drag_state = None;
            return false;
        }

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        // 构造副本音符列表（原始位置 + delta，tick/key clamp 到合法范围）
        let notes: Vec<Note> = drag_state
            .selected_indices_fast()
            .into_iter()
            .filter_map(|i| self.editor_state.data.get_note_view(i))
            .map(|n| {
                let tick = (n.tick + drag_state.delta_tick as f32).max(0.0);
                let key =
                    (n.key as i32 + drag_state.delta_key as i32).clamp(0, max_key as i32) as u16;
                Note::from_raw(tick, key, n.length, n.velocity, n.channel)
            })
            .collect();
        if notes.is_empty() {
            self.pending_copy_drag_state = None;
            return false;
        }

        // 与粘贴提交（commit_pasted_notes）一致：push history → batch insert → 选中新副本
        self.push_history();
        let inserted = self.editor_state.data.batch_insert_notes(&notes);
        self.editor_state.data.mark_current_track_changed();
        // 插入后同步 NoteStore（降级 no-op，保留调用兼容）
        self.editor_state.data.sync_note_store();
        // 插入位移了既有音符索引，旧选中索引全部失效：清空后按参数全等
        // 重选「副本」（最新件框选；副本 tick 可能落在现有音符之间，索引散布
        // 而非连续追加，不能按 start..start+inserted 连续区间选中）。
        self.selection_clear();
        self.select_notes_by_params(&notes);
        self.mark_notes_changed();
        self.pending_copy_drag_state = None;
        tracing::info!("Editor: 已复制 {} 个音符", inserted);
        true
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
    /// 否则 Save/Play/Export 时数据会丢失。`DraggingSelectionCopy` 同理
    /// （`pending_copy_drag_state` → `commit_pending_copy`）。
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
        // 复制模式：未写入的副本在保存/播放/导出前必须写入内存层。
        // 必须在 drain 之后（异步提交整轨替换音符，先插入副本会被覆盖）
        let copy_committed = self.commit_pending_copy();
        let after = self.editor_state.data.current_track_note_count();
        tracing::debug!(
            "Editor: 自动提交编辑（commit_current_edit），notes len {} -> {}, pending_committed={}, copy_committed={}, drained={}",
            before,
            after,
            pending_committed,
            copy_committed,
            drained
        );
        true
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
