//! 形状抽屉左侧面板：生产状态 → egui 模型，以及动作 → `Message` 映射。
//!
//! 画布由画布层绘制；本层只接管面板区域（含库列表、文件/编辑动作、
//! 面积填充、插入与缩放）。分割线留给 上一代，保证拖动与生产同源。

use lumino_core::shape::{AnchorBend, ShapeFill, SpamAlign, SpamEnds, StrokeKind};
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

        let rows = self.shape_drawer_rows();
        let bend = self.shape_drawer_bend();

        ShapeDrawerModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            rect,
            title: "Shape Library".to_string(),
            rows,
            name_input: state.name_input.clone(),
            name_hint: "Shape name".to_string(),
            file_actions: Self::shape_drawer_file_actions(),
            edit_actions: Self::shape_drawer_edit_actions(),
            area_toggle_label: if doc.area_mode {
                "Area: on".to_string()
            } else {
                "Area".to_string()
            },
            area_mode: doc.area_mode,
            area_clear: "Clear".to_string(),
            area_hint: self.shape_drawer_area_hint(),
            use10_hint: if doc.max_colour >= 16 {
                "16 = GM 通道 10".to_string()
            } else {
                "16 = 通道 10（需在工程设置启用）".to_string()
            },
            swatches: self.shape_drawer_swatches(),
            fill_actions: self.shape_drawer_fill_actions(),
            gate_label: "Gate".to_string(),
            gate_beats: doc.gate_beats,
            gate_minus: "-".to_string(),
            gate_plus: "+".to_string(),
            switch_actions: self.shape_drawer_switch_actions(),
            status: state.status.clone(),
            warning: self.shape_drawer_warning(),
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

    /// 库条目列表（选中项名加前缀圆点）。
    fn shape_drawer_rows(&self) -> Vec<ShapeRowModel> {
        let state = &self.state.shape_drawer;
        state
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
            .collect()
    }

    /// 当前可编辑的锚点弯折档位。
    fn shape_drawer_bend(&self) -> Option<AnchorBend> {
        let doc = self.state.shape_drawer.doc.borrow();

        // 锚点弯折：仅当选中曲线描边上的锚点时出现。
        doc.selected_anchor.and_then(|(stroke, anchor)| {
            let stroke = doc.strokes.get(stroke)?;
            if stroke.kind != StrokeKind::Curve {
                return None;
            }
            stroke.anchors.get(anchor).map(|anchor| anchor.bend)
        })
    }

    /// 文件动作按钮（新建 / 保存 / 重命名 / 删除）。
    fn shape_drawer_file_actions() -> Vec<ShapeActionButton> {
        vec![
            action("New", ShapePanelAction::New),
            action("Save", ShapePanelAction::Save),
            action("Rename", ShapePanelAction::Rename),
            action("Del", ShapePanelAction::Delete),
        ]
    }

    /// 编辑动作按钮（复制 / 粘贴 / 翻转 / 旋转）。
    fn shape_drawer_edit_actions() -> Vec<ShapeActionButton> {
        vec![
            action("Copy", ShapePanelAction::Copy),
            action("Paste", ShapePanelAction::Paste),
            action("FlipH", ShapePanelAction::FlipHorizontal),
            action("FlipV", ShapePanelAction::FlipVertical),
            action("Rot90", ShapePanelAction::Rotate90),
        ]
    }

    /// 面积填充提示：可填充时显示当前色号。
    fn shape_drawer_area_hint(&self) -> String {
        let doc = self.state.shape_drawer.doc.borrow();
        if doc.can_area_paint() {
            format!("current colour: {}", doc.area_colour)
        } else {
            "single closed stroke only".to_string()
        }
    }

    /// 色块列表（含最大色号）。
    fn shape_drawer_swatches(&self) -> Vec<ShapeSwatchModel> {
        let doc = self.state.shape_drawer.doc.borrow();
        (0..=16u8)
            .map(|colour| ShapeSwatchModel {
                colour,
                label: format!("{colour}"),
                selected: doc.area_mode && doc.area_colour == colour,
            })
            .collect()
    }

    /// 填充档位三按钮（填充 / 落位 / 末端）。
    fn shape_drawer_fill_actions(&self) -> Vec<ShapeActionButton> {
        let doc = self.state.shape_drawer.doc.borrow();
        let fill_label = match doc.fill {
            ShapeFill::Empty => "Fill: Empty",
            ShapeFill::Fill => "Fill: Fill",
            ShapeFill::Spam => "Fill: Spam",
            ShapeFill::OutlineSpam => "Fill: Outline",
        };
        let align_label = match doc.align {
            SpamAlign::Auto => "Align: Auto",
            SpamAlign::Aligned => "Align: Aligned",
            SpamAlign::Centred => "Align: Centred",
        };
        let ends_label = match doc.ends {
            SpamEnds::Round => "Ends: Round",
            SpamEnds::Keep => "Ends: Keep",
            SpamEnds::Drop => "Ends: Drop",
            SpamEnds::Min => "Ends: Min",
            SpamEnds::Stretch => "Ends: Stretch",
        };

        vec![
            action(fill_label, ShapePanelAction::CycleFill),
            action(align_label, ShapePanelAction::CycleAlign),
            action(ends_label, ShapePanelAction::CycleEnds),
        ]
    }

    /// 三个开关按钮（重叠填充 / 通道分离 / 颜色交界描边）。
    fn shape_drawer_switch_actions(&self) -> Vec<ShapeActionButton> {
        let doc = self.state.shape_drawer.doc.borrow();
        vec![
            action(
                &format!("Union: {}", if doc.union { "On" } else { "Off" }),
                ShapePanelAction::ToggleUnion,
            ),
            action(
                &format!("Apart: {}", if doc.apart { "On" } else { "Off" }),
                ShapePanelAction::ToggleApart,
            ),
            action(
                &format!("Borders: {}", if doc.borders { "On" } else { "Off" }),
                ShapePanelAction::ToggleBorders,
            ),
        ]
    }

    /// 首个库加载 warning 的展示文案。
    fn shape_drawer_warning(&self) -> Option<String> {
        let state = &self.state.shape_drawer;
        state.warnings.first().map(|warning| {
            format!(
                "{} {}: {}",
                tr!("跳过"),
                warning.path.display(),
                warning.message
            )
        })
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
            Panel::CycleFill => Prod::CycleFill,
            Panel::CycleAlign => Prod::CycleAlign,
            Panel::CycleEnds => Prod::CycleEnds,
            Panel::SetGateBeats(value) => Prod::SetGateBeats(*value),
            Panel::ToggleUnion => Prod::ToggleUnion,
            Panel::ToggleApart => Prod::ToggleApart,
            Panel::ToggleBorders => Prod::ToggleBorders,
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
