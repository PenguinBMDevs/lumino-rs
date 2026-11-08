//! 工具栏「工具选择区」：条目表、宽度预算与渲染。

use egui::{Align, Align2, CornerRadius, FontId, Layout, Sense, Stroke, Vec2};

use super::super::ToolbarModel;
use super::tool_selected;
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 工具栏条目：图标 + 是否工具选择器（17px 图标）+ 与前一列的间距。
pub(super) struct ToolEntry {
    pub(super) icon: Icon,
    pub(super) selector: bool,
    pub(super) gap_before: f32,
}

/// 与生产 iced `ToolItem::ALL` 同序。
pub(super) const TOOLS: &[ToolEntry] = &[
    ToolEntry {
        icon: Icon::MousePointer,
        selector: true,
        gap_before: 0.0,
    },
    ToolEntry {
        icon: Icon::MousePointerYSelect,
        selector: true,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::Pencil,
        selector: true,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::Eraser,
        selector: true,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::DrawToolbox,
        selector: true,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::Quantize,
        selector: false,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::Speed,
        selector: false,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::FlipVertical,
        selector: false,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::FlipHorizontal,
        selector: false,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::Split,
        selector: false,
        gap_before: 8.0,
    },
    ToolEntry {
        icon: Icon::Glue,
        selector: false,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::TransposeDown,
        selector: false,
        gap_before: 8.0,
    },
    ToolEntry {
        icon: Icon::TransposeUp,
        selector: false,
        gap_before: 4.0,
    },
    ToolEntry {
        icon: Icon::Tie,
        selector: false,
        gap_before: 8.0,
    },
];

/// 分组宽度：条目 + 间距 + 「精度:」标签与精度框（对应 iced 的 `Length::Shrink`）。
/// 工具条目高亮：前四个选择器映射 `Tool`，第 5 个（绘制工具箱）映射面板开关。
fn tool_highlighted(model: &ToolbarModel, index: usize, item: &ToolEntry) -> bool {
    if item.icon == Icon::DrawToolbox {
        return model.draw_panel_open;
    }
    tool_selected(model, index)
}

pub(super) fn width(ui: &egui::Ui, visible: usize) -> f32 {
    let items: f32 = TOOLS
        .iter()
        .take(visible)
        .map(|item| {
            if item.selector {
                layout::TOOL_SELECTOR_W
            } else {
                layout::TOOL_BUTTON_W
            }
        })
        .sum();
    let gaps: f32 = TOOLS.iter().take(visible).map(|item| item.gap_before).sum();
    let label = ui
        .painter()
        .layout_no_wrap(
            "精度:".to_owned(),
            FontId::proportional(theme::font::LABEL),
            theme::text_on_weak(),
        )
        .size()
        .x;
    items + gaps + 8.0 + label + 8.0 + layout::PRECISION_W
}

/// 渲染工具选择区内容。
pub(super) fn show(ui: &mut egui::Ui, cache: &mut IconCache, model: &ToolbarModel) {
    let visible = model.visible_tools.min(TOOLS.len());
    for (index, item) in TOOLS.iter().take(visible).enumerate() {
        ui.add_space(item.gap_before);
        let (icon_size, width) = if item.selector {
            (layout::TOOL_SELECTOR_ICON, layout::TOOL_SELECTOR_W)
        } else {
            (layout::TOOL_BUTTON_ICON, layout::TOOL_BUTTON_W)
        };
        widgets::icon_button(
            ui,
            cache,
            item.icon,
            icon_size,
            Vec2::new(width, layout::TOOL_BUTTON_W),
            tool_highlighted(model, index, item),
        );
    }

    ui.add_space(8.0);

    // 「精度:」标签在框外（与生产 iced 一致：14px、`background.weak.text`）。
    let label = "精度:";
    let label_width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(theme::font::LABEL),
            theme::text_on_weak(),
        )
        .size()
        .x;
    let (label_rect, _) = ui.allocate_exact_size(
        Vec2::new(label_width, layout::TOOL_BUTTON_W),
        Sense::hover(),
    );
    ui.painter().text(
        label_rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(theme::font::LABEL),
        theme::text_on_weak(),
    );
    ui.add_space(8.0);

    // 精度下拉 underlay：`background.weak` 底 + `background.strong` 1px 边框，
    // 内容 = 16px 吸附图标 + 6 + 13px 文本，内边距 8 → 57×26。
    let box_w = layout::PRECISION_W;
    let box_h = layout::PRECISION_H;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(box_w, box_h), Sense::click());
    super::super::toolbar_popup::set_snap_trigger(ui.ctx(), rect);
    if response.clicked() {
        super::super::toolbar_popup::push_action(
            ui.ctx(),
            super::super::toolbar_popup::ToolbarAction::ToggleSnapPicker,
        );
    }
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    child.add(cache.default_image(child.ctx(), Icon::MusicNote, 16.0));
    child.add(egui::Label::new(
        egui::RichText::new(&model.snap)
            .size(theme::font::BODY)
            .color(theme::text_on_weak()),
    ));
}
