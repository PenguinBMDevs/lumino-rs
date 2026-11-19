//! 「无面板」默认第一屏：品牌图标 + `Lumino` 字标（+ 无工程时的操作区）。
//!
//! 主区域没有任何视图（钢琴卷帘组 / 渲染组 / 播放器组关闭后）时显示，
//! 避免整块空白底色。
//!
//! 两种形态：
//! - **有工程**（或第一屏仅作占位）：品牌块在内容区居中（图标与字标并排），纯展示；
//! - **无工程**（干净启动 / 新建 / 关闭工程后）：左右分栏——左栏品牌块（图标在上、
//!   `Lumino` 字标在下），右栏 5 个圆角方块按钮（新建 / 打开文件 / 打开文件夹 /
//!   设置 / 协作）。
//!
//! 尺寸不写死：按内容区高度 / 宽度动态求图标边长，使组合大致满足
//! 「高约占 1/3、宽约占 1/2」，并在小窗口下自动收缩。

use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

use crate::icons::{Icon, IconCache};
use crate::theme;

/// 图标边长相对内容区高度的比例。
const LOGO_HEIGHT_RATIO: f32 = 0.30;
/// 组合（图标 + 间距 + 字标）最大宽度相对内容区宽度的比例。
const MAX_WIDTH_RATIO: f32 = 0.56;
/// 字标字号 = 图标边长 × 该系数。
const WORDMARK_RATIO: f32 = 0.5;
/// 欢迎屏左栏（上下堆叠）的字标字号系数。
///
/// 比并排版小：堆叠时字标独占一行，用 0.5 会显得字比图标还抢眼。
const COLUMN_WORDMARK_RATIO: f32 = 0.34;
/// 图文间距 = 图标边长 × 该系数。
const GAP_RATIO: f32 = 0.2;
/// 字标字距 = 字号 × 该系数。
const TRACKING_RATIO: f32 = 0.02;
/// 测量字宽用的基准字号。
const PROBE_SIZE: f32 = 100.0;
/// 图标边长下限 / 上限（逻辑像素），避免极端窗口下失衡。
const LOGO_MIN: f32 = 56.0;
const LOGO_MAX: f32 = 480.0;
/// 欢迎屏左栏（图标在上、字标在下）的图标高度比例。
///
/// 堆叠后整块高 ≈ 图标 × 1.8，取 0.22 让整块约占内容区高度的 40%——
/// 与并排版「高约占 1/3」的观感一致，但不会把字标顶出左栏。
const COLUMN_LOGO_HEIGHT_RATIO: f32 = 0.22;
/// 字标文本。
const WORDMARK: &str = "Lumino";

/// 左栏占内容区宽度的比例（右侧留给操作区）。
const LEFT_RATIO: f32 = 0.5;
/// 分栏竖线宽度。
const DIVIDER_W: f32 = 1.0;
/// 分栏竖线上下留白比例。
const DIVIDER_INSET_RATIO: f32 = 0.12;
/// 按钮列宽度相对右栏宽度的比例。
const COLUMN_RATIO: f32 = 0.62;
/// 按钮列宽度下限 / 上限。
const COLUMN_MIN: f32 = 220.0;
const COLUMN_MAX: f32 = 340.0;
/// 单个按钮高度。
const BUTTON_H: f32 = 52.0;
/// 按钮间距。
const BUTTON_GAP: f32 = 12.0;
/// 按钮圆角半径（圆角方块）。
const BUTTON_RADIUS: f32 = 12.0;
/// 按钮文字字号。
const BUTTON_FONT: f32 = 15.0;

/// 第一屏右侧操作区的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstScreenAction {
    /// 新建工程。
    NewProject,
    /// 打开工程文件（文件对话框）。
    OpenProjectFile,
    /// 打开工程文件夹（文件夹对话框）。
    OpenProjectFolder,
    /// 打开设置对话框。
    OpenSettings,
    /// 连接到多人协作房间（拉起协作面板）。
    OpenCollaboration,
}

