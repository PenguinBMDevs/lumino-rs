//! 右侧栏面板的命中区与定位工具（从 `right_panel.rs` 拆出）。
//!
//! 矩形写入 egui 内存，供无头测试与真实窗口自动化定位；内容区同时进入
//! 菜单占用列表，点击不会穿透到 iced。

use egui::{Id, Rect};

use super::super::model::I2mField;

/// 便于测试：面板内容矩形（由 `chrome` band 记录）。
pub fn panel_rect(ctx: &egui::Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Option<Rect>>(Id::unique("lumino.right-sidebar.content")))
        .flatten()
}

/// `chrome` band 记录内容矩形（供测试与真实窗口自动化定位）。
pub(crate) fn set_panel_rect(ctx: &egui::Context, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(Id::unique("lumino.right-sidebar.content"), Some(rect)));
}

/// 未接管面板时清掉矩形（避免陈旧占用吞掉点击）。
pub fn clear_panel_rect(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Option<Rect>>(Id::unique("lumino.right-sidebar.content")));
}

/// 便于测试：字段输入框矩形。
pub fn field_rect(ctx: &egui::Context, field: I2mField) -> Option<Rect> {
    rect(ctx, &format!("field:{}", field as u8))
}

/// 便于测试：按钮矩形（按按钮文本定位）。
pub fn button_rect(ctx: &egui::Context, label: &str) -> Option<Rect> {
    rect(ctx, &format!("button:{label}"))
}

/// 取本帧记录的任意控件矩形（键由各面板约定）。
pub fn rect(ctx: &egui::Context, key: &str) -> Option<Rect> {
    let rects: Vec<(String, Rect)> = ctx
        .data(|d| d.get_temp(Id::unique(RECTS_ID)))
        .unwrap_or_default();
    rects
        .into_iter()
        .find(|(name, _)| name == key)
        .map(|(_, rect)| rect)
}

/// 记录控件矩形（每帧先清空，面板内控件数量固定）。
pub(crate) fn record_rect(ctx: &egui::Context, key: String, rect: Rect) {
    let mut rects: Vec<(String, Rect)> = ctx
        .data(|d| d.get_temp(Id::unique(RECTS_ID)))
        .unwrap_or_default();
    if !rects.iter().any(|(name, _)| *name == key) {
        rects.push((key, rect));
    }
    ctx.data_mut(|d| d.insert_temp(Id::unique(RECTS_ID), rects));
}

/// 清空控件矩形（每帧面板绘制前调用）。
pub(crate) fn clear_rects(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<Vec<(String, Rect)>>(Id::unique(RECTS_ID)));
}

const RECTS_ID: &str = "lumino.right-sidebar.rects";
