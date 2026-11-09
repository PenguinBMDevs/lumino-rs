//! 形状抽屉左侧面板：生产状态 → egui 模型，以及动作 → `Message` 映射。
//!
//! 画布仍由 iced 的 `Canvas` 绘制；本层只接管面板区域（含库列表、文件/编辑动作、
//! 面积填充、插入与缩放）。分割线留给 iced，保证拖动与生产同源。

use lumino_core::shape::{AnchorBend, StrokeKind};
use lumino_core::tr;
use lumino_ui_egui::chrome::{
    ShapeActionButton, ShapeDrawerModel, ShapePanelAction, ShapeRowModel, ShapeSwatchModel,
};

use crate::Message;

impl crate::root::Root {
    /// 形状抽屉面板模型（`rect` 由宿主按 `panel_width` 计算）。
    pub(crate) fn shape_drawer_model(&self, rect: egui::Rect) -> ShapeDrawerModel {
        let state = &self.state.shape_drawer;
        let doc = state.doc.borrow();

        let rows = state
            .library
            .all()
            .map(|drawing| {
                let selected = state.selected.as_deref() == Some(drawing.id.as_str());
                ShapeRowModel {
                    id: drawing.id.clone(),
                    label: format!("{}{}", if selected { "● " } else { "" }, drawing.name),
                    selected,
                }
            })
            .collect();

        // 锚点弯折：仅当选中曲线描边上的锚点时出现。
        let bend = doc.selected_anchor.and_then(|(stroke, anchor)| {
            let stroke = doc.strokes.get(stroke)?;
            if stroke.kind != StrokeKind::Curve {
                return None;
            }
            stroke.anchors.get(anchor).map(|anchor| anchor.bend)
        });

        let area_hint = if doc.can_area_paint() {
            format!("current colour: {}", doc.area_colour)
        } else {
            "single closed stroke only".to_string()
        };

        ShapeDrawerModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            rect,
            title: "Shape Library".to_string(),
            rows,
            name_input: state.name_input.clone(),
            name_hint: "Shape name".to_string(),
            file_actions: vec![
                action("New", ShapePanelAction::New),
                action("Save", ShapePanelAction::Save),
                action("Rename", ShapePanelAction::Rename),
                action("Del", ShapePanelAction::Delete),
            ],
            edit_actions: vec![
                action("Copy", ShapePanelAction::Copy),
                action("Paste", ShapePanelAction::Paste),
                action("FlipH", ShapePanelAction::FlipHorizontal),
                action("FlipV", ShapePanelAction::FlipVertical),
                action("Rot90", ShapePanelAction::Rotate90),
            ],
            area_toggle_label: if doc.area_mode {
                "Area: on".to_string()
            } else {
                "Area".to_string()
            },
            area_mode: doc.area_mode,
            area_clear: "Clear".to_string(),
            area_hint,
            swatches: (0..=15u8)
                .map(|colour| ShapeSwatchModel {
                    colour,
                    label: format!("{colour}"),
                    selected: doc.area_mode && doc.area_colour == colour,
                })
                .collect(),
            status: state.status.clone(),
            warning: state.warnings.first().map(|warning| {
                format!(
                    "{} {}: {}",
                    tr!("跳过"),
                    warning.path.display(),
                    warning.message
                )
            }),
            size_label: "Size".to_string(),
            insert_beats: state.insert_beats_text.clone(),
            insert_beats_hint: "beats".to_string(),
            times_label: "x".to_string(),
            insert_keys: state.insert_keys_text.clone(),
            insert_keys_hint: "keys".to_string(),
            fit_label: if state.insert_fit_contain {
                "Contain".to_string()
            } else {
                "Stretch".to_string()
            },
            fit_contain: state.insert_fit_contain,
            scale_label: "Scale".to_string(),
            scale_percent: state.scale_percent_text.clone(),
            scale_hint: "%".to_string(),
            scale_apply: "Apply".to_string(),
            bend: bend.map(|bend| {
                if bend == AnchorBend::Corner {
                    "Bend: Corner".to_string()
                } else {
                    "Bend: Smooth".to_string()
                }
            }),
            bend_corner: matches!(bend, Some(AnchorBend::Corner)),
            insert: "Insert".to_string(),
        }
    }

    /// egui 形状抽屉动作 → 生产 `Message`。
    pub(crate) fn shape_drawer_action_message(
        &self,
        action: &lumino_ui_egui::shape_drawer::ShapeDrawerAction,
    ) -> Option<Message> {
        use crate::message::ShapeDrawerAction as Prod;
        use lumino_ui_egui::chrome::ShapePanelAction as Panel;
        use lumino_ui_egui::shape_drawer::ShapeDrawerAction as Action;
        let Action::Panel(panel) = action;
        let prod = match panel {
            Panel::Select(id) => Prod::Select(id.clone()),
            Panel::New => Prod::New,
            Panel::Save => Prod::Save,
            Panel::Rename => Prod::Rename,
            Panel::Delete => Prod::Delete,
            Panel::Copy => Prod::Copy,
            Panel::Paste => Prod::Paste,
            Panel::FlipHorizontal => Prod::FlipHorizontal,
            Panel::FlipVertical => Prod::FlipVertical,
            Panel::Rotate90 => Prod::Rotate90,
            Panel::ToggleAreaMode => Prod::ToggleAreaMode,
            Panel::ClearAreas => Prod::ClearAreas,
            Panel::SetAreaColour(colour) => Prod::SetAreaColour(*colour),
            Panel::SetName(text) => Prod::SetName(text.clone()),
            Panel::SetInsertBeats(text) => Prod::SetInsertBeats(text.clone()),
            Panel::SetInsertKeys(text) => Prod::SetInsertKeys(text.clone()),
            Panel::ToggleInsertFit => Prod::ToggleInsertFit,
            Panel::SetScalePercent(text) => Prod::SetScalePercent(text.clone()),
            Panel::ApplyScale => Prod::ApplyScale,
            Panel::ToggleAnchorBend => Prod::ToggleAnchorBend,
            Panel::Insert => Prod::Insert,
        };
        Some(Message::ShapeDrawer(prod))
    }
}

/// 面板按钮构造。
fn action(label: &str, action: ShapePanelAction) -> ShapeActionButton {
    ShapeActionButton {
        label: label.to_string(),
        action,
    }
}
