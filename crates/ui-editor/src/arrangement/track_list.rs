//! 工程走带左侧音轨列表 Canvas —— 按 yinhe 风格绘制音轨名称和选中状态
//!
//! 与右侧走带 Canvas 共享 scroll_y，实现同步滚动。
//! 支持长按/拖动音轨行改变音轨顺序：按下注册拖拽候选并同步到 Sidebar
//! 统一计时，移动超阈值或长按超时后激活，释放时发出排序事件。
//! 绘制逻辑在 `track_list/draw.rs`，交互状态在 `track_list/state.rs`。

mod draw;
mod state;

use std::collections::HashSet;
use std::time::Instant;

use iced_core::{Color, Point, Rectangle, keyboard};
use iced_widget::canvas::{self, Geometry, Program};

use crate::{Message, Renderer, Theme};

pub use state::{MuteSoloButton, TrackDragState, TrackListState};

/// 未设置音轨颜色时左侧色块宽度（像素）
pub(crate) const BADGE_WIDTH: f32 = 8.0;
/// 文本左侧边距（像素）
pub(crate) const TEXT_MARGIN: f32 = 6.0;
/// 静音/独奏按钮尺寸（像素）
pub(crate) const BTN_SIZE: f32 = 18.0;
/// 静音/独奏按钮间距（像素）
pub(crate) const BTN_GAP: f32 = 2.0;

/// 工程走带左侧音轨列表 Canvas
pub struct TrackListCanvas {
    /// 音轨列表：(id, name)
    pub tracks: Vec<(usize, String)>,
    /// 每轨显示标签（如 A01），与 tracks 一一对应
    pub track_labels: Vec<String>,
    /// 每轨通道号（用于生成显示标签）
    pub track_channels: Vec<u8>,
    /// 每轨颜色标签
    pub track_colors: Vec<Option<Color>>,
    /// 每轨是否为主控音轨
    pub track_conductors: Vec<bool>,
    /// 每轨静音状态（初始值）
    pub track_muted: Vec<bool>,
    /// 每轨独奏状态（初始值）
    pub track_soloed: Vec<bool>,
    /// 当前选中的音轨 ID（单选兼容）
    pub selected_track: usize,
    /// 当前多选集合（外部传入的初始值）
    pub selected_tracks: HashSet<usize>,
    /// 范围选择锚点
    pub selection_anchor: Option<usize>,
    /// 垂直滚动偏移
    pub scroll_y: f32,
    /// 每轨高度
    pub track_height: f32,
    /// 总高度
    pub total_height: f32,
    /// 垂直缩放倍率（1.0 = 默认高度），Ctrl+滚轮垂直缩放时用于计算新 zoom_y
    pub zoom_y: f32,
    /// Ctrl 键按下状态（窗口级 CtrlKeyChanged 可靠通道，用于 Ctrl+滚轮垂直缩放）
    pub ctrl_pressed: bool,
    /// 外部长按激活的拖拽排序标记（Sidebar 计时，None 表示无拖拽）
    pub drag_active: bool,
}

