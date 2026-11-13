//! 内存监控对话框（`DialogType::MemoryMonitor`）。
//!
//! 只读：标题 18px、三行总指标（分配器总内存 / 系统 RSS / GPU 资源，
//! 数值用强调色）、「按子系统分类」14px + 逐子系统 13px 行（名称左、数值右）、
//! 11px 说明、底部右对齐主色「关闭」。
//!
//! 快照在**生产侧**采样（`Snapshot::capture()`），本层只负责排版；窗口由
//! Runner 以 ~300ms 心跳重绘，因此数字保持刷新。

use egui::{Align, Color32, CornerRadius, RichText, Vec2};

use crate::chrome::MemoryMonitorModel;
use crate::theme;

const PAD: f32 = 24.0;
const METRIC_GAP: f32 = 4.0;
const ROW_GAP: f32 = 4.0;
/// 底部按钮（生产 100 宽、`padding([8,32])`）。
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 34.0;

/// 内存监控对话框产生的动作（只读 + 关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMonitorAction {
    /// 关闭窗口。
    Close,
}

/// 记录本帧动作。
pub fn push_action(ctx: &egui::Context, action: MemoryMonitorAction) {
    let pending = ctx
        .data(|d| {
            d.get_temp::<Option<MemoryMonitorAction>>(egui::Id::unique(
                "lumino.memory-monitor.action",
            ))
        })
        .flatten()
        .is_some();
    if pending {
        return;
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::unique("lumino.memory-monitor.action"),
            Some(action),
        )
    });
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<MemoryMonitorAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<MemoryMonitorAction>>(egui::Id::unique(
            "lumino.memory-monitor.action",
        ))
    })
    .flatten()
}

/// 渲染整个对话框窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &MemoryMonitorModel) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    let rect = ui.max_rect();
    let top = if model.titlebar {
        crate::layout::TITLEBAR_H
    } else {
        0.0
    };
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + top), rect.max);
    ui.painter().rect_filled(body, 0, theme::base_bg());
    let inner = body.shrink(PAD);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    child.spacing_mut().item_spacing = Vec2::ZERO;

    child.add(egui::Label::new(
        RichText::new(&model.title).size(18.0).color(theme::text()),
    ));
    // 间距按真实窗口逐行量出（旧版 的 1.5 倍行高比 egui 大）。
    child.add_space(23.0);
    for (index, metric) in model.metrics.iter().enumerate() {
        if index > 0 {
            child.add_space(METRIC_GAP);
        }
        metric_row(
            &mut child,
            &metric.label,
            &metric.value,
            14.0,
            theme::accent_strong(),
            26.0,
        );
    }

    child.add_space(22.0);
    child.add(egui::Label::new(
        RichText::new(&model.grid_title)
            .size(14.0)
            .color(theme::text()),
    ));
    child.add_space(18.0);
    if model.rows.is_empty() {
        child.add(egui::Label::new(
            RichText::new(&model.empty)
                .size(13.0)
                .color(theme::text_muted()),
        ));
    } else {
        for (index, row) in model.rows.iter().enumerate() {
            if index > 0 {
                child.add_space(ROW_GAP);
            }
            metric_row(&mut child, &row.name, &row.value, 13.0, theme::text(), 16.0);
        }
    }

    child.add_space(26.0);
    child.add(
        egui::Label::new(
            RichText::new(&model.note)
                .size(11.0)
                .color(theme::text_on_weak()),
        )
        .wrap(),
    );

    // 底部右对齐「关闭」：用整宽行 + 右锚点，避免受列布局影响。
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(inner.right() - BUTTON_W, inner.bottom() - BUTTON_H),
        Vec2::new(BUTTON_W, BUTTON_H),
    );
    let close = egui::Button::new(
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

/// 一行「标签（左）+ 数值（右）」。
///
/// 用两个子 `Ui` + `Label` 而不是 `painter.text`：数值与标签都要进入可访问树
/// （无头测试据此断言内容，读屏也能读）。
/// `row_h` = 文本区高度（不含 4 的行距），14px 指标行 26、13px 子系统行 16
/// （对应 旧版 的 1.5 倍行高）。
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
    left.add(egui::Label::new(
        RichText::new(label).size(size).color(theme::text()),
    ));
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                egui::pos2(mid, rect.top()),
                rect.max,
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    right.add(egui::Label::new(
        RichText::new(value).size(size).color(value_color),
    ));
}
