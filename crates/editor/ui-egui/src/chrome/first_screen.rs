//! 「无面板」默认第一屏：品牌图标 + `Lumino` 字标。
//!
//! 主区域没有任何视图（渲染组 / 瀑布流组关闭后）时显示，避免整块空白底色。
//! 只做静态绘制，不参与输入占用。

use egui::{FontId, Rect, Vec2};

use crate::icons::{Icon, IconCache};
use crate::theme;

/// 品牌图标边长（逻辑像素）。
const LOGO_BOX: f32 = 96.0;
/// 字标字号（逻辑点）。
const WORDMARK_SIZE: f32 = 48.0;
/// 图标与字标之间的间距。
const GAP: f32 = 20.0;
/// 字标文本。
const WORDMARK: &str = "Lumino";

/// 在 `area` 内水平 / 垂直居中绘制「图标 + 字标」。
pub fn show(ui: &egui::Ui, area: Rect, icons: &mut IconCache) {
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    let mut job = egui::text::LayoutJob::simple_singleline(
        WORDMARK.to_owned(),
        FontId::proportional(WORDMARK_SIZE),
        theme::text(),
    );
    // 字距略放开，避免大字号下字标显得挤。
    if let Some(section) = job.sections.first_mut() {
        section.format.extra_letter_spacing = 1.0;
    }
    let galley = painter.layout_job(job);
    let text_size = galley.size();

    let total_width = LOGO_BOX + GAP + text_size.x;
    let logo = Rect::from_min_size(
        egui::pos2(
            area.center().x - total_width * 0.5,
            area.center().y - LOGO_BOX * 0.5,
        ),
        Vec2::splat(LOGO_BOX),
    );
    // 方形品牌图标自带深色底，浅色主题下也不会与白色内容底融在一起。
    icons.paint(painter, Icon::Lumino, logo);
    painter.galley(
        egui::pos2(logo.right() + GAP, area.center().y - text_size.y * 0.5),
        galley,
        theme::text(),
    );
}
