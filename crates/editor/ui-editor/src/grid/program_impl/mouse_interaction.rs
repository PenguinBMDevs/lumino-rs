//! Program trait `mouse_interaction` 实现逻辑 — 鼠标交互光标形态反馈

use crate::grid::GridInteractionState;
use crate::{EditState, Editor, HitType};
use iced_core::{Rectangle, mouse};
use lumino_message::Tool;

pub(crate) fn handle(
    editor: &Editor,
    state: &GridInteractionState,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> mouse::Interaction {
    puffin::profile_function!();

    // 图片转 MIDI 放置模式：光标反馈
    let i2m = &editor.editor_state.image_to_midi;
    if i2m.is_active() {
        use lumino_editor_state::I2mInteraction;
        return match i2m.interaction {
            I2mInteraction::Selecting => mouse::Interaction::Crosshair,
            I2mInteraction::Dragging => mouse::Interaction::Grabbing,
            I2mInteraction::StretchLeft | I2mInteraction::StretchRight => {
                mouse::Interaction::ResizingHorizontally
            }
            I2mInteraction::None => {
                if let Some(pos) = cursor.position() {
                    let local_pos = iced_core::Point::new(pos.x - bounds.x, pos.y - bounds.y);
                    if let Some(hit) = editor.hit_test_i2m_region(local_pos) {
                        return match hit {
                            crate::SelectionHitType::LeftEdge
                            | crate::SelectionHitType::RightEdge => {
                                mouse::Interaction::ResizingHorizontally
                            }
                            crate::SelectionHitType::Inside => mouse::Interaction::Pointer,
                        };
                    }
                }
                mouse::Interaction::Crosshair
            }
        };
    }

    if editor.current_tool() == Tool::Eraser {
        return mouse::Interaction::Crosshair;
    }

    // 曲线工具直线模式：悬停锚点/连线可拖动（Pointer），其余区域十字光标
    if editor.current_tool() == Tool::Curve {
        if let Some(local_pos) = state.position
            && editor.line_tool_hit_test(local_pos).is_some()
        {
            return mouse::Interaction::Pointer;
        }
        return mouse::Interaction::Crosshair;
    }

    let interaction = &editor.editor_state.interaction;
    match interaction.edit_state {
        EditState::Dragging { .. }
        | EditState::DraggingSelection { .. }
        | EditState::DraggingSelectionCopy { .. } => mouse::Interaction::Grabbing,
        EditState::PendingDrag { .. } => mouse::Interaction::Pointer,
        EditState::ResizingStart { .. }
        | EditState::ResizingEnd { .. }
        | EditState::ResizingSelectionStart { .. }
        | EditState::ResizingSelectionEnd { .. } => mouse::Interaction::ResizingHorizontally,
        EditState::Drawing { .. } => mouse::Interaction::Crosshair,
        EditState::Selecting { .. } => mouse::Interaction::Crosshair,
        EditState::Scrubbing => mouse::Interaction::Grabbing,
        EditState::Idle => {
            // 先检查是否悬停在循环区域手柄上
            {
                puffin::profile_scope!("loop_range_hit_test");
                if let Some(local_pos) = state.position {
                    let v = &editor.editor_state.view;
                    if local_pos.y < v.ruler_height
                        && local_pos.x >= v.keyboard_width
                        && let Some(loop_range) = editor.loop_range.as_ref()
                    {
                        let hit = loop_range.hit_test_at(
                            local_pos.x,
                            v.keyboard_width,
                            v.scroll_x,
                            v.zoom_x,
                        );
                        match hit {
                            crate::grid::LoopHitTest::StartHandle
                            | crate::grid::LoopHitTest::EndHandle => {
                                return mouse::Interaction::ResizingHorizontally;
                            }
                            crate::grid::LoopHitTest::Body => {
                                return mouse::Interaction::Pointer;
                            }
                            crate::grid::LoopHitTest::None => {}
                        }
                    }
                }
            }

            // 先检查是否悬停在选择框上
            {
                puffin::profile_scope!("selection_box_hit_test");
                if let Some(cursor_pos) = cursor.position() {
                    let local_pos =
                        iced_core::Point::new(cursor_pos.x - bounds.x, cursor_pos.y - bounds.y);
                    if let Some(sel_hit) = editor.hit_test_selection_box(local_pos) {
                        return match sel_hit {
                            crate::SelectionHitType::LeftEdge
                            | crate::SelectionHitType::RightEdge => {
                                mouse::Interaction::ResizingHorizontally
                            }
                            crate::SelectionHitType::Inside => mouse::Interaction::Pointer,
                        };
                    }
                }
            }

            // 固定指示线模式下：检测是否悬停在指示线上
            {
                puffin::profile_scope!("playback_indicator_hit_test");
                if editor.editor_state.auto_scroll.mode
                    == lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft
                    && let Some(local_pos) = state.position
                {
                    let v = &editor.editor_state.view;
                    if local_pos.y < v.ruler_height && local_pos.x >= v.keyboard_width {
                        let indicator_screen_x = editor
                            .get_playback_indicator_screen_x()
                            .unwrap_or(v.keyboard_width);
                        let hit_margin = 8.0;
                        if (local_pos.x - indicator_screen_x).abs() <= hit_margin {
                            return mouse::Interaction::ResizingHorizontally;
                        }
                    }
                }
            }

            match interaction.hover_state {
                Some((_, HitType::Start)) | Some((_, HitType::End)) => {
                    mouse::Interaction::ResizingHorizontally
                }
                Some((_, HitType::Middle)) => mouse::Interaction::Pointer,
                None => mouse::Interaction::default(),
            }
        }
    }
}
