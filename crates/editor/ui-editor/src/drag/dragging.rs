//! 单音符拖动相关逻辑：启动判定、状态迁移、松手提交
//!
//! 从 `drag.rs` 抽出，控制文件行数并保持单一职责。

use iced_core::Point;
use lumino_editor_state::DragState;
use lumino_ui_core::constants::editor::DRAG_START_THRESHOLD_RATIO;

use crate::{EditState, Editor};

impl Editor {
    /// 检查是否应从 PendingDrag 转换到 Dragging 状态
    pub(crate) fn try_transition_to_dragging(&mut self, pos: iced_core::Point) {
        crate::puffin_profiler::try_transition_to_dragging();
        let EditState::PendingDrag {
            note_index,
            start_pos,
            original_tick,
            original_key,
        } = self.editor_state.interaction.edit_state
        else {
            return;
        };

        if !self.should_start_dragging(pos, Point::new(start_pos.0, start_pos.1)) {
            return;
        }

        // ghost 方案：拖动期间数据不动，仅维护 DragState 偏移
        let note_count = self.editor_state.data.current_track_note_count();
        let drag_state = DragState::from_single(
            note_index,
            note_count,
            original_tick as i64,
            original_key as i16,
        );
        // 更新 editor_state
        self.editor_state.interaction.edit_state = EditState::Dragging {
            note_index,
            drag_state,
            last_played_key: original_key,
        };
    }

    fn should_start_dragging(&self, pos: iced_core::Point, start_pos: iced_core::Point) -> bool {
        let delta_x = pos.x - start_pos.x;
        let delta_y = pos.y - start_pos.y;
        let key_threshold = self.editor_state.view.zoom_y * DRAG_START_THRESHOLD_RATIO;
        let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
        let started = distance > key_threshold;
        if started {
            tracing::info!(
                "Editor: 拖动启动 - delta=({}, {}), distance={}, threshold={}",
                delta_x,
                delta_y,
                distance,
                key_threshold
            );
        }
        started
    }

    /// 完成单音符拖动（ghost 方案）
    ///
    /// 松手时一次性将 `drag_state.delta` 应用到 document（音符唯一权威），并发送 `LocalNoteMoved` 协作同步事件。
    /// 返回 `true` 表示音符位置确实发生了变化。
    pub(crate) fn finalize_dragging(&mut self, note_index: usize, drag_state: DragState) -> bool {
        crate::puffin_profiler::finalize_dragging();
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: 单音符拖动 delta 为零，跳过提交");
            return false;
        }

        // 读取原始位置（apply 前的状态，用于协作同步事件，按值）
        // 2026-09 去 ID：字段取自 document 当前轨权威 NoteEvent，无 ID
        let (original_tick, original_key, length, current_track) = {
            let current_track = self.editor_state.data.current_track;
            let notes = self.editor_state.data.track_notes(current_track);
            let Some(original_note) = notes.get(note_index) else {
                return false;
            };
            (
                original_note.start_tick as f32,
                original_note.key as u16,
                (original_note.end_tick - original_note.start_tick) as f32,
                current_track,
            )
        };

        let tick_offset = drag_state.delta_tick as f32;
        let key_offset = drag_state.delta_key;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // NoteMove 操作日志化：先捕获 MoveOp（记录 apply 前的原始值快照），再应用数据
        let ops = self
            .editor_state
            .data
            .move_ops_from_drag_state_with_max_key(&drag_state, max_key);

        // 按值捕获原始完整事件（供移动后按新值重选，无 ID）。
        let original_event = {
            let notes = self.editor_state.data.track_notes(current_track);
            notes.get(note_index).copied()
        };

        // ghost 方案：流式应用 delta 到 notes 与当前 track_notes 缓存
        let modified = self
            .editor_state
            .data
            .apply_drag_state_streaming(&drag_state, max_key);
        if modified == 0 {
            tracing::debug!("Editor: 单音符拖动未产生实际变更（snap 后 delta 为零）");
            return false;
        }
        // 移动后选中跟随新值（删加语义下旧值已不存在，通用旧值重映射必空；
        // 此处按新值窗口定位，无全扫）。
        self.selection_clear();
        if let Some(orig) = original_event {
            let new_tick = (orig.start_tick as i64 + drag_state.delta_tick).max(0) as u32;
            let new_key =
                (orig.key as i32 + drag_state.delta_key as i32).clamp(0, max_key as i32) as u8;
            let len = orig.end_tick.saturating_sub(orig.start_tick).max(1);
            let news = lumino_midi_loader::NoteEvent::new(
                new_tick,
                new_tick.saturating_add(len),
                new_key,
                orig.velocity,
                orig.channel,
            );
            // release_velocity 需保留（new() 归零，补回）。
            let mut news_full = news;
            news_full.release_velocity = orig.release_velocity;
            if let Some(idx) = self
                .editor_state
                .data
                .track_notes(current_track)
                .position_of(&news_full)
            {
                self.selection_insert(idx);
            }
        }

        if !ops.is_empty() {
            self.editor_state.data.push_move_op(ops);
        }

        tracing::info!(
            "Editor: 音符移动完成 - original=({}, {}), offset=({}, {})",
            original_tick,
            original_key,
            tick_offset,
            key_offset
        );
        // 协作同步关闭时跳过单音符移动广播（消费端未连接会短路丢弃，按值）。
        if self.editor_state.data.collab_sync_enabled() {
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::local_note_moved(
                    original_tick,
                    original_key,
                    length,
                    tick_offset,
                    key_offset,
                    current_track,
                ),
            ));
        }
        true
    }

    // 注：原 `finalize_selection_dragging` 已移除——延迟提交方案下，松手保存到
    // `pending_drag_state`，真正提交在 `commit_pending_drag`（点击空白处或
    // `commit_current_edit` 时触发）。详见 `interaction/released.rs`。
}
