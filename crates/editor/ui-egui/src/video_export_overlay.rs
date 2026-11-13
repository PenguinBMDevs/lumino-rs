//! 视频导出进度覆盖层（egui 版）。
//!
//! 整窗 base 底 + 内边距 20，内容列水平居中、整体垂直居中（与 旧版
//! `container(content).padding(20)` 的居中行为一致）。
//! 四态：导出中 / 编码收尾 / 完成 / 失败。
//!
//! 排版采用「先排计划、再落笔」两步：`plan()` 产出 `Vec<Row>` 并据此算出总高，
//! 渲染时按 `(可用高 − 总高) / 2` 的偏移整体居中。

use egui::{Align, Color32, CornerRadius, RichText, Stroke, Vec2};

use crate::chrome::{
    VideoExportOverlayModel, VideoOverlayCompleted, VideoOverlayError, VideoOverlayFinalizing,
    VideoOverlayProgress, VideoOverlayView,
};
use crate::theme;

const ACTION_ID: &str = "lumino.video-export-overlay.action";
/// 整窗外边距（生产 `padding(20)`）。
const PAD: f32 = 20.0;
/// 内容列最大宽度。
const MAX_W: f32 = 520.0;
/// 行高（16 / 14 / 13 / 12px 文本）。
const ROW_16: f32 = 24.0;
const ROW_14: f32 = 21.0;
const ROW_13: f32 = 20.0;
const ROW_12: f32 = 18.0;
/// 按钮（`padding([8,32])` / `[6,16]`）。
const BUTTON_H: f32 = 34.0;
const BUTTON_GAP: f32 = 8.0;
/// 错误详情滚动区高度（生产 `height(200)`）。
const ERROR_BOX_H: f32 = 200.0;
/// 预览占位高度。
const PREVIEW_H: f32 = 160.0;

