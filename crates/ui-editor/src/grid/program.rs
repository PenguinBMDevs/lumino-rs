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

        // 曲线工具直线模式：两次点击为设置锚点，双击语义（删除音符）无意义，
        // 抑制双击检测，保证第二击正常走 Pressed 设置终点锚点。
        if self.editor.current_tool() != lumino_message::Tool::Curve
            && self.detect_double_click(state, local_pos)
        {
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
        // 注意：handle_scrolled 中 X/Y 两轴均为 scroll - delta（取反累加），
        // 因此垂直增量直接平移到水平轴即可保持语义一致：
        // 向下滚（delta_y < 0）→ delta_x < 0 → scroll_x 增大（视图右移）。
        if shift_pressed && delta_x.abs() < f32::EPSILON {
            delta_x = delta_y;
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
        // 注意取反：handle_scrolled 中两轴均为 scroll - delta（取反累加），
        // 发送 -delta_y 才能保持"向上滚 → 视图右移"的既有语义。
        let (_, delta_y) = Self::wheel_delta(delta);
        let delta_y = delta_y.clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA);
        if delta_y.abs() < f32::EPSILON {
            return None;
        }
        Some(canvas::Action::publish(Message::EditorAction(
            EditorAction::Scrolled {
                delta_x: -delta_y,
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

    /// 互斥隔离：标尺区 Ctrl+滚轮只产生缩放，绝不产生平移（Scrolled）
    #[test]
    fn test_ruler_wheel_ctrl_zooms_only() {
        let editor = Editor::new();
        let grid = PianoRollGrid::new(&editor);
        let action = grid
            .handle_ruler_wheel_scroll(
                &ScrollDelta::Lines { x: 0.0, y: 1.0 },
                true,
                Point::new(430.0, 20.0),
            )
            .expect("Ctrl+滚轮应产生动作");
        // 展开 Action 检查是否只有 ZoomXChanged（缩放与平移互斥，二者不会同时发出）
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::ZoomXChanged { zoom, fixed_ratio }) => {
                assert!(zoom > 0.0);
                assert!((0.0..=1.0).contains(&fixed_ratio));
            }
            other => panic!("Ctrl+滚轮标尺区应只发 ZoomXChanged，实际为: {other:?}"),
        }
    }

    /// 互斥隔离：标尺区无 Ctrl 滚轮只产生水平平移，绝不产生缩放
    #[test]
    fn test_ruler_wheel_without_ctrl_pans_only() {
        let editor = Editor::new();
        let grid = PianoRollGrid::new(&editor);
        let action = grid
            .handle_ruler_wheel_scroll(
                &ScrollDelta::Lines { x: 0.0, y: 1.0 },
                false,
                Point::new(430.0, 20.0),
            )
            .expect("普通滚轮应产生动作");
        let (message, _, _) = action.into_inner();
        match message {
            Some(Message::EditorAction(lumino_ui_core::message::EditorAction::Scrolled {
                delta_x,
                delta_y,
            })) => {
                // 向上滚 → 发送 delta_x < 0（handle_scrolled 取反后 scroll_x 增大、视图右移），且无垂直分量
                assert!(delta_x < 0.0);
                assert_eq!(delta_y, 0.0);
            }
            other => panic!("无 Ctrl 标尺区应只发 Scrolled，实际为: {other:?}"),
        }
    }

    /// 触控板水平滑动方向（回归测试：左滑/右滑应内容跟随手指）：
    /// 1) 网格区收到触控板像素增量 → 原样透传 delta_x（左滑为负）；
    /// 2) Editor 消费后 scroll_x 符号与 delta_x 相反（左滑 → scroll_x 增大）。
    #[test]
    fn test_grid_wheel_horizontal_swipe_follows_finger() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 1000.0;
        editor.editor_state.canvas.size_y = 500.0;
        let grid = PianoRollGrid::new(&editor);

        // 触控板左滑（像素增量 x < 0）
        let action = grid
            .handle_wheel_scroll(&ScrollDelta::Pixels { x: -100.0, y: 0.0 }, false)
            .expect("触控板水平滑动应产生动作");
        let (message, _, _) = action.into_inner();
        let (delta_x, delta_y) = match message {
            Some(Message::EditorAction(lumino_ui_core::message::EditorAction::Scrolled {
                delta_x,
                delta_y,
            })) => (delta_x, delta_y),
            other => panic!("网格区滚轮应发 Scrolled，实际为: {other:?}"),
        };
        assert!(delta_x < 0.0, "左滑应产生负 delta_x，实际={delta_x}");
        assert_eq!(delta_y, 0.0);

        // Editor 消费后：scroll_x 增大（内容跟随手指左移，显示更后音符）
        editor.handle_action(lumino_ui_core::message::EditorAction::Scrolled { delta_x, delta_y });
        assert!(
            editor.editor_state.view.smooth_scroll.target_x > 0.0,
            "左滑后 scroll_x 应增大，实际 target_x={}",
            editor.editor_state.view.smooth_scroll.target_x
        );
    }

    /// 互斥隔离：Ctrl 状态下普通平移分支绝不生效——
    /// Ctrl+滚轮与普通滚轮不可叠加，同一事件至多产生一种动作
    #[test]
    fn test_ruler_wheel_actions_are_exclusive() {
        let editor = Editor::new();
        let grid = PianoRollGrid::new(&editor);
        // 同一个滚轮事件，在 Ctrl 按下/松开两种状态下产生且只产生一种动作
        let ctrl_action = grid.handle_ruler_wheel_scroll(
            &ScrollDelta::Lines { x: 0.0, y: -1.0 },
            true,
            Point::new(430.0, 20.0),
        );
        let plain_action = grid.handle_ruler_wheel_scroll(
            &ScrollDelta::Lines { x: 0.0, y: -1.0 },
            false,
            Point::new(430.0, 20.0),
        );
        let ctrl_msg = ctrl_action.expect("Ctrl+滚轮应产生动作").into_inner().0;
        let plain_msg = plain_action.expect("普通滚轮应产生动作").into_inner().0;
        assert!(matches!(ctrl_msg, Some(Message::ZoomXChanged { .. })));
        assert!(matches!(
            plain_msg,
            Some(Message::EditorAction(
                lumino_ui_core::message::EditorAction::Scrolled { .. }
            ))
        ));
    }

    /// 斜向滚动（触控板双指对角线滑动）：单条事件携带双轴非零分量，
    /// 必须同时驱动 X 轴与 Y 轴滚动——这是「斜向滚动」的核心验收点。
    #[test]
    fn test_grid_wheel_diagonal_scrolls_both_axes() {
        let mut editor = Editor::new();
        editor.editor_state.canvas.size_x = 2000.0;
        editor.editor_state.canvas.size_y = 1000.0;
        // 制造足够内容使 max_scroll 双轴均 > 0（否则会被 clamp 到 0 导致滚动失效）
        editor.editor_state.view.total_ticks = 100000;
        {
            let state = &mut editor.editor_state;
            let total_ticks = state.view.total_ticks;
            lumino_editor_state::editor_state::viewport::Viewport::new(
                &mut state.view,
                &mut state.max_scroll,
            )
            .update_max_scroll(total_ticks);
        }
        let grid = PianoRollGrid::new(&editor);

        // 触控板左滑+上滑（像素增量 x<0, y<0）
        let action = grid
            .handle_wheel_scroll(
                &ScrollDelta::Pixels {
                    x: -100.0,
                    y: -50.0,
                },
                false,
            )
            .expect("斜向滚动应产生动作");
        let (message, _, _) = action.into_inner();
        let (delta_x, delta_y) = match message {
            Some(Message::EditorAction(lumino_ui_core::message::EditorAction::Scrolled {
                delta_x,
                delta_y,
            })) => (delta_x, delta_y),
            other => panic!("网格区滚轮应发 Scrolled，实际为: {other:?}"),
        };
        // 双轴分量都应非零
        assert!(delta_x < 0.0, "左滑应产生负 delta_x，实际={delta_x}");
        assert!(delta_y < 0.0, "上滑应产生负 delta_y，实际={delta_y}");

        // Editor 消费后：双轴目标位置都应变化（斜向滚动生效）
        editor.handle_action(lumino_ui_core::message::EditorAction::Scrolled { delta_x, delta_y });
        assert!(
            editor.editor_state.view.smooth_scroll.target_x > 0.0,
            "斜向滚动后 scroll_x 应增大，实际 target_x={}",
            editor.editor_state.view.smooth_scroll.target_x
        );
        assert!(
            editor.editor_state.view.smooth_scroll.target_y > 0.0,
            "斜向滚动后 scroll_y 应增大，实际 target_y={}",
            editor.editor_state.view.smooth_scroll.target_y
        );
    }
}
