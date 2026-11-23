//! 工具栏「工具选择区」：条目表、宽度预算与渲染。

use egui::{Align, Align2, CornerRadius, FontId, Layout, Sense, Stroke, Vec2};
use lumino_core::Tool;

use super::super::ToolbarModel;
use super::super::toolbar_popup::{self, ToolbarAction, ToolbarCommand};
use super::precision;
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

/// 与生产 上一代 `ToolItem::ALL` 同序。
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

/// 分组宽度：条目 + 间距 + 精度选择器（标签 + 间距 + 下拉框）。
///
/// `precision_width` 取 [`precision::selector_width`]：与绘制共用同一函数，
/// 按钮随文本变宽时工具栏总宽同步变化（否则会压住右侧分组）。
pub(super) fn width(visible: usize, precision_width: f32) -> f32 {
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
    items + gaps + layout::PRECISION_LEADING_GAP + precision_width
}

/// 工具条目高亮：前四个选择器映射 `Tool`，第 5 个（绘制工具箱）映射面板开关。
fn tool_highlighted(model: &ToolbarModel, index: usize, item: &ToolEntry) -> bool {
    if item.icon == Icon::DrawToolbox {
        return model.draw_panel_open;
    }
    tool_selected(model, index)
}

/// 条目 → 动作（前四个是工具选择器，第 5 个是悬浮条开关，其余是命令）。
///
/// 修饰键与 上一代 同口径：移调默认 ±1 半音、Ctrl ±12；水平翻转默认居中、
/// Shift 向右、Ctrl 向左。
fn action_for(icon: Icon, modifiers: egui::Modifiers) -> Option<ToolbarAction> {
    let command = |c| Some(ToolbarAction::ToolCommand(c));
    Some(match icon {
        Icon::MousePointer => ToolbarAction::ToolSelected(Tool::Pointer),
        Icon::MousePointerYSelect => ToolbarAction::ToolSelected(Tool::PointerYSelect),
        Icon::Pencil => ToolbarAction::ToolSelected(Tool::Pencil),
        Icon::Eraser => ToolbarAction::ToolSelected(Tool::Eraser),
        Icon::DrawToolbox => ToolbarAction::ToggleToolPanel,
        Icon::Quantize => return command(ToolbarCommand::Quantize),
        Icon::Speed => return command(ToolbarCommand::SpeedChange),
        Icon::FlipVertical => return command(ToolbarCommand::FlipVertical),
        Icon::FlipHorizontal => {
            return command(if modifiers.shift {
                ToolbarCommand::FlipHorizontalRight
            } else if modifiers.ctrl {
                ToolbarCommand::FlipHorizontalLeft
            } else {
                ToolbarCommand::FlipHorizontalCenter
            });
        }
        Icon::Split => return command(ToolbarCommand::Split),
        Icon::Glue => return command(ToolbarCommand::Glue),
        Icon::TransposeDown => {
            return command(if modifiers.ctrl {
                ToolbarCommand::TransposeDownOctave
            } else {
                ToolbarCommand::TransposeDown
            });
        }
        Icon::TransposeUp => {
            return command(if modifiers.ctrl {
                ToolbarCommand::TransposeUpOctave
            } else {
                ToolbarCommand::TransposeUp
            });
        }
        Icon::Tie => return command(ToolbarCommand::Tie),
        _ => return None,
    })
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
        let response = widgets::icon_button(
            ui,
            cache,
            item.icon,
            icon_size,
            Vec2::new(width, layout::TOOL_BUTTON_W),
            tool_highlighted(model, index, item),
        );
        // 工具区条目顺序与生产 `ToolbarTips::tools` 一一对应；模型缺项时
        // `HoverTip::default()` 为空，不显示浮窗（测试 / 草稿场景）。
        let tip = model.tips.tools.get(index).copied().unwrap_or_default();
        let response = widgets::hover_tip(response, tip);
        if response.clicked() {
            let modifiers = ui.input(|i| i.modifiers);
            if let Some(action) = action_for(item.icon, modifiers) {
                toolbar_popup::push_action(ui.ctx(), action);
            }
        }
    }
    ui.add_space(layout::PRECISION_LEADING_GAP);

    // 精度标签在框外（与生产 上一代 一致：14px、`background.weak.text`），
    // 文案由生产侧按当前语言给出（中文「精度:」/ 英文「Precision:」）。
    // 绘制用真实度量；折叠预算用 `precision::label_width` 的上界（只会更宽，
    // 因此组内不会被压到）。
    let label = model.precision_label.as_str();
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
    ui.add_space(layout::PRECISION_LABEL_GAP);

    // 精度下拉 underlay：`background.weak` 底 + `background.strong` 1px 边框，
    // 内容 = 16px 吸附图标 + 6 + 13px 文本，内边距 8。宽度随文本自动放大
    // （`precision::button_width` 与折叠预算同源），不再靠省略号收敛。
    //
    // 命中注册顺序：先占位（`Sense::empty`，不参与命中）→ 画内容 → 最后整块
    // `interact`。内部 `Label` 默认 `Sense::hover`，若先注册按钮，点在文字上会
    // 被 Label 抢走悬停/点击（真机表现就是「点精度框里的数字没反应」）。
    let box_w = precision::button_width(&model.snap);
    let box_h = layout::PRECISION_H;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(box_w, box_h), Sense::empty());
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::toolbar_bg(),
        Stroke::new(1.0, theme::selected_bg()),
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(layout::PRECISION_PAD_X, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = layout::PRECISION_ICON_GAP;
    child.add(cache.default_image(child.ctx(), Icon::MusicNote, layout::PRECISION_ICON));
    child.add(
        // 所有预设档位（含三连音与 `Custom 3840`）的按钮宽度都由
        // `precision::button_width` 保证足够，正常不会出现省略号；仅当用户换成
        // 异常宽的程序字体、估算被击穿时，才由截断兜住，不画到按钮外。
        egui::Label::new(
            egui::RichText::new(&model.snap)
                .size(theme::font::BODY)
                .color(theme::text_on_weak()),
        )
        .truncate(),
    );
    let response = ui.interact(
        rect,
        egui::Id::unique("lumino.toolbar.precision"),
        Sense::click(),
    );
    let response = widgets::hover_tip(response, model.tips.precision);
    super::super::toolbar_popup::set_snap_trigger(ui.ctx(), rect);
    if response.clicked() {
        super::super::toolbar_popup::push_action(
            ui.ctx(),
            super::super::toolbar_popup::ToolbarAction::ToggleSnapPicker,
        );
    }
}