/// 第一屏右侧的一个按钮。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstScreenButton {
    /// 点击后产生的动作。
    pub action: FirstScreenAction,
    /// 按钮文案（由生产侧按当前语言提供）。
    pub label: String,
}

/// 第一屏模型。
#[derive(Debug, Clone, PartialEq)]
pub struct FirstScreenModel {
    /// 品牌块整体不透明度（0..=1）。
    pub opacity: f32,
    /// 右侧操作区按钮；空 = 只显示品牌块（居中），用于「已打开工程」形态。
    pub actions: Vec<FirstScreenButton>,
}

impl FirstScreenModel {
    /// 只有品牌块（有工程 / 纯占位形态）。
    pub fn brand_only(opacity: f32) -> Self {
        Self {
            opacity,
            actions: Vec::new(),
        }
    }

    /// 是否显示右侧操作区。
    pub fn welcome(&self) -> bool {
        !self.actions.is_empty()
    }
}

/// 记录一次第一屏动作（同一帧只保留最后一条，与工具栏弹层同口径）。
pub fn push_action(ctx: &egui::Context, action: FirstScreenAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.first-screen.action"), action));
}

/// 取走本帧第一屏动作。
pub fn take_action(ctx: &egui::Context) -> Option<FirstScreenAction> {
    let action = ctx
        .data(|d| d.get_temp::<FirstScreenAction>(egui::Id::unique("lumino.first-screen.action")));
    if action.is_some() {
        ctx.data_mut(|d| {
            d.remove::<FirstScreenAction>(egui::Id::unique("lumino.first-screen.action"))
        });
    }
    action
}

/// 左右分栏：`(左栏, 右栏)`，中间留 1px 竖线。
pub fn split(area: Rect) -> (Rect, Rect) {
    let half = (area.width() * LEFT_RATIO).max(0.0);
    let left = Rect::from_min_max(area.min, egui::pos2(area.left() + half, area.bottom()));
    let right = Rect::from_min_max(
        egui::pos2((left.right() + DIVIDER_W).min(area.right()), area.top()),
        area.max,
    );
    (left, right)
}

/// 按钮列矩形（在右栏内水平 / 垂直居中）。
pub fn button_column(right: Rect, count: usize) -> Option<Rect> {
    if count == 0 || right.width() <= 1.0 || right.height() <= 0.0 {
        return None;
    }
    let width = (right.width() * COLUMN_RATIO).clamp(COLUMN_MIN.min(right.width()), COLUMN_MAX);
    if width <= 1.0 {
        return None;
    }
    let total_h = count as f32 * BUTTON_H + (count.saturating_sub(1)) as f32 * BUTTON_GAP;
    let height = total_h.min(right.height());
    Some(Rect::from_center_size(
        right.center(),
        Vec2::new(width, height),
    ))
}

/// 第 `index` 个按钮的矩形（`column` 来自 [`button_column`]）。
pub fn button_rect(column: Rect, index: usize) -> Rect {
    let y = column.top() + index as f32 * (BUTTON_H + BUTTON_GAP);
    Rect::from_min_size(
        egui::pos2(column.left(), y),
        Vec2::new(column.width(), BUTTON_H.min(column.bottom() - y)),
    )
}

/// 品牌块排布。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrandLayout {
    /// 图标与字标并排（有工程 / 纯占位形态，保持既有第一屏外观）。
    Row,
    /// 图标在上、字标在下（无工程欢迎屏的左栏）。
    Column,
}

/// 品牌块实际不透明度。
///
/// 欢迎屏（无工程）是**功能页**而不是水印：`first_screen_opacity`（默认 20%）
/// 只描述「品牌占位」的淡化程度，若也套到欢迎屏上，左栏会淡到看不清、与右侧
/// 实心按钮失衡。占位形态（有工程）继续跟随设置。
pub fn brand_opacity(model: &FirstScreenModel) -> f32 {
    if model.welcome() {
        1.0
    } else {
        model.opacity.clamp(0.0, 1.0)
    }
}

