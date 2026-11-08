//! 右侧栏面板共用控件：按钮、文本度量、滚动容器与按压判定。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Pos2, Rect, Sense, Vec2};

use crate::theme;

/// 按钮样式参数。
pub(super) struct ButtonStyle {
    pub(super) fill: Color32,
    pub(super) hover: Color32,
    pub(super) text: Color32,
    pub(super) enabled: bool,
}

/// 标准按钮：`padding 6`、圆角 4、悬停换底色。
pub(super) fn button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    style: ButtonStyle,
) -> egui::Response {
    super::record_rect(ui.ctx(), format!("button:{label}"), rect);
    let response = ui.interact(
        rect,
        Id::unique(("right-panel-button", label)),
        if style.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let fill = if style.enabled && response.hovered() {
        style.hover
    } else {
        style.fill
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    ui.painter().text(
        Pos2::new(rect.center().x, rect.center().y),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(theme::font::BODY),
        style.text,
    );
    response
}

/// 文本宽度（比例字体度量）。
pub(super) fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), theme::TEXT)
        .size()
        .x
}

/// 纵向滚动容器：内容高度由模型计算，宽度铺满。
pub(super) fn scroll_body(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &str,
    total: f32,
    add: impl FnOnce(&mut egui::Ui, Rect),
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.bar_width = 6.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        scroll.floating = true;
    }
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let (_, content) = ui.allocate_space(Vec2::new(
                ui.available_width(),
                total.max(ui.available_height()),
            ));
            if content.width() > 0.0 && content.height() > 0.0 {
                add(ui, content);
            }
        });
}

/// 指定鼠标键是否**本帧按下**且落在 `rect` 内（素材拖出要求按下即触发）。
pub(super) fn pressed_in(ui: &egui::Ui, rect: Rect, button: egui::PointerButton) -> bool {
    ui.input(|i| i.pointer.button_pressed(button))
        && ui
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|pos| rect.contains(pos))
}

/// 当前指针位置（逻辑坐标）。
pub(super) fn pointer_pos(ui: &egui::Ui) -> Option<Pos2> {
    ui.input(|i| i.pointer.interact_pos())
}
