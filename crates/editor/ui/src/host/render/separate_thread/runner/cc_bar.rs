//! CC 柱状条实例构建 — build_cc_bar_instances

use crate::host::Host;
use lumino_gfx::{CcBarColors, CcBarData, CcBarViewParams};

impl Host {
    /// 构建 CC 柱状条实例（背景/网格/中心线）
    pub(crate) fn build_cc_bar_instances(&self) -> Vec<lumino_gfx::CcBarInstance> {
        use crate::editor::grid::theme::ThemeExt;
        use crate::editor::velocity::EditMode;

        let editor = &self.root.editor;
        let panel = &editor.velocity_panel;

        // 根据编辑模式获取数据点和模式参数
        let (is_bend, is_velocity, cc_number) = match panel.edit_mode {
            EditMode::Bend => (true, false, 0u8),
            EditMode::Cc(n) => (false, false, n),
            EditMode::Velocity => (false, true, 0u8),
            EditMode::Tempo => (false, false, 0u8),
        };

        // Velocity 模式从 notes 获取力度点
        // 2026-08 单一权威源：从 document 读取（track_notes 缓存已删除）
        let velocity_points = if is_velocity {
            crate::editor::velocity::VelocityPanel::build_velocity_points(
                editor.editor_state.data.current_track_notes(),
            )
        } else {
            Vec::new()
        };

        // CC/Bend 模式从 automation_lanes 获取控制点
        let (cc_points, bend_points) = if is_bend {
            let bend_pts = crate::editor::velocity::VelocityPanel::build_bend_points(editor);
            (Vec::new(), bend_pts)
        } else if !is_velocity {
            let cc_pts = crate::editor::velocity::VelocityPanel::build_cc_points(editor, cc_number);
            (cc_pts, Vec::new())
        } else {
            (Vec::new(), Vec::new())
        };

        let track_idx = editor.editor_state.data.current_track as u16;
        let automation_lane = if is_bend {
            editor
                .editor_state
                .data
                .find_automation_lane(track_idx, &lumino_note_core::AutomationTarget::PitchBend)
                .and_then(|idx| {
                    editor
                        .editor_state
                        .data
                        .automation_lanes
                        .get(idx)
                        .map(|a| &**a)
                })
        } else if !is_velocity {
            editor
                .editor_state
                .data
                .find_automation_lane(
                    track_idx,
                    &lumino_note_core::AutomationTarget::CC {
                        controller: cc_number,
                    },
                )
                .and_then(|idx| {
                    editor
                        .editor_state
                        .data
                        .automation_lanes
                        .get(idx)
                        .map(|a| &**a)
                })
        } else {
            None
        };

        let view = &editor.editor_state.view;
        let canvas = &editor.editor_state.canvas;
        let theme = self.root.theme();
        let panel_height = self.root.visual.velocity_panel_height;

        // 颜色
        let note_color = theme.extended_palette().primary.weak.color;
        let bar_color = [note_color.r, note_color.g, note_color.b, 0.30];

        let bg = theme.extended_palette().background.base.color;
        let bg_color = [bg.r, bg.g, bg.b, 1.0];

        let handle = theme.extended_palette().background.strong.color;
        let handle_color = [handle.r, handle.g, handle.b, 0.25];

        let text_c = theme.text_color();
        let grab_alpha = if theme.is_light() { 0.40 } else { 0.35 };
        let grab_color = [text_c.r, text_c.g, text_c.b, grab_alpha];

        let cc_view_params = CcBarViewParams {
            panel_height,
            keyboard_width: view.keyboard_width,
            scroll_x: view.scroll_x,
            zoom_x: view.zoom_x,
            canvas_offset_x: canvas.offset_x,
            canvas_offset_y: canvas.offset_y,
            canvas_size_x: canvas.size_x,
            canvas_size_y: canvas.size_y,
            value_zoom: panel.value_zoom,
            value_scroll: panel.value_scroll,
            line_thickness: panel.automation_line_thickness,
        };
        let cc_colors = CcBarColors {
            bar_color,
            bg_color,
            handle_color,
            grab_color,
        };
        let cc_data = CcBarData {
            velocity_points: &velocity_points,
            cc_points: &cc_points,
            bend_points: &bend_points,
            automation_lane,
            velocity_curve_style: self.root.settings.display.velocity_curve_style,
        };

        lumino_gfx::build_cc_bar_instances(&panel.edit_mode, &cc_view_params, &cc_data, &cc_colors)
    }
}
