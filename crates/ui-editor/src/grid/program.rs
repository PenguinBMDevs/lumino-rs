//! 钢琴卷帘网格绘制程序

use super::state::GridInteractionState;
use crate::Editor;
use iced_core::Point;
use iced_widget::canvas::{self};
use lumino_ui_core::Message;
use lumino_ui_core::constants::editor as editor_constants;
use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};

/// 滚轮缩放步进系数：每滚动一个刻度（line），缩放倍率变化 10%
const ZOOM_WHEEL_STEP: f32 = 0.1;
/// Pixel 增量换算为刻度线的除数（与力度面板 Automation 缩放的换算保持一致）
const PIXEL_TO_LINE_SCALE: f32 = 50.0;

pub struct PianoRollGrid<'a> {
    pub editor: &'a Editor,
}

impl<'a> PianoRollGrid<'a> {
    pub fn new(editor: &'a Editor) -> Self {
        Self { editor }
    }

    pub(super) fn detect_double_click(
        &self,
        state: &mut GridInteractionState,
        local_pos: Point,
    ) -> bool {
        use editor_constants::*;

        let now = std::time::Instant::now();
        let is_double_click = state.last_click_pos.is_some_and(|last_pos| {
            let time_delta = now.duration_since(state.last_click_time).as_millis();
            let pos_delta =
                ((local_pos.x - last_pos.x).powi(2) + (local_pos.y - last_pos.y).powi(2)).sqrt();
            time_delta < DOUBLE_CLICK_TIME_MS && pos_delta < DOUBLE_CLICK_DISTANCE_PX
        });

        if !is_double_click {
            state.last_click_time = now;
            state.last_click_pos = Some(local_pos);
        }

        is_double_click
    }

