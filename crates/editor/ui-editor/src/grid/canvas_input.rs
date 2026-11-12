//! 与 UI 框架无关的画布输入分派。
//!
//! iced `Grid` program 曾在 `update()` 里把鼠标事件翻译成编辑动作；egui 独占
//! 整窗后由宿主（winit 事件）直接调用本模块。`program_impl` 只负责把 iced
//! 事件映射成 [`CanvasEvent`] 再转调这里，两条路径共享同一份语义，避免行为漂移。

use lumino_message::{ContextMenuTarget, PianoRollContextMenuAction, Tool};
use lumino_ui_core::Message;
use lumino_ui_core::message::{EditorAction, LoopRangeAction, Point2};

use super::PianoRollGrid;
use super::state::GridInteractionState;
use crate::Editor;

pub mod vertical;

/// 画布鼠标按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasButton {
    /// 左键。
    Left,
    /// 右键。
    Right,
}

/// 一帧画布输入事件（坐标均相对画布左上角）。
#[derive(Debug, Clone, Copy)]
pub enum CanvasEvent {
    /// 按下（`local` 必须在画布内）。
    Press {
        /// 画布局部坐标。
        local: (f32, f32),
        /// 按钮。
        button: CanvasButton,
    },
    /// 指针移动。
    Move {
        /// 画布局部坐标。
        local: (f32, f32),
    },
    /// 左键释放。
    Release,
    /// 滚轮（`precise = true` 表示触控板像素级增量）。
    Wheel {
        /// 画布局部坐标。
        local: (f32, f32),
        /// 水平增量（Lines 或 Pixels，由 `precise` 决定）。
        delta_x: f32,
        /// 垂直增量。
        delta_y: f32,
        /// 是否像素级增量。
        precise: bool,
    },
    /// 修饰键变化。
    Modifiers {
        /// Shift。
        shift: bool,
        /// Ctrl。
        ctrl: bool,
    },
    /// 窗口失焦（清掉可能收不到 Released 的修饰键）。
    FocusLost,
}

