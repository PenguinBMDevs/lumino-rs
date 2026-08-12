//! 简单状态更新消息处理器
//!
//! 处理 `try_handle_simple_state` 中的剩余直接消息，
//! 包括滚动条、缩放、画布边界、动画帧、性能数据、MIDI 输入等。

use crate::message::Message;
use crate::root::Root;
use std::time::Instant;

impl Root {
    /// 处理简单的状态更新消息
    pub(crate) fn try_handle_simple_state(&mut self, msg: &Message) -> bool {
        match msg {
            Message::Progress(progress) => {
                if let Some((ref msg, p)) = *progress {
                    self.progress = Some((msg.clone(), p));
                } else {
                    self.progress = None;
                }
                true
            }
            Message::ScrollbarScrolled(x) => {
                self.editor.set_scroll_x(*x);
                true
            }
            Message::ScrollbarScrolledY(y) => {
                self.editor.set_scroll_y(*y);
                true
            }
            Message::ArrangementScrollX(x) => self.handle_arrangement_scroll_x(*x),
            Message::ArrangementScrollY(y) => self.handle_arrangement_scroll_y(*y),
            Message::ArrangementZoomX { zoom, fixed_ratio } => {
                self.handle_arrangement_zoom_x(*zoom, *fixed_ratio)
            }
            Message::ArrangementZoomY { zoom, fixed_ratio } => {
                self.handle_arrangement_zoom_y(*zoom, *fixed_ratio)
            }
            Message::ZoomXChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_x(*zoom, *fixed_ratio);
                true
            }
            Message::ZoomYChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_y(*zoom, *fixed_ratio);
                true
            }
            Message::CanvasBoundsChanged { offset, size } => {
                self.editor
                    .set_canvas_offset(iced_core::Point::new(offset.x, offset.y));
                self.editor
                    .set_canvas_size(iced_core::Point::new(size.width, size.height));

                // 缩放不应出现空白区域：viewport 变化后，若琴键没填满 viewport，
                // 自动钳正 zoom_y，始终让 content 高度 ≥ viewport 高度。
                // 确保面板开关或 window resize 后不留空区。
                let state = &mut self.editor.editor_state;
                let vh = (state.canvas.size_y - state.view.ruler_height).max(0.0);
                let th = state.view.visible_key_count as f32 * state.view.zoom_y;

                if th < vh {
                    // content 没填满 viewport → 调高 zoom
                    let fill_zoom = (vh / state.view.visible_key_count as f32).clamp(
                        crate::constants::editor::zoom::MIN_ZOOM_Y,
                        crate::constants::editor::zoom::MAX_ZOOM_Y,
                    );
                    if (fill_zoom - state.view.zoom_y).abs() > f32::EPSILON {
                        state.view.zoom_y = fill_zoom;
                        let total_ticks = state.view.total_ticks;
                        lumino_editor_state::editor_state::viewport::Viewport::new(
                            &mut state.view,
                            &mut state.max_scroll,
                        )
                        .update_max_scroll(total_ticks);
                    }
                }

                // 重新钳制滚动位置
                let ms_y = (state.max_scroll.1 - vh).max(0.0);
                state.view.scroll_y = state.view.scroll_y.min(ms_y);
                let vw = (state.canvas.size_x - state.view.keyboard_width).max(0.0);
                let ms_x = (state.max_scroll.0 - vw).max(0.0);
                state.view.scroll_x = state.view.scroll_x.min(ms_x);

                self.editor
                    .invalidate_caches(crate::editor::CacheInvalidation::KEYBOARD);
                true
            }
            Message::MenuStateChanged(is_open) => {
                self.state.is_menu_open = *is_open;
                true
            }
            Message::CtrlKeyChanged(pressed) => {
                self.toolbar.ctrl_pressed = *pressed;
                // 同步到 Editor：ruler/键盘区 Ctrl+滚轮缩放依赖此可靠通道
                // （iced canvas 内 ModifiersChanged 事件可能因焦点问题不送达）
                self.editor.set_ctrl_pressed(*pressed);
                true
            }
            Message::ShiftKeyChanged(pressed) => {
                self.toolbar.shift_pressed = *pressed;
                true
            }
            Message::ToggleSettings | Message::Null => true,
            Message::ModeToggled => self.handle_mode_toggle(),
            Message::AnimationTick => self.handle_animation_tick(),
            Message::VelocityPanelResize(height) => {
                self.visual.velocity_panel_height = *height;
                true
            }
            Message::PerfUpdate(data) => {
                self.statusbar.set_perf_data(*data);
                true
            }
            Message::MidiInputEvent { data } => {
                let data = data.clone();
                if let Ok(mut buf) = self.midi.input_buffer.lock() {
                    buf.push_back(data);
                }
                true
            }
            Message::ArrangementCursorSet(tick) => {
                let tick_f = *tick as f32;
                self.editor.playback_position = tick_f;
                if let Some(manager) = &mut self.playback.manager {
                    manager.seek(tick_f);
                }
                true
            }
            Message::ArrangementSelectionChanged(rect) => {
                let data = &mut self.editor.editor_state.data;
                data.arrange_selection.clear();
                if let Some((tick_start, tick_end, track_lo, track_hi)) = rect {
                    let ts = tick_start.max(0.0) as u32;
                    let te = tick_end.max(0.0) as u32;
                    if te > ts {
                        let track_lo_u16 = (*track_lo).min(u16::MAX as usize) as u16;
                        let track_hi_u16 = (*track_hi).min(u16::MAX as usize) as u16;
                        data.arrange_selection.add_rect_track(
                            ts,
                            te,
                            0,
                            127,
                            track_lo_u16,
                            track_hi_u16,
                        );
                    }
                }
                true
            }
            Message::ArrangementSelectionCleared => {
                self.editor.editor_state.data.arrange_selection.clear();
                true
            }
            Message::ArrangementMoveNotes {
                delta_ticks,
                delta_tracks,
            } => {
                let moved = self.editor.arrange_move_notes(*delta_ticks, *delta_tracks);
                if moved > 0 {
                    self.editor
                        .editor_state
                        .data
                        .arrange_selection
                        .offset_ticks(*delta_ticks);
                    self.editor
                        .editor_state
                        .data
                        .arrange_selection
                        .offset_tracks(*delta_tracks);
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            Message::ArrangementErase {
                tick_start,
                tick_end,
                track_lo,
                track_hi,
            } => {
                let deleted =
                    self.editor
                        .arrange_erase(*tick_start, *tick_end, *track_lo, *track_hi);
                if deleted > 0 {
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            Message::ArrangementRazor { tick, track } => {
                let split = self.editor.arrange_razor(*tick, *track);
                if split > 0 {
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            Message::ArrangementAddNote {
                track,
                tick,
                duration,
                key,
                velocity,
            } => {
                let track_count = self.sidebar.tracks.len();
                let added = self.editor.arrange_add_note(
                    track_count,
                    *track,
                    *tick,
                    *duration,
                    *key,
                    *velocity,
                );
                if added {
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            Message::ArrangementGhostNotesUpdated(notes) => {
                self.arrangement_view.ghost_notes = notes.clone();
                true
            }
            Message::ArrangementDragSelectionRect(rect) => {
                self.arrangement_view.drag_sel_rect = *rect;
                true
            }
            Message::ArrangementCopy => {
                self.editor.arrange_copy_selected_notes();
                true
            }
            Message::ArrangementPaste => {
                let pasted = self.editor.arrange_paste_notes_from_clipboard();
                if pasted {
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            Message::ArrangementCut => {
                let cut = self.editor.arrange_cut_selected_notes();
                if cut > 0 {
                    self.editor.editor_state.data.arrange_selection.clear();
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            Message::ArrangementDeleteSelection => {
                let deleted = self.editor.arrange_delete_selected_notes();
                if deleted > 0 {
                    self.editor.editor_state.data.arrange_selection.clear();
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
                true
            }
            _ => false,
        }
    }

    /// 处理动画 tick（切换动画 + 平滑滚动 + 弹簧物理）
    pub(crate) fn handle_animation_tick(&mut self) -> bool {
        let still_animating = self.state.toggle_animation.update();
        if !still_animating
            && self.state.toggle_animation.position >= 0.5
            && self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
        {
            self.state.current_mode = crate::titlebar::mode_toggle::AppMode::Waterfall;
        } else if !still_animating
            && self.state.toggle_animation.position < 0.5
            && self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Editor
        {
            self.state.current_mode = crate::titlebar::mode_toggle::AppMode::Editor;
        }

        let scroll_animating = {
            let state = &mut self.editor.editor_state;
            let v = &mut state.view;
            let (new_x, new_y, still_active) = v.smooth_scroll.update(v.scroll_x, v.scroll_y);
            // 钳制到有效滚动范围，防止平滑滚动超出键盘/音轨边界
            let max_y =
                (state.max_scroll.1 - (state.canvas.size_y - v.ruler_height).max(0.0)).max(0.0);
            let max_x =
                (state.max_scroll.0 - (state.canvas.size_x - v.keyboard_width).max(0.0)).max(0.0);
            v.scroll_x = new_x.clamp(0.0, max_x);
            v.scroll_y = new_y.clamp(0.0, max_y);
            still_active
        };
        if scroll_animating {
            self.editor
                .invalidate_caches(crate::editor::CacheInvalidation::ALL);
        }

        self.editor.update_selection_box_animation(None);

        // 音轨拖拽排序长按计时：候选按下后超过阈值自动激活拖拽
        self.sidebar.update_track_reorder_timer(Instant::now());

        // 轮询异步 MoveOp 提交结果（每帧一次，将后台线程结果应用到 data 并 push history）
        if self.editor.poll_async_commit().is_some() {
            self.editor
                .invalidate_caches(crate::editor::CacheInvalidation::ALL);
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }

        // 清理过期 Toast（每帧调用，低成本 O(N) retain）
        self.toast.cleanup_expired(Instant::now());

        true
    }
}