    pub(super) fn handle_left_press(
        &self,
        state: &mut GridInteractionState,
        local_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::message::EditorAction;

        let v = &self.editor.editor_state.view;
        if local_pos.y < v.ruler_height && local_pos.x >= v.keyboard_width {
            // 先检测是否点击到循环区域
            if let Some(loop_range) = self.editor.loop_range.as_ref()
                && loop_range.enabled()
            {
                let loop_start_x =
                    loop_range.start_tick() * v.zoom_x - v.scroll_x + v.keyboard_width;
                let loop_end_x = loop_range.end_tick() * v.zoom_x - v.scroll_x + v.keyboard_width;
                if local_pos.x >= loop_start_x && local_pos.x <= loop_end_x {
                    state.is_loop_dragging = true;
                    return Some(canvas::Action::publish(Message::LoopRange(
                        lumino_ui_core::message::LoopRangeAction::RulerPressed {
                            x: local_pos.x,
                            y: local_pos.y,
                        },
                    )));
                }
            }

            // 固定指示线模式下：检测是否点击到指示线本身（支持拖拽）
            let asc = self.editor.editor_state.auto_scroll;
            if asc.mode == lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft {
                let indicator_screen_x = self
                    .editor
                    .get_playback_indicator_screen_x()
                    .unwrap_or(v.keyboard_width);
                let hit_margin = 8.0; // 点击容差
                if (local_pos.x - indicator_screen_x).abs() <= hit_margin {
                    state.is_dragging_indicator = true;
                    return Some(canvas::Action::publish(Message::EditorAction(
                        EditorAction::IndicatorDragStart { x: local_pos.x },
                    )));
                }
            }

            let tick = self.editor.x_to_tick(local_pos.x);
            let snapped_tick = self.editor.snap_tick(tick).max(0.0);
            return Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Scrubbed { tick: snapped_tick },
            )));
        }

        if self.detect_double_click(state, local_pos) {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::DoubleClicked(lumino_ui_core::message::Point2::new(
                    local_pos.x,
                    local_pos.y,
                )),
            )))
        } else {
            Some(canvas::Action::publish(Message::EditorAction(
                EditorAction::Pressed {
                    pos: lumino_ui_core::message::Point2::new(local_pos.x, local_pos.y),
                    shift: state.shift_pressed,
                },
            )))
        }
    }

    pub(super) fn handle_wheel_scroll(
        &self,
        delta: &iced_core::mouse::ScrollDelta,
        shift_pressed: bool,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::message::EditorAction;

        let (mut delta_x, mut delta_y) = Self::wheel_delta(delta);

        // Shift+滚轮：将垂直滚动转换为水平滚动
        // 部分平台已自动转换（delta_x 非零），未转换的平台需要手动处理
        // 注意取反：handle_scrolled 中 X 轴是 scroll_x + delta_x（直接加），
        // Y 轴是 scroll_y - delta_y（取反减），所以垂直→水平映射时必须取反符号。
        if shift_pressed && delta_x.abs() < f32::EPSILON {
            delta_x = -delta_y;
            delta_y = 0.0;
        }

        let delta_x = delta_x.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);

        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled { delta_x, delta_y },
        )))
    }

    /// 标尺区域（顶部小节号栏）滚轮处理：
    /// - Ctrl+滚轮：X 轴缩放，以鼠标位置为缩放锚点
    /// - 普通滚轮：水平平移（向上滚向右、向下滚向左移动视图）
    pub(super) fn handle_ruler_wheel_scroll(
        &self,
        delta: &iced_core::mouse::ScrollDelta,
        control_pressed: bool,
        local_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        use lumino_ui_core::message::EditorAction;

        if control_pressed {
            let factor = Self::zoom_factor_from_delta(delta)?;
            let view = &self.editor.editor_state.view;
            let canvas = &self.editor.editor_state.canvas;
            let viewport_w = (canvas.size_x - view.keyboard_width).max(0.0);
            return Some(canvas::Action::publish(Message::ZoomXChanged {
                zoom: view.zoom_x * factor,
                fixed_ratio: Self::fixed_ratio_from_viewport(
                    local_pos.x,
                    view.keyboard_width,
                    viewport_w,
                ),
            }));
        }

        // 普通滚轮：垂直增量映射为水平移动。向上滚（delta_y > 0）向右移动视图
        // （scroll_x 增大），向下滚（delta_y < 0）向左移动视图。
        let (_, delta_y) = Self::wheel_delta(delta);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        if delta_y.abs() < f32::EPSILON {
            return None;
        }
        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled {
                delta_x: delta_y,
                delta_y: 0.0,
            },
        )))
    }

    /// 键盘区域（左侧琴键栏）滚轮处理：
    /// Ctrl+滚轮：Y 轴缩放，以鼠标位置为缩放锚点；无 Ctrl 时无操作（保持原行为，不干扰标记）。
    pub(super) fn handle_keyboard_wheel_scroll(
        &self,
        delta: &iced_core::mouse::ScrollDelta,
        control_pressed: bool,
        local_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        if !control_pressed {
            return None;
        }
        let factor = Self::zoom_factor_from_delta(delta)?;
        let view = &self.editor.editor_state.view;
        let canvas = &self.editor.editor_state.canvas;
        let viewport_h = (canvas.size_y - view.ruler_height).max(0.0);
        Some(canvas::Action::publish(Message::ZoomYChanged {
            zoom: view.zoom_y * factor,
            fixed_ratio: Self::fixed_ratio_from_viewport(
                local_pos.y,
                view.ruler_height,
                viewport_h,
            ),
        }))
    }

    /// 解析滚轮增量（Lines 乘以滚动刻度系数，Pixels 原样）→ (delta_x, delta_y)
    fn wheel_delta(delta: &iced_core::mouse::ScrollDelta) -> (f32, f32) {
        match delta {
            iced_core::mouse::ScrollDelta::Lines { x, y } => {
                (*x * SCROLL_LINES_SCALE, *y * SCROLL_LINES_SCALE)
            }
            iced_core::mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
        }
    }

    /// 计算缩放因子：向上滚动（delta > 0）放大、向下滚动（delta < 0）缩小。
    /// 返回 None 表示无需缩放（增量为 0）。
    fn zoom_factor_from_delta(delta: &iced_core::mouse::ScrollDelta) -> Option<f32> {
        let line_delta = match delta {
            iced_core::mouse::ScrollDelta::Lines { y, .. } => *y,
            iced_core::mouse::ScrollDelta::Pixels { y, .. } => *y / PIXEL_TO_LINE_SCALE,
        };
        let step = line_delta.clamp(-1.0, 1.0);
        if step.abs() < f32::EPSILON {
            None
        } else {
            Some(1.0 + step * ZOOM_WHEEL_STEP)
        }
    }

    /// 计算鼠标在视口内的锚点比例（0.0 贴左/上，1.0 贴右/下）。
    /// 视口尺寸过小时回退到中心锚点（0.5）。
    fn fixed_ratio_from_viewport(position: f32, origin: f32, viewport_size: f32) -> f32 {
        if viewport_size > 0.0 {
            ((position - origin) / viewport_size).clamp(0.0, 1.0)
        } else {
            0.5
        }
    }

    /// 更新框选框的弹簧物理动画
    ///
    /// 委托给 Editor::update_selection_box_animation 执行。
    pub fn update_selection_box_animation(&self, mouse_pos: Option<Point>) {
        self.editor.update_selection_box_animation(mouse_pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::mouse::ScrollDelta;

    #[test]
    fn test_wheel_delta_lines_scale() {
        let (dx, dy) = PianoRollGrid::wheel_delta(&ScrollDelta::Lines { x: 2.0, y: -1.0 });
        assert_eq!(dx, 2.0 * SCROLL_LINES_SCALE);
        assert_eq!(dy, -SCROLL_LINES_SCALE);
    }

    #[test]
    fn test_wheel_delta_pixels_unchanged() {
        let (dx, dy) = PianoRollGrid::wheel_delta(&ScrollDelta::Pixels { x: 10.0, y: -25.0 });
        assert_eq!(dx, 10.0);
        assert_eq!(dy, -25.0);
    }

    #[test]
    fn test_zoom_factor_zoom_in_on_scroll_up() {
        // 向上滚动（y > 0）→ 放大
        let factor = PianoRollGrid::zoom_factor_from_delta(&ScrollDelta::Lines { x: 0.0, y: 1.0 });
        assert_eq!(factor, Some(1.1));
    }

    #[test]
    fn test_zoom_factor_zoom_out_on_scroll_down() {
        // 向下滚动（y < 0）→ 缩小
        let factor = PianoRollGrid::zoom_factor_from_delta(&ScrollDelta::Lines { x: 0.0, y: -1.0 });
        assert_eq!(factor, Some(0.9));
    }

    #[test]
    fn test_zoom_factor_zero_delta_returns_none() {
        assert_eq!(
            PianoRollGrid::zoom_factor_from_delta(&ScrollDelta::Lines { x: 0.0, y: 0.0 }),
            None
        );
    }

    #[test]
    fn test_zoom_factor_pixels_converted_and_clamped() {
        // 像素增量换算：y=50 → 1 个刻度
        let factor =
            PianoRollGrid::zoom_factor_from_delta(&ScrollDelta::Pixels { x: 0.0, y: 50.0 });
        assert_eq!(factor, Some(1.1));
        // 大增量被钳制为单个刻度（单步缩放，防止跳变）
        let factor =
            PianoRollGrid::zoom_factor_from_delta(&ScrollDelta::Pixels { x: 0.0, y: -500.0 });
        assert_eq!(factor, Some(0.9));
    }

    #[test]
    fn test_fixed_ratio_from_viewport() {
        // 锚点比例：视口 [60, 800) 内，60 → 0.0（贴左），430 → 0.5（中心），800 → 1.0（贴右）
        let ratio = PianoRollGrid::fixed_ratio_from_viewport(60.0, 60.0, 740.0);
        assert!((ratio - 0.0).abs() < f32::EPSILON);
        let ratio = PianoRollGrid::fixed_ratio_from_viewport(430.0, 60.0, 740.0);
        assert!((ratio - 0.5).abs() < f32::EPSILON);
        let ratio = PianoRollGrid::fixed_ratio_from_viewport(800.0, 60.0, 740.0);
        assert!((ratio - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fixed_ratio_from_viewport_degenerate_falls_back_to_center() {
        // 视口退化（尺寸为 0）时回退到中心锚点
        let ratio = PianoRollGrid::fixed_ratio_from_viewport(0.0, 0.0, 0.0);
        assert_eq!(ratio, 0.5);
    }

    #[test]
    fn test_keyboard_wheel_without_ctrl_is_noop() {
        // 键盘区域未按 Ctrl 时滚轮不产生任何动作（保持原有行为）
        let editor = Editor::new();
        let grid = PianoRollGrid::new(&editor);
        let action = grid.handle_keyboard_wheel_scroll(
            &ScrollDelta::Lines { x: 0.0, y: 1.0 },
            false,
            Point::new(30.0, 300.0),
        );
        assert!(action.is_none());
    }
}
