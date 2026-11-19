//! 「无面板」默认第一屏：品牌图标 + `Lumino` 字标。
//!
//! 主区域没有任何视图（钢琴卷帘组 / 渲染组 / 播放器组关闭后）时显示，
//! 避免整块空白底色。只做静态绘制，不参与输入占用。
//!
//! 尺寸不写死：按内容区高度 / 宽度动态求图标边长，使组合大致满足
//! 「高约占 1/3、宽约占 1/2」，并在小窗口下自动收缩。

use egui::{FontId, Rect, Vec2};

use crate::icons::{Icon, IconCache};
use crate::theme;

/// 图标边长相对内容区高度的比例。
const LOGO_HEIGHT_RATIO: f32 = 0.30;
/// 组合（图标 + 间距 + 字标）最大宽度相对内容区宽度的比例。
const MAX_WIDTH_RATIO: f32 = 0.56;
/// 字标字号 = 图标边长 × 该系数。
const WORDMARK_RATIO: f32 = 0.5;
/// 图文间距 = 图标边长 × 该系数。
const GAP_RATIO: f32 = 0.2;
/// 字标字距 = 字号 × 该系数。
const TRACKING_RATIO: f32 = 0.02;
/// 测量字宽用的基准字号。
const PROBE_SIZE: f32 = 100.0;
/// 图标边长下限 / 上限（逻辑像素），避免极端窗口下失衡。
const LOGO_MIN: f32 = 56.0;
const LOGO_MAX: f32 = 480.0;
/// 字标文本。
const WORDMARK: &str = "Lumino";

/// 在 `area` 内水平 / 垂直居中绘制「图标 + 字标」。
///
/// `opacity` 为整体不透明度（0..=1）；≤ 0.001 时直接跳过绘制。
pub fn show(ui: &egui::Ui, area: Rect, icons: &mut IconCache, opacity: f32) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.001 || area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    let painter = ui.painter();
    let color = theme::text().gamma_multiply(opacity);

    // 先量一次「每 1pt 字号」的字标宽度，再用闭式解同时满足高度与宽度约束，
    // 避免先排一次版再回缩导致尺寸在不同帧之间抖动。
    let probe =
        painter.layout_no_wrap(WORDMARK.to_owned(), FontId::proportional(PROBE_SIZE), color);
    let width_per_pt = (probe.size().x / PROBE_SIZE).max(f32::EPSILON);
    // 字标宽度 = 字号 × (基础宽度 + 字距)；字距按字号比例加回测量值。
    let tracking = (WORDMARK.chars().count().saturating_sub(1)) as f32 * TRACKING_RATIO;
    let text_width_per_logo = WORDMARK_RATIO * (width_per_pt + tracking);
    let width_per_logo = 1.0 + GAP_RATIO + text_width_per_logo;

    let logo_by_height = area.height() * LOGO_HEIGHT_RATIO;
    let logo_by_width = area.width() * MAX_WIDTH_RATIO / width_per_logo;
    let logo_size = logo_by_height.min(logo_by_width).clamp(LOGO_MIN, LOGO_MAX);

    let font_size = (logo_size * WORDMARK_RATIO).max(1.0);
    let gap = logo_size * GAP_RATIO;

    let mut job = egui::text::LayoutJob::simple_singleline(
        WORDMARK.to_owned(),
        FontId::proportional(font_size),
        color,
    );
    // 字距随字号放大，避免大字号下字标显得挤。
    if let Some(section) = job.sections.first_mut() {
        section.format.extra_letter_spacing = font_size * TRACKING_RATIO;
    }
    let galley = painter.layout_job(job);
    let text_size = galley.size();

    let total_width = logo_size + gap + text_size.x;
    let logo = Rect::from_min_size(
        egui::pos2(
            area.center().x - total_width * 0.5,
            area.center().y - logo_size * 0.5,
        ),
        Vec2::splat(logo_size),
    );
    // 方形品牌图标自带深色底，浅色主题下也不会与白色内容底融在一起。
    icons.paint_alpha(painter, Icon::Lumino, logo, opacity);
    painter.galley(
        egui::pos2(logo.right() + gap, area.center().y - text_size.y * 0.5),
        galley,
        color,
    );
}
