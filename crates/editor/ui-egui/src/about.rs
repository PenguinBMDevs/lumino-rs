//! 关于对话框（`DialogType::About`）。
//!
//! 布局：左侧 Lumino logo，右侧品牌行 + 版本号，两行文字整体垂直居中。
//! 只做只读展示，不产生任何动作；关闭走窗口标题栏的红绿灯。

use egui::{FontId, Label, Rect, RichText, Vec2};

use crate::chrome::AboutModel;
use crate::icons::{Icon, IconCache};
use crate::theme;

/// 整窗内边距（与其他对话框一致）。
const PAD: f32 = 24.0;
/// logo 边长（与设置「关于」页同源尺寸）。
const LOGO_SIZE: f32 = 96.0;
/// logo 与文字列的间距。
const LOGO_GAP: f32 = 24.0;
/// 品牌行字号。
const TITLE_SIZE: f32 = 20.0;
/// 版本行字号。
const VERSION_SIZE: f32 = 13.0;
/// 两行文字之间的间距。
const LINE_GAP: f32 = 6.0;

/// 渲染整个关于窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &AboutModel, icons: &mut IconCache) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    // 顶部 30px 自制标题栏由 `chrome::popup_titlebar` 绘制，本模块让出该区域。
    let body = crate::layout::body_rect(rect, model.titlebar);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    // logo：固定在左侧、垂直居中。品牌图标保留原色（`default_tint` 为 None）。
    let logo = Rect::from_center_size(
        egui::pos2(inner.left() + LOGO_SIZE * 0.5, inner.center().y),
        Vec2::splat(LOGO_SIZE),
    );
    icons.paint(ui.painter(), Icon::LogoInApp, logo);

    // 文字列：先按实际宽度排版量高，再让两行整体垂直居中。
    let text_left = logo.right() + LOGO_GAP;
    let text_width = (inner.right() - text_left).max(0.0);
    let title = ui.painter().layout(
        model.title.clone(),
        FontId::proportional(TITLE_SIZE),
        theme::text(),
        text_width,
    );
    let version = ui.painter().layout_no_wrap(
        model.version.clone(),
        FontId::proportional(VERSION_SIZE),
        theme::text_dim(),
    );
    let block_h = title.size().y + LINE_GAP + version.size().y;
    let title_top = inner.center().y - block_h * 0.5;

    let title_rect = Rect::from_min_size(
        egui::pos2(text_left, title_top),
        Vec2::new(text_width, title.size().y),
    );
    ui.put(
        title_rect,
        Label::new(
            RichText::new(&model.title)
                .size(TITLE_SIZE)
                .color(theme::text()),
        )
        .selectable(false),
    );

    // 版本号在文字列内水平居中（标题仍左对齐）。
    let version_left = centered_left(text_left, text_width, version.size().x);
    let version_rect = Rect::from_min_size(
        egui::pos2(version_left, title_rect.bottom() + LINE_GAP),
        version.size(),
    );
    ui.put(
        version_rect,
        Label::new(
            RichText::new(&model.version)
                .size(VERSION_SIZE)
                .color(theme::text_dim()),
        )
        .selectable(false),
    );
}

/// 文字列内水平居中的左边界；内容比列宽时退化为左对齐。
fn centered_left(column_left: f32, column_width: f32, item_width: f32) -> f32 {
    column_left + ((column_width - item_width) * 0.5).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::centered_left;

    #[test]
    fn version_centering_uses_text_column_center() {
        assert!((centered_left(144.0, 352.0, 100.0) - 270.0).abs() < f32::EPSILON);
    }

    #[test]
    fn overwide_item_falls_back_to_left_alignment() {
        assert_eq!(centered_left(10.0, 20.0, 40.0), 10.0);
    }
}