/// 覆盖层产生的动作。
pub use crate::chrome::VideoOverlayAction;
use VideoOverlayAction as Action;

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: Action) {
    if ctx
        .data(|d| d.get_temp::<Option<Action>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<Action> {
    ctx.data_mut(|d| d.remove_temp::<Option<Action>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 按钮语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    /// 主色、宽内边距（`padding([8,32])`）。
    Wide,
    /// 危险色。
    Danger,
    /// 次要（弱底）。
    Neutral,
}

/// 一行排版计划。
enum Row {
    /// 文本（size, color, 是否居中）。
    Text(String, f32, Color32, bool),
    /// 竖向间距。
    Gap(f32),
    /// 进度条。
    Bar(f32),
    /// 预览占位。
    Preview,
    /// 按钮。
    Button(String, ButtonKind, Action),
    /// 错误详情滚动框。
    ErrorBox(String),
    /// 一行：按钮 + 右侧说明。
    ButtonWithHint(String, String),
}

impl Row {
    fn height(&self) -> f32 {
        match self {
            Row::Text(_, size, _, _) => match *size {
                s if s >= 16.0 => ROW_16,
                s if s >= 14.0 => ROW_14,
                s if s >= 13.0 => ROW_13,
                _ => ROW_12,
            },
            Row::Gap(h) => *h,
            Row::Bar(_) => ROW_14,
            Row::Preview => PREVIEW_H,
            Row::Button(..) | Row::ButtonWithHint(..) => BUTTON_H,
            Row::ErrorBox(_) => ERROR_BOX_H,
        }
    }
}

/// 渲染整个覆盖层窗口（`ui` = 客户端区域）。
pub fn show(ui: &mut egui::Ui, model: &VideoExportOverlayModel) {
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
    let area = body.shrink(PAD);
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }

    let rows = plan(&model.view);
    let total: f32 = rows.iter().map(Row::height).sum();
    let width = area.width().min(MAX_W);
    let left = area.center().x - width / 2.0;
    let mut y = area.top() + ((area.height() - total) / 2.0).max(0.0);
    let ctx = ui.ctx().clone();

    for row in rows {
        let h = row.height();
        let row_rect = egui::Rect::from_min_size(egui::pos2(left, y), Vec2::new(width, h));
        match row {
            Row::Gap(_) => {}
            Row::Text(text, size, color, center) => {
                let layout = if center {
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight)
                } else {
                    egui::Layout::left_to_right(Align::Center)
                };
                let mut child =
                    ui.new_child(egui::UiBuilder::new().max_rect(row_rect).layout(layout));
                child.add(
                    egui::Label::new(RichText::new(text).size(size).color(color)).selectable(false),
                );
            }
            Row::Bar(progress) => {
                let bar = egui::Rect::from_center_size(
                    row_rect.center(),
                    Vec2::new(row_rect.width(), 10.0),
                );
                ui.painter().rect(
                    bar,
                    CornerRadius::same(5),
                    theme::base_bg(),
                    Stroke::new(1.0, theme::selected_bg()),
                    egui::StrokeKind::Inside,
                );
                let filled = (bar.width() - 2.0) * progress.clamp(0.0, 1.0);
                if filled > 0.0 {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(bar.left() + 1.0, bar.top() + 1.0),
                            Vec2::new(filled, bar.height() - 2.0),
                        ),
                        CornerRadius::same(4),
                        theme::accent(),
                    );
                }
            }
            Row::Preview => {
                ui.painter().rect(
                    row_rect,
                    CornerRadius::same(4),
                    theme::toolbar_bg(),
                    Stroke::new(1.0, theme::selected_bg()),
                    egui::StrokeKind::Inside,
                );
            }
            Row::Button(label, kind, action) => {
                if button(ui, row_rect, &label, kind) {
                    push_action(&ctx, action);
                }
            }
            Row::ButtonWithHint(label, hint) => {
                let w = text_width(ui, &label, 14.0) + 32.0;
                let hint_w = text_width(ui, &hint, 12.0);
                let total_w = w + BUTTON_GAP + hint_w;
                let x = left + (width - total_w) / 2.0;
                let btn = egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, h));
                if button(ui, btn, &label, ButtonKind::Neutral) {
                    push_action(&ctx, Action::ForceFinish);
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(
                            egui::pos2(x + w + BUTTON_GAP, y),
                            Vec2::new(hint_w.max(1.0), h),
                        ))
                        .layout(egui::Layout::left_to_right(Align::Center)),
                );
                child.add(egui::Label::new(
                    RichText::new(hint).size(12.0).color(theme::text_muted()),
                ));
            }
            Row::ErrorBox(detail) => {
                ui.painter().rect(
                    row_rect,
                    CornerRadius::same(4),
                    theme::base_bg(),
                    Stroke::new(1.0, theme::selected_bg()),
                    egui::StrokeKind::Inside,
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row_rect.shrink(8.0))
                        .layout(egui::Layout::top_down(Align::Min)),
                );
                egui::ScrollArea::vertical()
                    .id_salt("lumino.video-overlay.error")
                    .auto_shrink([false, false])
                    .show(&mut child, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(detail).size(13.0).color(theme::danger()),
                            )
                            .wrap(),
                        );
                    });
            }
        }
        y += h;
    }
}

/// 按状态产出排版计划。
fn plan(view: &VideoOverlayView) -> Vec<Row> {
    match view {
        VideoOverlayView::Exporting(p) => exporting(p),
        VideoOverlayView::Finalizing(f) => finalizing(f),
        VideoOverlayView::Completed(c) => completed(c),
        VideoOverlayView::Error(e) => error(e),
    }
}

