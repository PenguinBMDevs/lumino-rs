//! 工具栏「工具选择区」：条目表、宽度预算与渲染。

use egui::{Align, Align2, CornerRadius, FontId, Layout, Sense, Stroke, Vec2};

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
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::Pencil,
        selector: true,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::Eraser,
        selector: true,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::DrawToolbox,
        selector: true,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::Quantize,
        selector: false,
        gap_before: 8.0,
    },
    ToolEntry {
        icon: Icon::Speed,
        selector: false,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::FlipVertical,
        selector: false,
        gap_before: 8.0,
    },
    ToolEntry {
        icon: Icon::FlipHorizontal,
        selector: false,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::Split,
        selector: false,
        gap_before: 8.0,
    },
    ToolEntry {
        icon: Icon::Glue,
        selector: false,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::TransposeDown,
        selector: false,
        gap_before: 8.0,
    },
    ToolEntry {
        icon: Icon::TransposeUp,
        selector: false,
        gap_before: 2.0,
    },
    ToolEntry {
        icon: Icon::Tie,
        selector: false,
        gap_before: 2.0,
    },
];

/// 分组宽度：条目 + 间距 + 「精度:」标签与精度框（对应 iced 的 `Length::Shrink`）。
pub(super) fn width() -> f32 {
    let items: f32 = TOOLS
        .iter()
        .map(|item| {
            if item.selector {
                layout::TOOL_SELECTOR_W
            } else {
                layout::TOOL_BUTTON_W
            }
        })
        .sum();
    let gaps: f32 = TOOLS.iter().map(|item| item.gap_before).sum();
    items + gaps + 8.0 + 40.0 + 4.0 + layout::PRECISION_W
}

/// 渲染工具选择区内容。
pub(super) fn show(ui: &mut egui::Ui, cache: &mut IconCache) {
    for item in TOOLS {
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
            item.icon == Icon::MousePointer,
        );
    }

    ui.add_space(8.0);

    // 「精度:」标签在框外（与生产 iced 一致）。
    let (label_rect, _) =
        ui.allocate_exact_size(Vec2::new(40.0, layout::TOOL_BUTTON_W), Sense::hover());
    ui.painter().text(
        label_rect.center(),
        Align2::CENTER_CENTER,
        "精度:",
        FontId::proportional(theme::font::SMALL),
        theme::TEXT_MUTED,
    );
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(layout::PRECISION_W, layout::TOOL_BUTTON_W),
        Sense::click(),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::PANEL_BG,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 4.0;
    child.add(cache.default_image(child.ctx(), Icon::MusicNote, 12.0));
    child.add(egui::Label::new(
        egui::RichText::new("1/4")
            .size(theme::font::SMALL)
            .color(theme::TEXT),
    ));
}
