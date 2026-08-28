//! 工程走带左侧音轨列表 —— 交互事件处理（选择/静音独奏/拖拽排序）
//!
//! 从 `track_list.rs` 抽出，控制文件行数并保持单一职责。

use std::time::Instant;

use iced_core::Point;
use iced_widget::canvas;

use super::TrackListCanvas;
use super::state::{MuteSoloButton, TrackListState};
use crate::Message;

impl TrackListCanvas {
    pub(super) fn ensure_state(&self, state: &mut TrackListState) {
        let count = self.tracks.len();
        state.track_muted.resize(count, false);
        state.track_soloed.resize(count, false);
        for (i, &v) in self.track_muted.iter().enumerate().take(count) {
            state.track_muted[i] = v;
        }
        for (i, &v) in self.track_soloed.iter().enumerate().take(count) {
            state.track_soloed[i] = v;
        }
        // 单一真相源：泳道高亮必须跟随模型当前轨（sidebar.selected_track，
        // 即 editor.current_track），而非被泳道点击置位后冻结的本地状态。
        // 一旦模型当前轨脱离本地集合（侧边栏切轨等外部路径），立即以模型为准
        // 重设，回收旧泳道高亮，杜绝"两个同时活跃的音轨"双高亮；泳道内单选/
        // 多选（集合已含模型当前轨）则保留本地交互状态。
        let model_current = self.selected_track;
        if !state.selected_tracks.contains(&model_current) {
            state.selected_tracks.clear();
            if self.selected_tracks.is_empty() {
                state.selected_tracks.insert(model_current);
            } else {
                state.selected_tracks.clone_from(&self.selected_tracks);
            }
            state.selection_anchor = None;
        }
    }

    pub(super) fn track_index_at_y(&self, y: f32) -> Option<usize> {
        let idx = (y / self.track_height) as usize;
        if idx < self.tracks.len() {
            Some(idx)
        } else {
            None
        }
    }

    pub(super) fn is_mute_solo_hit(
        &self,
        pos: Point,
        idx: usize,
        canvas_w: f32,
    ) -> Option<MuteSoloButton> {
        if self.track_conductors.get(idx).copied().unwrap_or(false) {
            return None;
        }
        let track_y = idx as f32 * self.track_height - self.scroll_y;
        let total_btn_w = 2.0 * super::BTN_SIZE + super::BTN_GAP;
        let btn_x_start = canvas_w - total_btn_w - 6.0;
        let btn_y = track_y + (self.track_height - super::BTN_SIZE) * 0.5;
        if pos.x < btn_x_start
            || pos.x > btn_x_start + total_btn_w
            || pos.y < btn_y
            || pos.y > btn_y + super::BTN_SIZE
        {
            return None;
        }
        if pos.x < btn_x_start + super::BTN_SIZE {
            Some(MuteSoloButton::Mute)
        } else {
            Some(MuteSoloButton::Solo)
        }
    }

    /// 左键按下：执行选择逻辑，并注册拖拽排序候选
    pub(super) fn handle_left_press(
        &self,
        state: &mut TrackListState,
        pos: Point,
        canvas_w: f32,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::constants::editor::{DOUBLE_CLICK_DISTANCE_PX, DOUBLE_CLICK_TIME_MS};

        let rel_y = pos.y + self.scroll_y;
        let idx = self.track_index_at_y(rel_y)?;
        let (track_id, _) = self.tracks.get(idx)?;
        let track_id = *track_id;

        let now = Instant::now();
        let is_double = state.last_click_pos.is_some_and(|last_pos| {
            let dt = now.duration_since(state.last_click_time).as_millis();
            let dist = ((pos.x - last_pos.x).powi(2) + (pos.y - last_pos.y).powi(2)).sqrt();
            dt < DOUBLE_CLICK_TIME_MS && dist < DOUBLE_CLICK_DISTANCE_PX
        });

        if is_double {
            return Some(canvas::Action::publish(
                lumino_ui_core::sidebar_event::Event::track_selected(track_id),
            ));
        }

        state.last_click_time = now;
        state.last_click_pos = Some(pos);

        if let Some(btn) = self.is_mute_solo_hit(pos, idx, canvas_w) {
            return Some(match btn {
                MuteSoloButton::Mute => {
                    if let Some(v) = state.track_muted.get_mut(idx) {
                        *v = !*v;
                    }
                    canvas::Action::publish(
                        lumino_ui_core::sidebar_event::Event::track_mute_toggled(track_id),
                    )
                }
                MuteSoloButton::Solo => {
                    if let Some(v) = state.track_soloed.get_mut(idx) {
                        *v = !*v;
                    }
                    canvas::Action::publish(
                        lumino_ui_core::sidebar_event::Event::track_solo_toggled(track_id),
                    )
                }
            });
        }

        let shift = state.modifiers.shift();

        if shift {
            if let Some(anchor_idx) = state.selection_anchor {
                let lo = anchor_idx.min(idx);
                let hi = anchor_idx.max(idx);
                state.selected_tracks.clear();
                for i in lo..=hi {
                    if let Some((id, _)) = self.tracks.get(i) {
                        state.selected_tracks.insert(*id);
                    }
                }
            } else {
                state.selected_tracks.clear();
                state.selected_tracks.insert(track_id);
            }
            state.selection_anchor = Some(idx);
        } else {
            state.selected_tracks.clear();
            state.selected_tracks.insert(track_id);
            state.selection_anchor = Some(idx);
        }

        // 注册拖拽排序候选（长按计时由 Sidebar 统一驱动）
        state.begin_drag(track_id, Point::new(pos.x, rel_y), idx);

        let ids: Vec<usize> = state.selected_tracks.iter().copied().collect();
        Some(canvas::Action::publish(Message::Batch(vec![
            lumino_ui_core::sidebar_event::Event::track_selected(track_id),
            lumino_ui_core::sidebar_event::Event::tracks_selected(ids),
            lumino_ui_core::sidebar_event::Event::track_reorder_started(track_id),
        ])))
    }

    /// 左键释放：若拖拽已激活则发出排序事件，否则仅结束候选
    pub(super) fn handle_left_release(
        &self,
        state: &mut TrackListState,
        _pos: Point,
    ) -> Option<canvas::Action<Message>> {
        let drag = state.take_drag()?;
        if !drag.active && !self.drag_active {
            return None; // 未激活 = 普通点击（选择已在按下时完成）
        }
        Some(canvas::Action::publish(
            lumino_ui_core::sidebar_event::Event::track_reorder_ended(Some(drag.hover_index)),
        ))
    }

    /// 鼠标移动：拖拽候选更新（激活 + 插入指示位置）
    pub(super) fn handle_cursor_moved(
        &self,
        state: &mut TrackListState,
        pos: Point,
    ) -> Option<canvas::Action<Message>> {
        state.drag.as_ref()?;
        let abs_pos = Point::new(pos.x, pos.y + self.scroll_y);
        let hover_changed = state.update_drag(abs_pos, self.track_height, self.tracks.len());
        self.clamp_drag_hover_to_conductor(state);
        if hover_changed {
            // 指示位置变化：空消息触发重绘
            Some(canvas::Action::publish(Message::Null))
        } else {
            None
        }
    }

    /// Conductor 首位不变量：插入指示不允许出现在 conductor 之前
    pub(super) fn clamp_drag_hover_to_conductor(&self, state: &mut TrackListState) {
        let Some(drag) = state.drag.as_mut() else {
            return;
        };
        if let Some(ci) = self.track_conductors.iter().position(|&c| c)
            && drag.hover_index <= ci
        {
            drag.hover_index = ci + 1;
        }
    }
}
