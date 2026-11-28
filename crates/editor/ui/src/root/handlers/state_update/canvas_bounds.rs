//! Canvas 尺寸/偏移更新后的视口钳正。
//!
//! 原臂体的「先写 canvas、再按卷帘方向钳正缩放与滚动」顺序逐行保持不变；
//! 两个方向的钳正逻辑按业务对象拆成独立函数，避免单个函数超长。

use crate::message::{Point2, Size2};
use crate::root::Root;
use lumino_editor_state::editor_state::EditorState;
use lumino_editor_state::editor_state::viewport::Viewport;

impl Root {
    /// 更新 Canvas 位置与尺寸，并按当前卷帘方向重新钳正视口
    pub(super) fn canvas_bounds(&mut self, offset: Point2, size: Size2) -> bool {
        self.editor
            .editor_state
            .canvas
            .set_offset(offset.x, offset.y);
        self.editor
            .editor_state
            .canvas
            .set_size(size.width, size.height);

        // 缩放不应出现空白区域：viewport 变化后，若琴键没填满 viewport，
        // 自动钳正 zoom_y，始终让 content 高度 ≥ viewport 高度。
        // 确保面板开关或 window resize 后不留空区。
        let state = &mut self.editor.editor_state;
        let is_vertical = state.is_vertical_roll;
        if is_vertical {
            clamp_vertical_viewport(state);
        } else {
            clamp_horizontal_viewport(state);
        }

        true
    }
}

/// 纵向卷帘：键盘横向铺满宽度，时间轴垂直
fn clamp_vertical_viewport(state: &mut EditorState) {
    let keyboard_h = state.view.keyboard_width;
    let vh_grid = (state.canvas.size_y - state.view.ruler_height - keyboard_h).max(0.0);
    let vw_keyboard = state.canvas.size_x.max(0.0);
    // 视口扩大后若内容未填满则放大铺满（保持 128/256 完整显示且无空白）
    {
        let total_w = state.view.visible_key_count as f32 * state.view.zoom_y;
        if total_w < vw_keyboard {
            let fill_zoom = (vw_keyboard / state.view.visible_key_count as f32).clamp(
                crate::constants::editor::zoom::MIN_ZOOM_Y,
                crate::constants::editor::zoom::MAX_ZOOM_Y,
            );
            if (fill_zoom - state.view.zoom_y).abs() > f32::EPSILON {
                state.view.zoom_y = fill_zoom;
                let total_ticks = state.view.total_ticks;
                Viewport::new(&mut state.view, &mut state.max_scroll)
                    .update_max_scroll(total_ticks);
            }
        }
    }
    // 钳制滚动：scroll_y 按宽度（键盘水平），scroll_x 按网格高度（时间垂直）
    let ms_y = (state.max_scroll.1 - vw_keyboard).max(0.0);
    state.view.scroll_y = state.view.scroll_y.min(ms_y).max(0.0);
    let ms_x = (state.max_scroll.0 - vh_grid).max(0.0);
    state.view.scroll_x = state.view.scroll_x.min(ms_x).max(0.0);
}

/// 横向钢琴卷帘：按视口高度/宽度钳正缩放与滚动
fn clamp_horizontal_viewport(state: &mut EditorState) {
    let vh = (state.canvas.size_y - state.view.ruler_height).max(0.0);
    let vw = (state.canvas.size_x - state.view.keyboard_width).max(0.0);
    {
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
                Viewport::new(&mut state.view, &mut state.max_scroll)
                    .update_max_scroll(total_ticks);
            }
        }
    }

    // 重新钳制滚动位置
    let ms_y = (state.max_scroll.1 - vh).max(0.0);
    state.view.scroll_y = state.view.scroll_y.min(ms_y);
    let ms_x = (state.max_scroll.0 - vw).max(0.0);
    state.view.scroll_x = state.view.scroll_x.min(ms_x);
}
