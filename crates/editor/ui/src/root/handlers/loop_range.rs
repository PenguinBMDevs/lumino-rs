//! 循环区域消息处理器
//!
//! 处理 LoopRangeAction 相关的消息（切换循环、设置范围、标尺交互等）。

use crate::message::{LoopRangeAction, Message};
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 循环区域消息处理器
pub struct LoopRangeHandler;

impl LoopRangeHandler {
    /// 创建一个循环区域消息处理器
    pub fn new() -> Self {
        Self
    }

    /// 处理循环区域动作
    pub fn handle_action(root: &mut Root, action: LoopRangeAction) {
        match action {
            LoopRangeAction::Toggle => {
                let enabled = if let Some(loop_range) = &mut root.editor.loop_range {
                    loop_range.toggle();
                    root.editor.ruler_cache.clear();
                    loop_range.enabled()
                } else {
                    false
                };
                Self::sync_loop_to_playback_state(root, enabled);
                tracing::info!("Root: 循环区域切换为 {}", enabled);
            }
            LoopRangeAction::SetRange(start, end) => {
                let (enabled, start_tick, end_tick) =
                    if let Some(loop_range) = &mut root.editor.loop_range {
                        loop_range.set_range(start, end);
                        if !loop_range.enabled() {
                            loop_range.enable();
                        }
                        root.editor.ruler_cache.clear();
                        (
                            loop_range.enabled(),
                            loop_range.start_tick(),
                            loop_range.end_tick(),
                        )
                    } else {
                        return;
                    };
                Self::sync_loop_to_playback_with_range(root, enabled, start_tick, end_tick);
                tracing::info!("Root: 循环范围设置为 [{:.2}, {:.2}]", start, end);
            }
            LoopRangeAction::Clear => {
                if let Some(loop_range) = &mut root.editor.loop_range {
                    loop_range.disable();
                    Self::sync_loop_to_playback_state(root, false);
                    root.editor.ruler_cache.clear();
                    tracing::info!("Root: 循环区域已清除");
                }
            }
            LoopRangeAction::RulerPressed { x, y: _ } => {
                if let Some(loop_range) = &mut root.editor.loop_range {
                    let view = &root.editor.editor_state.view;
                    let hit = loop_range.handle_mouse_press(
                        x,
                        view.keyboard_width,
                        view.scroll_x,
                        view.zoom_x,
                        view.ruler_height,
                        view.snap_precision,
                    );
                    if hit != crate::editor::grid::LoopHitTest::None {
                        root.editor.ruler_cache.clear();
                        tracing::debug!("Root: 标尺循环区域点击检测: {:?}", hit);
                    }
                }
            }
            LoopRangeAction::RulerMoved { x, y: _ } => {
                let should_sync = if let Some(loop_range) = &mut root.editor.loop_range {
                    if loop_range.is_dragging() {
                        let view = &root.editor.editor_state.view;
                        loop_range.handle_mouse_move(
                            x,
                            view.keyboard_width,
                            view.scroll_x,
                            view.zoom_x,
                            view.snap_precision,
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if should_sync {
                    let (enabled, start, end) = Self::get_loop_range_state(root);
                    Self::sync_loop_to_playback_with_range(root, enabled, start, end);
                    root.editor.ruler_cache.clear();
                }
            }
            LoopRangeAction::RulerReleased => {
                if let Some(loop_range) = &mut root.editor.loop_range
                    && loop_range.is_dragging()
                {
                    let start = loop_range.start_tick();
                    let end = loop_range.end_tick();
                    loop_range.handle_mouse_release();
                    root.editor.ruler_cache.clear();
                    tracing::debug!("Root: 循环拖拽释放，范围 [{:.2}, {:.2}]", start, end);
                }
            }
            LoopRangeAction::RulerDoubleClicked { x: _, y: _ } => {
                let enabled = if let Some(loop_range) = &mut root.editor.loop_range {
                    loop_range.toggle();
                    root.editor.ruler_cache.clear();
                    loop_range.enabled()
                } else {
                    false
                };
                Self::sync_loop_to_playback_state(root, enabled);
                tracing::info!("Root: 标尺双击切换循环为 {}", enabled);
            }
        }
    }

    fn get_loop_range_state(root: &Root) -> (bool, f32, f32) {
        root.editor
            .loop_range
            .as_ref()
            .map_or((false, 0.0, 0.0), |lr| {
                (lr.enabled(), lr.start_tick(), lr.end_tick())
            })
    }

    fn sync_loop_to_playback_state(root: &mut Root, enabled: bool) {
        if let Some(manager) = &mut root.playback.manager {
            manager.set_looping(enabled);
            if enabled {
                if let Some(lr) = &root.editor.loop_range {
                    manager.set_loop_range(lr.start_tick(), lr.end_tick());
                }
            } else {
                manager.clear_loop_range();
            }
        }
        root.toolbar.is_looping = enabled;
    }

    fn sync_loop_to_playback_with_range(root: &mut Root, enabled: bool, start: f32, end: f32) {
        if let Some(manager) = &mut root.playback.manager {
            manager.set_looping(enabled);
            if enabled {
                manager.set_loop_range(start, end);
            } else {
                manager.clear_loop_range();
            }
        }
        root.toolbar.is_looping = enabled;
    }
}

impl Default for LoopRangeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for LoopRangeHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::LoopRange(action) => {
                Self::handle_action(root, action);
                None
            }
            other => Some(other),
        }
    }
}
