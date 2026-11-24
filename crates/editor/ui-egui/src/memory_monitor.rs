//! 内存监控对话框（`DialogType::MemoryMonitor`）。
//!
//! 只读：标题 18px、三行总指标（分配器总内存 / 系统 RSS / GPU 资源，
//! 数值用强调色）、「按子系统分类」14px + 逐子系统 13px 行（名称左、数值右）、
//! 11px 说明、底部右对齐主色「关闭」。
//!
//! 版面分两块互不相交的几何：上方是内容视口（[`content_rect`]，超长时滚动），
//! 下方是固定按钮带（[`close_button_rect`]）。窗口高度由
//! [`PREFERRED_CLIENT_HEIGHT`] 按最坏内容推导——子系统行数（最多 6）与
//! 折行说明都不会再把文字顶到按钮上。
//!
//! 快照在**生产侧**采样（`Snapshot::capture()`），本层只负责排版；窗口由
//! Runner 以 ~300ms 心跳重绘，因此数字保持刷新。

use egui::{Align, Color32, CornerRadius, RichText, Vec2};

use crate::chrome::MemoryMonitorModel;
use crate::layout;
use crate::theme;

const PAD: f32 = 24.0;
const METRIC_GAP: f32 = 4.0;
const ROW_GAP: f32 = 4.0;
/// 指标行（14px 文本）的分配高度。
const METRIC_ROW_H: f32 = 26.0;
/// 子系统行（13px 文本）的分配高度。
const ROW_H: f32 = 16.0;
/// 标题行（18px）的排版高度（内置字体实测）。
const TITLE_LINE_H: f32 = 21.0;
/// 小节标题（14px）的排版高度。
const SECTION_LINE_H: f32 = 16.0;
/// 说明段落（11px）单行高度：3 行实测 52。
const NOTE_LINE_H: f32 = 52.0 / 3.0;
/// 说明段落的最坏折行数：中英文在 252px 宽下均为 3 行。
const NOTE_WORST_LINES: f32 = 3.0;
/// 总指标行数（分配器 / RSS / GPU）。
const METRIC_ROWS: f32 = 3.0;
/// 子系统行的最坏数量：`AllocTag::COUNT`（MIDI / 音色库 / 音频 / GPU / UI / 其他）。
const ROW_WORST: f32 = 6.0;
/// 内容与底部按钮带之间的安全间距（保证折行说明不压按钮）。
const BUTTON_GAP: f32 = 16.0;
/// 底部按钮（生产 100 宽、`padding([8,32])`）。
const BUTTON_W: f32 = 100.0;
/// 底部按钮高度。
const BUTTON_H: f32 = 34.0;
/// 高度余量：抵消字体栅格取整，避免内容恰好等于视口时误触发滚动条。
const HEIGHT_SLACK: f32 = 4.0;

/// 最坏情况内容高度（逻辑像素）：3 行总指标 + 6 行子系统 + 3 行说明。
///
/// 逐项对应 [`show`] 的排版顺序：标题、23、指标块、22、小节标题、18、
/// 子系统块、26、说明段。
const WORST_CONTENT_H: f32 = TITLE_LINE_H
    + 23.0
    + METRIC_ROWS * METRIC_ROW_H
    + (METRIC_ROWS - 1.0) * METRIC_GAP
    + 22.0
    + SECTION_LINE_H
    + 18.0
    + ROW_WORST * ROW_H
    + (ROW_WORST - 1.0) * ROW_GAP
    + 26.0
    + NOTE_WORST_LINES * NOTE_LINE_H;

/// 内存占用详情对话框的推荐客户区高度（含 30px 自制标题栏）。
///
/// `dialog_preferred_size::DialogType::MemoryMonitor` 直接引用本常量，
/// 窗口高度由内容布局推导而不是另写一个数字：改排版 / 改文案时，
/// 高度与回归测试（`tests/memory_monitor_bands.rs`）一起约束，不会再次
/// 出现「窗口钉死 402、内容却长到 434」的按钮遮挡。
///
/// 内容随语言 / 用户程序字体继续变高时由内容区滚动兜底，绝不压按钮。
pub const PREFERRED_CLIENT_HEIGHT: f32 =
    layout::TITLEBAR_H + PAD * 2.0 + WORST_CONTENT_H + BUTTON_GAP + BUTTON_H + HEIGHT_SLACK;

/// 内存监控对话框产生的动作（只读 + 关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMonitorAction {
    /// 关闭窗口。
    Close,
}