fn progress_rows(p: &VideoOverlayProgress) -> Vec<Row> {
    vec![
        Row::Text(p.frames_line.clone(), 13.0, theme::text_muted(), true),
        Row::Text(p.time_line.clone(), 13.0, theme::text_muted(), true),
        Row::Gap(4.0),
        Row::Bar(p.progress),
        Row::Gap(4.0),
        Row::Text(p.percent_line.clone(), 12.0, theme::text_muted(), true),
        Row::Gap(8.0),
        Row::Text(p.render_fps_line.clone(), 13.0, theme::text_muted(), true),
        Row::Text(p.speed_line.clone(), 13.0, theme::text_muted(), true),
        Row::Text(p.remaining_line.clone(), 13.0, theme::text_muted(), true),
    ]
}

fn exporting(p: &VideoOverlayProgress) -> Vec<Row> {
    let mut rows = vec![
        Row::Text(p.heading.clone(), 16.0, theme::text(), true),
        Row::Gap(8.0),
        Row::Preview,
        Row::Gap(8.0),
    ];
    rows.extend(progress_rows(p));
    rows.push(Row::Gap(12.0));
    rows.push(Row::Button(
        "取消导出".to_owned(),
        ButtonKind::Danger,
        Action::CancelExport,
    ));
    rows
}

fn finalizing(f: &VideoOverlayFinalizing) -> Vec<Row> {
    let mut rows = vec![
        Row::Text(f.progress.heading.clone(), 16.0, theme::text(), true),
        Row::Gap(8.0),
        Row::Preview,
        Row::Gap(8.0),
        Row::Text(f.finalizing_line.clone(), 14.0, theme::text(), true),
        Row::Gap(4.0),
    ];
    rows.extend(progress_rows(&f.progress));
    rows.push(Row::Gap(4.0));
    rows.push(Row::Text(f.hint.clone(), 12.0, theme::text_muted(), true));
    rows.push(Row::Gap(8.0));
    rows.push(Row::ButtonWithHint(
        f.force_finish.clone(),
        f.force_hint.clone(),
    ));
    rows
}

fn completed(c: &VideoOverlayCompleted) -> Vec<Row> {
    let mut rows = vec![
        Row::Text(c.heading.clone(), 16.0, theme::success(), true),
        Row::Gap(12.0),
        Row::Preview,
        Row::Gap(8.0),
    ];
    for line in &c.stats {
        rows.push(Row::Text(line.clone(), 13.0, theme::text_muted(), true));
    }
    rows.push(Row::Gap(16.0));
    rows.push(Row::Button(
        c.confirm.clone(),
        ButtonKind::Wide,
        Action::Dismiss,
    ));
    rows
}

fn error(e: &VideoOverlayError) -> Vec<Row> {
    vec![
        Row::Text(e.heading.clone(), 16.0, theme::danger(), true),
        Row::Gap(12.0),
        Row::ErrorBox(e.detail.clone()),
        Row::Gap(16.0),
        Row::Button(e.confirm.clone(), ButtonKind::Wide, Action::Dismiss),
    ]
}

/// 渲染一个按钮（`Wide` 用 64px 内边距，其余 32px）。
fn button(ui: &mut egui::Ui, rect: egui::Rect, label: &str, kind: ButtonKind) -> bool {
    let pad = if matches!(kind, ButtonKind::Wide) {
        64.0
    } else {
        32.0
    };
    let w = (text_width(ui, label, 14.0) + pad).min(rect.width());
    let x = rect.left() + (rect.width() - w) / 2.0;
    let (fill, text) = match kind {
        ButtonKind::Wide => (theme::accent(), theme::on_accent()),
        ButtonKind::Danger => (theme::danger(), Color32::WHITE),
        ButtonKind::Neutral => (theme::toolbar_bg(), theme::text()),
    };
    let widget = egui::Button::new(RichText::new(label).size(14.0).color(text))
        .fill(fill)
        .corner_radius(CornerRadius::same(4));
    ui.put(
        egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(w, rect.height())),
        widget,
    )
    .clicked()
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(size),
            theme::text(),
        )
        .size()
        .x
        .ceil()
}
