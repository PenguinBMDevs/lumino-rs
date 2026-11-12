//! 框选框：生产状态 → egui 模型。
//!
//! 与 iced `selection_box::draw` 同语义：
//! 1. 拖拽中（`EditState::Selecting`）显示半透明选框；
//!    弹簧模式取动画位置，Direct 模式取实时位置；
//! 2. 已选音符显示包围盒（Selecting 状态下不重复画）；
//! 3. 全部矩形裁剪到卷帘内容区（键盘列 / 标尺 / 窗口外不显示）。

use lumino_core::storage::config::SelectionBoxMode;
use lumino_ui_egui::marquee::MarqueeModel;

use crate::root::Root;

impl Root {
    /// 本帧框选框模型（窗口逻辑坐标）。
    pub fn marquee_model(&self) -> MarqueeModel {
        let editor = &self.editor;
        let canvas = &editor.editor_state.canvas;
        let offset = egui::vec2(canvas.offset_x, canvas.offset_y);
        let local = lumino_ui_editor::grid::utils::content_bounds(editor);
        let content = egui::Rect::from_min_size(
            egui::pos2(local.x + offset.x, local.y + offset.y),
            egui::vec2(local.width, local.height),
        );
        if content.width() <= 0.0 || content.height() <= 0.0 {
            return MarqueeModel::default();
        }

        let mut rects = Vec::new();
        let mut push_local = |min_x: f32, max_x: f32, min_y: f32, max_y: f32| {
            let rect = egui::Rect::from_min_max(
                egui::pos2(min_x + offset.x, min_y + offset.y),
                egui::pos2(max_x + offset.x, max_y + offset.y),
            );
            let clipped = rect.intersect(content);
            if clipped.width() > 0.0 && clipped.height() > 0.0 {
                rects.push(clipped);
            }
        };

        // 1) 拖拽中的选框。
        let selection_box = if editor.selection_box_mode() == SelectionBoxMode::Direct {
            editor.get_selection_box()
        } else {
            editor
                .selection_box_anim
                .get()
                .map(|anim| (anim.start_pos, anim.current_pos))
                .or_else(|| editor.get_selection_box())
        };
        if let Some((start, current)) = selection_box {
            push_local(
                start.x.min(current.x),
                start.x.max(current.x),
                start.y.min(current.y),
                start.y.max(current.y),
            );
        }

        // 2) 选中音符的包围盒（Selecting 状态下只画上面的拖拽框）。
        let selected = &editor.editor_state.interaction.selected_notes;
        if !selected.is_empty()
            && !matches!(
                editor.editor_state.interaction.edit_state,
                lumino_ui_editor::EditState::Selecting { .. }
            )
        {
            for (min_x, max_x, min_y, max_y) in editor.get_selection_box_rects() {
                push_local(min_x, max_x, min_y, max_y);
            }
        }

        MarqueeModel { rects }
    }
}