/// 分派一条画布输入，返回要发布的消息。
pub fn dispatch(
    editor: &Editor,
    state: &mut GridInteractionState,
    event: CanvasEvent,
) -> Option<Message> {
    let grid = PianoRollGrid::new(editor);
    let local = |local: (f32, f32)| iced_core::Point::new(local.0, local.1);

    match event {
        CanvasEvent::Modifiers { shift, ctrl } => {
            state.shift_pressed = shift;
            state.control_pressed = ctrl;
            None
        }
        CanvasEvent::FocusLost => {
            state.shift_pressed = false;
            state.control_pressed = false;
            None
        }
        CanvasEvent::Press { local: pos, button } => match button {
            CanvasButton::Left => {
                let local_pos = local(pos);
                // ShapeStore 列表 / 区域着色优先于普通画布交互。
                if let Some(action) = crate::grid::shape_list_box::press_action(editor, local_pos) {
                    return action.map(Message::EditorAction);
                }
                if let Some(action) = crate::grid::area_paint_box::press_action(editor, local_pos) {
                    return action.map(Message::EditorAction);
                }
                grid.handle_left_press(state, local_pos)
            }
            CanvasButton::Right => {
                let local_pos = local(pos);
                if editor.shape_list_open {
                    return Some(Message::EditorAction(EditorAction::ShapeListClose));
                }
                // 区域着色：右键写 colour=0（清除该区域），不打开上下文菜单。
                if let Some(action) = crate::grid::area_paint_box::clear_action(editor, local_pos) {
                    return Some(Message::EditorAction(action));
                }
                // 仅在有效钢琴卷帘区域内打开右键菜单
                if editor.is_inside_canvas(local_pos) {
                    // 待确认镜像：选择工具与它的 owner 工具都可直接右键；
                    // 已确认图形只在对齐的鼠标工具下开放。
                    let target = match editor.drawn_shape_pending_at_screen(local_pos) {
                        Some(true) => editor
                            .pending_ref_at_screen(local_pos)
                            .filter(|pending| editor.pending_menu_allowed(*pending))
                            .map_or(ContextMenuTarget::Notes, |_| {
                                ContextMenuTarget::PendingDrawnShape
                            }),
                        Some(false) if editor.current_tool() == Tool::ShapeSelect => {
                            ContextMenuTarget::DrawnShape
                        }
                        _ => ContextMenuTarget::Notes,
                    };
                    return Some(Message::PianoRollContextMenu(
                        PianoRollContextMenuAction::Open {
                            position: Point2::new(local_pos.x, local_pos.y),
                            target,
                        },
                    ));
                }
                None
            }
        },
        CanvasEvent::Move { local: pos } => {
            let local_pos = local(pos);
            // 更新框选框平滑动画
            editor.update_selection_box_animation(Some(local_pos));

            if state.is_loop_dragging {
                return Some(Message::LoopRange(LoopRangeAction::RulerMoved {
                    x: local_pos.x,
                    y: local_pos.y,
                }));
            }
            if state.is_dragging_indicator {
                return Some(Message::EditorAction(EditorAction::IndicatorDragMove {
                    x: local_pos.x,
                }));
            }
            if editor.is_inside_canvas(local_pos) {
                return Some(Message::EditorAction(EditorAction::Moved(Point2::new(
                    local_pos.x,
                    local_pos.y,
                ))));
            }
            None
        }
        CanvasEvent::Release => {
            // 清除框选框动画状态
            editor.selection_box_anim.set(None);

            if state.is_loop_dragging {
                state.is_loop_dragging = false;
                return Some(Message::LoopRange(LoopRangeAction::RulerReleased));
            }
            if state.is_dragging_indicator {
                state.is_dragging_indicator = false;
            }
            Some(Message::EditorAction(EditorAction::Released))
        }
        CanvasEvent::Wheel {
            local: pos,
            delta_x,
            delta_y,
            precise,
        } => {
            let local_pos = local(pos);
            let view = &editor.editor_state.view;
            // 标尺区域（顶部小节号栏）：滚轮缩放 X 轴
            if local_pos.y < view.ruler_height {
                return grid.handle_ruler_wheel_scroll(delta_x, delta_y, precise, local_pos);
            }
            // 键盘区域（左侧琴键栏）：滚轮缩放 Y 轴
            if local_pos.x < view.keyboard_width {
                return grid.handle_keyboard_wheel_scroll(delta_x, delta_y, precise, local_pos);
            }
            if editor.is_inside_canvas(local_pos) {
                return grid.handle_wheel_scroll(delta_x, delta_y, precise, state.shift_pressed);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;
    use crate::tests::test_helpers::seed_notes;
    use lumino_message::Tool;
    use lumino_ui_core::Message;
    use lumino_ui_core::message::EditorAction;

    /// 空白工程 + 固定画布几何（局部坐标 = 画布坐标）。
    fn editor() -> Editor {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
        editor.editor_state.view.snap_precision = 1.0;
        editor.set_canvas_size(iced_core::Point::new(4000.0, 4000.0));
        editor.set_canvas_offset(iced_core::Point::new(0.0, 0.0));
        editor
    }

    fn apply(editor: &mut Editor, message: Option<Message>) {
        if let Some(Message::EditorAction(action)) = message {
            editor.handle_action(action);
        }
    }

    /// 铅笔：按下 → 拖动 → 松开必须落下一个音符（与 iced 路径同语义）。
    #[test]
    fn press_drag_release_places_note_with_pencil() {
        let mut editor = editor();
        editor.set_tool(Tool::Pencil);
        let mut state = GridInteractionState::default();

        let press = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Press {
                local: (500.0, 300.0),
                button: CanvasButton::Left,
            },
        );
        assert!(
            matches!(
                press,
                Some(Message::EditorAction(EditorAction::Pressed { .. }))
            ),
            "铅笔按下应发 Pressed，实际：{press:?}"
        );
        apply(&mut editor, press);

        let moved = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Move {
                local: (700.0, 300.0),
            },
        );
        apply(&mut editor, moved);

        let released = dispatch(&editor, &mut state, CanvasEvent::Release);
        assert!(
            matches!(
                released,
                Some(Message::EditorAction(EditorAction::Released))
            ),
            "松开应发 Released，实际：{released:?}"
        );
        apply(&mut editor, released);

        assert_eq!(
            editor.editor_state.data.current_track_notes().len(),
            1,
            "按下-拖动-松开后应有且仅有 1 个音符"
        );
    }

    /// 右键：画布内打开卷帘上下文菜单（携带局部位标）。
    #[test]
    fn right_press_opens_context_menu() {
        let editor = editor();
        let mut state = GridInteractionState::default();

        let message = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Press {
                local: (600.0, 320.0),
                button: CanvasButton::Right,
            },
        );
        match message {
            Some(Message::PianoRollContextMenu(PianoRollContextMenuAction::Open {
                position,
                ..
            })) => {
                assert_eq!((position.x, position.y), (600.0, 320.0));
            }
            other => panic!("右键应打开上下文菜单，实际：{other:?}"),
        }
    }

    /// 滚轮：标尺区缩放 X，网格区滚动，键盘区缩放 Y。
    #[test]
    fn wheel_routes_by_region() {
        let editor = editor();
        let mut state = GridInteractionState::default();
        let view = &editor.editor_state.view;
        let ruler_y = view.ruler_height * 0.5;
        let keyboard_x = view.keyboard_width * 0.5;

        let zoom_x = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Wheel {
                local: (400.0, ruler_y),
                delta_x: 0.0,
                delta_y: 1.0,
                precise: false,
            },
        );
        assert!(
            matches!(zoom_x, Some(Message::ZoomXChanged { .. })),
            "标尺滚轮应缩放 X，实际：{zoom_x:?}"
        );

        let zoom_y = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Wheel {
                local: (keyboard_x, 300.0),
                delta_x: 0.0,
                delta_y: 1.0,
                precise: false,
            },
        );
        assert!(
            matches!(zoom_y, Some(Message::ZoomYChanged { .. })),
            "键盘列滚轮应缩放 Y，实际：{zoom_y:?}"
        );

        let scrolled = dispatch(
            &editor,
            &mut state,
            CanvasEvent::Wheel {
                local: (600.0, 320.0),
                delta_x: 0.0,
                delta_y: -1.0,
                precise: false,
            },
        );
        match scrolled {
            Some(Message::EditorAction(EditorAction::Scrolled { delta_y, .. })) => {
                assert!(delta_y < 0.0, "向下滚 delta_y 应为负：{delta_y}");
            }
            other => panic!("网格区滚轮应滚动视图，实际：{other:?}"),
        }
    }

    /// 修饰键与失焦：状态同步（失焦清空，避免 Shift 约束残留）。
    #[test]
    fn modifiers_and_focus_lost_update_state() {
        let editor = editor();
        let mut state = GridInteractionState::default();

        assert!(
            dispatch(
                &editor,
                &mut state,
                CanvasEvent::Modifiers {
                    shift: true,
                    ctrl: true,
                }
            )
            .is_none(),
            "修饰键事件不产生消息"
        );
        assert!(state.shift_pressed && state.control_pressed);

        assert!(
            dispatch(&editor, &mut state, CanvasEvent::FocusLost).is_none(),
            "失焦不产生消息"
        );
        assert!(!state.shift_pressed && !state.control_pressed);
    }
}