/// 在 `area` 内绘制第一屏；返回需要登记为「已占用」的矩形（按钮）。
pub fn show(
    ui: &egui::Ui,
    area: Rect,
    icons: &mut IconCache,
    model: &FirstScreenModel,
) -> Vec<Rect> {
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return Vec::new();
    }
    let opacity = brand_opacity(model);
    if !model.welcome() && opacity <= 0.001 {
        return Vec::new();
    }
    if !model.welcome() {
        draw_brand(ui, area, icons, opacity, BrandLayout::Row);
        return Vec::new();
    }

    let (left, right) = split(area);
    draw_brand(ui, left, icons, opacity, BrandLayout::Column);
    draw_divider(ui, area, left);

    let Some(column) = button_column(right, model.actions.len()) else {
        return Vec::new();
    };
    let mut blocked = Vec::with_capacity(model.actions.len());
    for (index, button) in model.actions.iter().enumerate() {
        let rect = button_rect(column, index);
        if rect.height() <= 1.0 {
            continue;
        }
        let response = ui
            .interact(
                rect,
                egui::Id::unique(("lumino.first-screen.button", index)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        // 按钮恒为不透明：`first_screen_opacity` 是「品牌水印」的淡化设置，
        // 不该把可点击的操作区一起淡到看不清。
        paint_button(ui, rect, &button.label, response.hovered());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &button.label)
        });
        if response.clicked() {
            push_action(ui.ctx(), button.action);
        }
        blocked.push(rect);
    }
    blocked
}

/// 分栏竖线：只在左右两栏之间画一条 1px 弱分隔（上下留白，不顶到边缘）。
fn draw_divider(ui: &egui::Ui, area: Rect, left: Rect) {
    let inset = area.height() * DIVIDER_INSET_RATIO;
    let x = left.right() + DIVIDER_W * 0.5;
    ui.painter().rect_filled(
        Rect::from_min_max(
            egui::pos2(x - DIVIDER_W * 0.5, area.top() + inset),
            egui::pos2(x + DIVIDER_W * 0.5, area.bottom() - inset),
        ),
        0,
        theme::border(),
    );
}

/// 单个圆角方块按钮：底色 + 悬停底色 + 1px 边框 + 居中文字。
fn paint_button(ui: &egui::Ui, rect: Rect, label: &str, hovered: bool) {
    let painter = ui.painter();
    let fill = if hovered {
        theme::hover_bg()
    } else {
        theme::card_bg()
    };
    painter.rect(
        rect,
        CornerRadius::same(BUTTON_RADIUS as u8),
        fill,
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(BUTTON_FONT),
        theme::text(),
    );
}

