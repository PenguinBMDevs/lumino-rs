//! 工具栏右侧显示开关：洋葱皮(+下拉) / 自动滚动 / 协作 / 「更多」。
//!
//! 位置以「虚拟 More」为基准：无溢出时 More 消失、协作按钮补位；生产布局里
//! 两者宽度相近，洋葱皮与自动滚动的实际位置几乎不变。

use egui::{Rect, Vec2};

use super::super::ToolbarModel;
use super::super::toolbar_popup::{self, ToolbarAction};
use super::icon_at;
use crate::icons::Icon;
use crate::layout;
use crate::theme;

/// 绘制右侧四个控件并接管点击。
pub(super) fn show(
    ui: &mut egui::Ui,
    content: Rect,
    icons: &mut crate::icons::IconCache,
    model: &ToolbarModel,
) {
    let more_right = content.right() - layout::TOOLBAR_PAD_X;
    let more = Rect::from_min_size(
        egui::pos2(more_right - layout::MORE_BUTTON_W, content.top() + 19.0),
        Vec2::new(layout::MORE_BUTTON_W, layout::RIGHT_TOOL_W),
    );
    let scroll_rect = Rect::from_min_size(
        egui::pos2(
            more.left() - 36.0 - layout::RIGHT_TOOL_W,
            content.top() + 17.0,
        ),
        Vec2::splat(layout::RIGHT_TOOL_W),
    );
    let caret_rect = Rect::from_min_size(
        egui::pos2(scroll_rect.left() - 32.0 - 16.0, content.top() + 17.0),
        Vec2::new(16.0, layout::RIGHT_TOOL_W),
    );
    let onion_rect = Rect::from_min_size(
        egui::pos2(
            caret_rect.left() - layout::RIGHT_TOOL_W,
            content.top() + 17.0,
        ),
        Vec2::splat(layout::RIGHT_TOOL_W),
    );

    if model.more_visible {
        let more_button = icon_at(ui, icons, Icon::EllipsisVertical, more, 18.0, false);
        super::super::overflow::set_trigger(ui.ctx(), more, content);
        if more_button.clicked() {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToggleOverflowMenu);
        }
    } else {
        super::super::overflow::clear_trigger(ui.ctx());
        if model.collaboration_visible {
            let collab = Rect::from_min_size(
                egui::pos2(
                    content.right() - layout::TOOLBAR_PAD_X - 4.0 - layout::RIGHT_TOOL_W,
                    content.top() + 17.0,
                ),
                Vec2::splat(layout::RIGHT_TOOL_W),
            );
            let response = icon_at(ui, icons, Icon::Users, collab, 18.0, false);
            if response.clicked() {
                toolbar_popup::push_action(ui.ctx(), ToolbarAction::OpenCollaborationDialog);
            }
        }
    }

    if model.groups.onion {
        // 洋葱皮开启：`background.strong` 底色（与工具选中态同源）。
        if model.onion {
            ui.painter().rect_filled(
                onion_rect,
                egui::CornerRadius::same(4),
                theme::selected_bg(),
            );
        }
        let onion = icon_at(ui, icons, Icon::OnionSkin, onion_rect, 20.0, model.onion);
        let caret = icon_at(ui, icons, Icon::CaretDown, caret_rect, 12.0, false);
        super::super::toolbar_popup::set_onion_triggers(ui.ctx(), onion_rect, caret_rect);
        if onion.clicked() {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToggleOnionSkin);
        }
        if caret.clicked() {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::ToggleOnionMenu);
        }
    } else {
        super::super::toolbar_popup::clear_onion_triggers(ui.ctx());
    }

    if model.groups.auto_scroll {
        let scroll_icon = match model.auto_scroll {
            lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft => {
                Icon::ArrowsLeftRight
            }
            lumino_core::storage::config::AutoScrollMode::ScrollingIndicator => Icon::Scroll,
            lumino_core::storage::config::AutoScrollMode::Off => Icon::Ban,
        };
        let response = icon_at(ui, icons, scroll_icon, scroll_rect, 20.0, false);
        if response.clicked() {
            toolbar_popup::push_action(ui.ctx(), ToolbarAction::AutoScrollModeChanged);
        }
    }
}