/// 记录本帧动作。
pub fn push_action(ctx: &egui::Context, action: MemoryMonitorAction) {
    crate::action_slot::Slot::new(egui::Id::unique("lumino.memory-monitor.action"))
        .push_first(ctx, action);
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<MemoryMonitorAction> {
    crate::action_slot::Slot::new(egui::Id::unique("lumino.memory-monitor.action")).take(ctx)
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &MemoryMonitorModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let client = ui.max_rect();
    let body = body_rect(client, model.titlebar);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    // 底部按钮带先占位：内容只拿到它上方的视口，超长时滚动，
    // 绝不与按钮共享同一块几何（这是重叠 bug 的结构性修复）。
    let viewport = content_rect(client, model.titlebar);
    if viewport.height() > 0.0 {
        let mut host = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(viewport)
                .layout(egui::Layout::top_down(Align::Min)),
        );
        host.spacing_mut().item_spacing = Vec2::ZERO;
        egui::ScrollArea::vertical()
            .id_salt("lumino.memory-monitor.content")
            .auto_shrink([false, false])
            .show(&mut host, |ui| show_content(ui, model));
    }

    let close_rect = close_button_rect(client, model.titlebar);
    let close = crate::widgets::text_button(
        RichText::new(&model.close)
            .size(14.0)
            .color(theme::on_accent()),
    )
    .fill(theme::accent())
    .corner_radius(CornerRadius::same(4));
    if ui.put(close_rect, close).clicked() {
        push_action(ui.ctx(), MemoryMonitorAction::Close);
    }
}

/// 客户区扣除标题栏后的内容区（`titlebar` = 是否绘制 30px 自制标题栏）。
fn body_rect(client: egui::Rect, titlebar: bool) -> egui::Rect {
    layout::body_rect(client, titlebar)
}

/// 内容视口：标题栏下方、底部按钮带上方的可滚动区域。
///
/// 与 [`close_button_rect`] 共同保证「内容 / 按钮」两块几何不相交；
/// 无头几何回归测试直接复用本函数。
pub fn content_rect(client: egui::Rect, titlebar: bool) -> egui::Rect {
    let inner = body_rect(client, titlebar).shrink(PAD);
    let button_top = inner.bottom() - BUTTON_H;
    let bottom = (button_top - BUTTON_GAP).max(inner.top());
    egui::Rect::from_min_max(inner.min, egui::pos2(inner.right(), bottom))
}

/// 底部「关闭」按钮的矩形（右下角，含 PAD 内边距）。
pub fn close_button_rect(client: egui::Rect, titlebar: bool) -> egui::Rect {
    let inner = body_rect(client, titlebar).shrink(PAD);
    egui::Rect::from_min_size(
        egui::pos2(inner.right() - BUTTON_W, inner.bottom() - BUTTON_H),
        Vec2::new(BUTTON_W, BUTTON_H),
    )
}

/// 内容流：标题 → 三行总指标 → 子系统分类 → 说明。
///
/// 由 [`show`] 放进滚动视口；`ui` 的可用宽度决定说明段落的折行宽度。
fn show_content(ui: &mut egui::Ui, model: &MemoryMonitorModel) {
    ui.spacing_mut().item_spacing = Vec2::ZERO;

    ui.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    // 间距按真实窗口逐行量出（上一代 的 1.5 倍行高比 egui 大）。
    ui.add_space(23.0);
    for (index, metric) in model.metrics.iter().enumerate() {
        if index > 0 {
            ui.add_space(METRIC_GAP);
        }
        metric_row(
            ui,
            &metric.label,
            &metric.value,
            14.0,
            theme::accent_strong(),
            METRIC_ROW_H,
        );
    }

    ui.add_space(22.0);
    ui.add(egui::Label::new(
        RichText::new(&model.grid_title)
            .size(14.0)
            .color(theme::text()),
    ));
    ui.add_space(18.0);
    if model.rows.is_empty() {
        ui.add(egui::Label::new(
            RichText::new(&model.empty)
                .size(13.0)
                .color(theme::text_muted()),
        ));
    } else {
        for (index, row) in model.rows.iter().enumerate() {
            if index > 0 {
                ui.add_space(ROW_GAP);
            }
            metric_row(ui, &row.name, &row.value, 13.0, theme::text(), ROW_H);
        }
    }

    ui.add_space(26.0);
    ui.add(
        egui::Label::new(
            RichText::new(&model.note)
                .size(11.0)
                .color(theme::text_on_weak()),
        )
        .wrap(),
    );
}

/// 一行「标签（左）+ 数值（右）」。
///
/// 用两个子 `Ui` + `Label` 而不是 `painter.text`：数值与标签都要进入可访问树
/// （无头测试据此断言内容，读屏也能读）。
/// `row_h` = 文本区高度（不含 4 的行距）：指标行取 [`METRIC_ROW_H`]、
/// 子系统行取 [`ROW_H`]（对应 上一代 的 1.5 倍行高）。
fn metric_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    size: f32,
    value_color: Color32,
    row_h: f32,
) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h), egui::Sense::hover());
    let mid = rect.left() + rect.width() / 2.0;
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                rect.min,
                egui::pos2(mid, rect.bottom()),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    left.add(egui::Label::new(RichText::new(label).size(size).color(theme::text())).truncate());
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                egui::pos2(mid, rect.top()),
                rect.max,
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    right.add(egui::Label::new(RichText::new(value).size(size).color(value_color)).truncate());
}