/// 在 `area` 内居中绘制「图标 + 字标」，按 `layout` 决定并排还是上下堆叠。
fn draw_brand(ui: &egui::Ui, area: Rect, icons: &mut IconCache, opacity: f32, layout: BrandLayout) {
    if area.width() <= 0.0 || area.height() <= 0.0 {
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
    let wordmark_ratio = match layout {
        BrandLayout::Row => WORDMARK_RATIO,
        BrandLayout::Column => COLUMN_WORDMARK_RATIO,
    };
    let text_width_per_logo = wordmark_ratio * (width_per_pt + tracking);

    // 两种排布各自的「一个 logo 单位」宽度：
    // - Row：logo + 间距 + 字标宽；
    // - Column：宽度由较宽的那个决定（字标通常更宽）。
    let (width_per_logo, logo_by_height_ratio) = match layout {
        BrandLayout::Row => (1.0 + GAP_RATIO + text_width_per_logo, LOGO_HEIGHT_RATIO),
        BrandLayout::Column => (text_width_per_logo.max(1.0), COLUMN_LOGO_HEIGHT_RATIO),
    };
    let logo_by_height = area.height() * logo_by_height_ratio;
    let logo_by_width = area.width() * MAX_WIDTH_RATIO / width_per_logo.max(f32::EPSILON);
    let logo_size = logo_by_height.min(logo_by_width).clamp(LOGO_MIN, LOGO_MAX);

    let font_size = (logo_size * wordmark_ratio).max(1.0);
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

    let logo = match layout {
        BrandLayout::Row => {
            let total_width = logo_size + gap + text_size.x;
            Rect::from_min_size(
                egui::pos2(
                    area.center().x - total_width * 0.5,
                    area.center().y - logo_size * 0.5,
                ),
                Vec2::splat(logo_size),
            )
        }
        BrandLayout::Column => {
            let total_height = logo_size + gap + text_size.y;
            Rect::from_min_size(
                egui::pos2(
                    area.center().x - logo_size * 0.5,
                    area.center().y - total_height * 0.5,
                ),
                Vec2::splat(logo_size),
            )
        }
    };
    // 方形品牌图标自带深色底，浅色主题下也不会与白色内容底融在一起。
    icons.paint_alpha(painter, Icon::Lumino, logo, opacity);
    let text_pos = match layout {
        BrandLayout::Row => egui::pos2(logo.right() + gap, area.center().y - text_size.y * 0.5),
        BrandLayout::Column => egui::pos2(area.center().x - text_size.x * 0.5, logo.bottom() + gap),
    };
    painter.galley(text_pos, galley, color);
}

#[cfg(test)]
mod tests {
    use super::{
        FirstScreenAction, FirstScreenButton, FirstScreenModel, button_column, button_rect, split,
    };
    use egui::{Rect, pos2, vec2};

    fn area() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))
    }

    #[test]
    fn split_halves_content_and_leaves_divider() {
        let (left, right) = split(area());
        assert_eq!(left.width(), 600.0, "左栏占一半");
        assert_eq!(right.left(), 601.0, "右栏从分栏线之后开始");
        assert_eq!(right.right(), 1200.0);
        assert_eq!(right.width(), 599.0);
    }

    #[test]
    fn button_column_is_centered_and_bounded() {
        let (_, right) = split(area());
        let column = button_column(right, 5).expect("右栏足够大");
        assert_eq!(column.width(), 340.0, "宽度上限 340");
        let expected_h = 5.0 * 52.0 + 4.0 * 12.0;
        assert_eq!(column.height(), expected_h);
        assert!((column.center().x - right.center().x).abs() < f32::EPSILON);
        assert!((column.center().y - right.center().y).abs() < f32::EPSILON);
    }

    #[test]
    fn button_column_shrinks_in_small_window() {
        let (_, right) = split(Rect::from_min_size(pos2(0.0, 0.0), vec2(560.0, 300.0)));
        let column = button_column(right, 5).expect("右栏仍可放按钮");
        assert!(column.width() <= right.width(), "不得溢出右栏");
        assert!(column.height() <= 300.0, "不得溢出内容区高度");
    }

    #[test]
    fn buttons_stack_with_fixed_gap() {
        let (_, right) = split(area());
        let column = button_column(right, 5).expect("右栏足够大");
        let first = button_rect(column, 0);
        let second = button_rect(column, 1);
        assert_eq!(first.top(), column.top());
        assert_eq!(first.height(), 52.0);
        assert_eq!(second.top() - first.bottom(), 12.0, "间距固定 12");
        assert_eq!(first.left(), column.left());
        assert_eq!(first.width(), column.width());
    }

    #[test]
    fn empty_actions_mean_brand_only() {
        let model = FirstScreenModel::brand_only(0.5);
        assert!(!model.welcome());
        assert!(model.actions.is_empty());
    }

    #[test]
    fn welcome_model_reports_actions() {
        let model = FirstScreenModel {
            opacity: 1.0,
            actions: vec![FirstScreenButton {
                action: FirstScreenAction::NewProject,
                label: "新建工程".to_owned(),
            }],
        };
        assert!(model.welcome());
    }

    #[test]
    fn welcome_brand_ignores_watermark_opacity() {
        let mut model = FirstScreenModel {
            opacity: 0.2,
            actions: vec![FirstScreenButton {
                action: FirstScreenAction::NewProject,
                label: "新建工程".to_owned(),
            }],
        };
        assert_eq!(
            super::brand_opacity(&model),
            1.0,
            "欢迎屏是功能页，品牌块不得按水印透明度淡化"
        );
        model.actions.clear();
        assert_eq!(super::brand_opacity(&model), 0.2, "占位形态继续跟随设置");
    }
}