impl TrackListCanvas {
    pub fn new(
        tracks: Vec<(usize, String)>,
        selected_track: usize,
        scroll_y: f32,
        track_height: f32,
        total_height: f32,
    ) -> Self {
        let count = tracks.len();
        Self {
            tracks,
            track_labels: vec![String::new(); count],
            track_channels: vec![0; count],
            track_colors: vec![None; count],
            track_conductors: vec![false; count],
            track_muted: vec![false; count],
            track_soloed: vec![false; count],
            selected_track,
            selected_tracks: HashSet::new(),
            selection_anchor: None,
            scroll_y,
            track_height,
            total_height,
            zoom_y: 1.0,
            ctrl_pressed: false,
            drag_active: false,
        }
    }

    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.track_labels = labels;
        self
    }

    pub fn with_channels(mut self, channels: Vec<u8>) -> Self {
        self.track_channels = channels;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Option<Color>>) -> Self {
        self.track_colors = colors;
        self
    }

    pub fn with_conductors(mut self, conductors: Vec<bool>) -> Self {
        self.track_conductors = conductors;
        self
    }

    pub fn with_mutes(mut self, muted: Vec<bool>) -> Self {
        self.track_muted = muted;
        self
    }

    pub fn with_solos(mut self, soloed: Vec<bool>) -> Self {
        self.track_soloed = soloed;
        self
    }

    pub fn with_selection(mut self, selected: HashSet<usize>, anchor: Option<usize>) -> Self {
        self.selected_tracks = selected;
        self.selection_anchor = anchor;
        self
    }

    /// 设置外部长按激活的拖拽排序标记（来自 Sidebar 统一计时）
    pub fn with_drag_active(mut self, active: bool) -> Self {
        self.drag_active = active;
        self
    }

    /// 设置垂直缩放倍率（1.0 = 默认高度），与右侧走带视口 zoom_y 保持一致
    pub fn with_zoom_y(mut self, zoom_y: f32) -> Self {
        self.zoom_y = zoom_y;
        self
    }

    /// 设置 Ctrl 键按下状态（窗口级 CtrlKeyChanged 可靠通道）
    pub fn with_ctrl_pressed(mut self, pressed: bool) -> Self {
        self.ctrl_pressed = pressed;
        self
    }

    fn ensure_state(&self, state: &mut TrackListState) {
        let count = self.tracks.len();
        state.track_muted.resize(count, false);
        state.track_soloed.resize(count, false);
        for (i, &v) in self.track_muted.iter().enumerate().take(count) {
            state.track_muted[i] = v;
        }
        for (i, &v) in self.track_soloed.iter().enumerate().take(count) {
            state.track_soloed[i] = v;
        }
        if state.selected_tracks.is_empty() {
            if !self.selected_tracks.is_empty() {
                state.selected_tracks.clone_from(&self.selected_tracks);
                state.selection_anchor = self.selection_anchor;
            } else {
                state.selected_tracks.insert(self.selected_track);
            }
        }
    }

    fn track_index_at_y(&self, y: f32) -> Option<usize> {
        let idx = (y / self.track_height) as usize;
        if idx < self.tracks.len() {
            Some(idx)
        } else {
            None
        }
    }

    fn is_mute_solo_hit(&self, pos: Point, idx: usize, canvas_w: f32) -> Option<MuteSoloButton> {
        if self.track_conductors.get(idx).copied().unwrap_or(false) {
            return None;
        }
        let track_y = idx as f32 * self.track_height - self.scroll_y;
        let total_btn_w = 2.0 * BTN_SIZE + BTN_GAP;
        let btn_x_start = canvas_w - total_btn_w - 6.0;
        let btn_y = track_y + (self.track_height - BTN_SIZE) * 0.5;
        if pos.x < btn_x_start
            || pos.x > btn_x_start + total_btn_w
            || pos.y < btn_y
            || pos.y > btn_y + BTN_SIZE
        {
            return None;
        }
        if pos.x < btn_x_start + BTN_SIZE {
            Some(MuteSoloButton::Mute)
        } else {
            Some(MuteSoloButton::Solo)
        }
    }

    /// 左键按下：执行选择逻辑，并注册拖拽排序候选
    fn handle_left_press(
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
    fn handle_left_release(
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
    fn handle_cursor_moved(
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
    fn clamp_drag_hover_to_conductor(&self, state: &mut TrackListState) {
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

impl Program<Message, Theme, Renderer> for TrackListCanvas {
    type State = TrackListState;

    fn update(
        &self,
        state: &mut TrackListState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        puffin::profile_function!();
        self.ensure_state(state);

        match event {
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            canvas::Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta }) => {
                use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};

                // Ctrl + 滚轮：垂直缩放（与钢琴卷帘键盘区一致：平滑步进 + 指针锚点）
                if self.ctrl_pressed {
                    let pos = cursor.position()?;
                    let factor = crate::zoom::zoom_factor_from_delta(delta)?;
                    return Some(canvas::Action::publish(Message::ArrangementZoomY {
                        zoom: self.zoom_y * factor,
                        fixed_ratio: crate::zoom::fixed_ratio_from_viewport(
                            pos.y - bounds.y,
                            0.0,
                            bounds.height,
                        ),
                    }));
                }

                let (_, dy) = match delta {
                    iced_core::mouse::ScrollDelta::Lines { x, y } => {
                        (x * SCROLL_LINES_SCALE, y * SCROLL_LINES_SCALE)
                    }
                    iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                let dy = dy.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
                Some(canvas::Action::publish(Message::ArrangementScrollY(
                    self.scroll_y - dy,
                )))
            }
            canvas::Event::Mouse(iced_core::mouse::Event::ButtonPressed(
                iced_core::mouse::Button::Left,
            )) => {
                if let Some(pos) = cursor.position() {
                    let local_pos = Point::new(pos.x - bounds.x, pos.y - bounds.y);
                    self.handle_left_press(state, local_pos, bounds.width)
                } else {
                    None
                }
            }
            canvas::Event::Mouse(iced_core::mouse::Event::ButtonReleased(
                iced_core::mouse::Button::Left,
            )) => {
                if let Some(pos) = cursor.position() {
                    let local_pos = Point::new(pos.x - bounds.x, pos.y - bounds.y);
                    self.handle_left_release(state, local_pos)
                } else {
                    state.take_drag();
                    None
                }
            }
            canvas::Event::Mouse(iced_core::mouse::Event::CursorMoved { position }) => {
                let local_pos = Point::new(position.x - bounds.x, position.y - bounds.y);
                self.handle_cursor_moved(state, local_pos)
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &TrackListState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        puffin::profile_function!();
        draw::draw(self, state, renderer, theme, bounds)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::Size;
    use iced_core::mouse::Cursor;

    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(0.0, 0.0), Size::new(160.0, 600.0))
    }

    fn canvas(ctrl: bool, zoom_y: f32) -> TrackListCanvas {
        TrackListCanvas::new(vec![(0, "A".into()), (1, "B".into())], 0, 0.0, 48.0, 96.0)
            .with_zoom_y(zoom_y)
            .with_ctrl_pressed(ctrl)
    }

    fn wheel(delta: iced_core::mouse::ScrollDelta) -> canvas::Event {
        canvas::Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta })
    }

    /// Ctrl+滚轮：垂直缩放，倍率按卷帘式平滑步进（每刻度 ±10%），
    /// 锚点比例为鼠标在列表内的纵向相对位置。
    #[test]
    fn test_ctrl_wheel_zooms_y_around_pointer() {
        let canvas = canvas(true, 1.0);
        let mut state = TrackListState::default();
        let cursor = Cursor::Available(Point::new(80.0, 300.0));
        let action = canvas
            .update(
                &mut state,
                &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
                bounds(),
                cursor,
            )
            .expect("Ctrl+滚轮应产生垂直缩放动作");
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::ArrangementZoomY { zoom, fixed_ratio }) => {
                // zoom_y(1.0) * 因子(1 + 1*0.1) = 1.1
                assert!((zoom - 1.1).abs() < f32::EPSILON, "zoom = {zoom}");
                // 鼠标位于列表纵向中点（300/600）
                assert!(
                    (fixed_ratio - 0.5).abs() < f32::EPSILON,
                    "fixed_ratio = {fixed_ratio}"
                );
            }
            other => panic!("Ctrl+滚轮音轨列表应发 ArrangementZoomY，实际为: {other:?}"),
        }
    }

    /// Ctrl+滚轮向下滚动（y < 0）→ 缩小
    #[test]
    fn test_ctrl_wheel_zooms_y_out() {
        let canvas = canvas(true, 2.0);
        let mut state = TrackListState::default();
        let cursor = Cursor::Available(Point::new(80.0, 150.0));
        let action = canvas
            .update(
                &mut state,
                &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }),
                bounds(),
                cursor,
            )
            .expect("Ctrl+滚轮应产生垂直缩放动作");
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::ArrangementZoomY { zoom, fixed_ratio }) => {
                // zoom_y(2.0) * 因子(1 - 1*0.1) = 1.8
                assert!((zoom - 1.8).abs() < f32::EPSILON, "zoom = {zoom}");
                assert!(
                    (fixed_ratio - 0.25).abs() < f32::EPSILON,
                    "fixed_ratio = {fixed_ratio}"
                );
            }
            other => panic!("Ctrl+滚轮音轨列表应发 ArrangementZoomY，实际为: {other:?}"),
        }
    }

    /// Ctrl+滚轮但增量为 0 → 无操作（避免旧式 dy<=0 误判缩小的缺陷）
    #[test]
    fn test_ctrl_wheel_zero_delta_is_noop() {
        let canvas = canvas(true, 1.0);
        let mut state = TrackListState::default();
        let cursor = Cursor::Available(Point::new(80.0, 300.0));
        assert!(
            canvas
                .update(
                    &mut state,
                    &wheel(iced_core::mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 }),
                    bounds(),
                    cursor
                )
                .is_none()
        );
    }

    /// 未按 Ctrl：普通滚轮仍为垂直滚动（既有行为不变）
    #[test]
    fn test_plain_wheel_still_scrolls_y() {
        let canvas = canvas(false, 1.0);
        let mut state = TrackListState::default();
        let cursor = Cursor::Available(Point::new(80.0, 300.0));
        let action = canvas
            .update(
                &mut state,
                &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
                bounds(),
                cursor,
            )
            .expect("普通滚轮应产生滚动动作");
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::ArrangementScrollY(y)) => {
                // scroll_y(0.0) - dy(1 * SCROLL_LINES_SCALE = 30) = -30（由 Root 钳制）
                assert!((y - -30.0).abs() < f32::EPSILON, "y = {y}");
            }
            other => panic!("普通滚轮音轨列表应发 ArrangementScrollY，实际为: {other:?}"),
        }
    }
}
