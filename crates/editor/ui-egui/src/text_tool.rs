//! 文字工具覆盖层（文本框 / 字形预览 / 输入框 / √× 模式按钮）——egui 版。
//!
//! 取代 旧版 `text_tool_box::draw` + `text_input` 覆盖层：
//! - 文本框：灰色 3px 边框 + 0.78 灰 35% 填充；
//! - 字形预览：生产侧栅格化成 RGBA（与 √ 生成音符同源），这里上传纹理铺满框；
//! - 输入框：框顶 28px 单行 `TextEdit`，编辑态自动聚焦；
//! - 按钮：√（绿）/ ×（红）/ 模式（合并=蓝 M，正常=灰 N）。

use std::sync::Arc;

use egui::{Color32, CornerRadius, Margin, Rect, Stroke, StrokeKind, TextureOptions, UiBuilder};

use crate::icons::{Icon, IconCache};
use crate::marquee::{FILL, STROKE, STROKE_WIDTH};

/// 按钮边长（与 旧版 一致）。
const BUTTON: f32 = 36.0;
/// 确认 / 取消底色。
const CONFIRM_BG: Color32 = Color32::from_rgb(46, 125, 50);
const CANCEL_BG: Color32 = Color32::from_rgb(198, 40, 40);
/// 模式按钮底色（合并 = 蓝，正常 = 灰）。
const MODE_MERGED_BG: Color32 = Color32::from_rgb(33, 118, 210);
const MODE_NORMAL_BG: Color32 = Color32::from_rgb(120, 120, 120);
/// 边框（白色 60%）。
const BORDER: Color32 = Color32::from_rgba_premultiplied(153, 153, 153, 153);
/// 输入框高度上限。
const INPUT_H: f32 = 28.0;

/// 字形预览图（RGBA）。
#[derive(Debug, Clone, PartialEq)]
pub struct TextPreview {
    /// 铺放矩形（窗口逻辑坐标）。
    pub rect: Rect,
    /// 位图宽。
    pub width: usize,
    /// 位图高。
    pub height: usize,
    /// RGBA 像素。
    pub rgba: Arc<Vec<u8>>,
    /// 缓存签名（尺寸 + 文本 + 模式 + 卷帘方向）。
    pub sig: String,
}

/// 文字工具覆盖层模型。
#[derive(Debug, Clone, PartialEq)]
pub struct TextToolModel {
    /// 文本框（窗口逻辑坐标，已裁剪）。
    pub rect: Rect,
    /// 输入框（框顶 28px）。
    pub input_rect: Rect,
    /// 当前文字。
    pub text: String,
    /// 是否处于编辑态（显示输入框）。
    pub editing: bool,
    /// 占位文案。
    pub placeholder: String,
    /// √ / × / 模式按钮（仅文字工具下显示）。
    pub confirm: Option<Rect>,
    pub cancel: Option<Rect>,
    pub mode: Option<Rect>,
    /// 模式按钮当前是否为「合并」。
    pub mode_merged: bool,
    /// 字形预览。
    pub preview: Option<TextPreview>,
}

impl TextToolModel {
    /// 是否没有可见内容。
    pub fn is_empty(&self) -> bool {
        self.rect.width() <= 0.0 || self.rect.height() <= 0.0
    }

    /// 本帧需要从 旧版 手里接管的矩形（文本框 + 按钮）。
    pub fn blocking_rects(&self) -> Vec<Rect> {
        let mut rects = vec![self.rect, self.input_rect];
        for rect in [self.confirm, self.cancel, self.mode].into_iter().flatten() {
            rects.push(rect);
        }
        rects.retain(|r| r.width() > 0.0 && r.height() > 0.0);
        rects
    }
}

/// 本帧动作。
#[derive(Debug, Clone, PartialEq)]
pub enum TextToolAction {
    /// 输入框文字变化。
    TextChanged(String),
    /// √：确认生成音符。
    Confirm,
    /// ×：取消并清空。
    Cancel,
    /// 模式切换（正常 / key 范围合并）。
    ModeToggle,
}

const ACTION_ID: &str = "lumino.text-tool.action";

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: TextToolAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<TextToolAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<TextToolAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// 预览纹理缓存（签名变化才重建纹理）。
#[derive(Clone)]
struct PreviewCache {
    sig: String,
    texture: egui::TextureHandle,
}

/// 绘制覆盖层并返回本帧动作。
pub fn show(
    ui: &mut egui::Ui,
    model: &mut TextToolModel,
    icons: &mut IconCache,
) -> Option<TextToolAction> {
    if model.is_empty() {
        return None;
    }
    {
        let painter = ui.painter();
        // 文本框底色与边框（与框选框同色）。
        painter.rect_filled(model.rect, CornerRadius::ZERO, FILL);
        painter.rect_stroke(
            model.rect,
            CornerRadius::ZERO,
            Stroke::new(STROKE_WIDTH, STROKE),
            StrokeKind::Middle,
        );

        // 字形预览：与 √ 生成音符同一份栅格。
        if let Some(preview) = &model.preview {
            let cache_id = egui::Id::unique("lumino.text-tool.preview");
            let mut cache = ui.ctx().data_mut(|d| d.get_temp::<PreviewCache>(cache_id));
            if cache.as_ref().is_none_or(|c| c.sig != preview.sig) {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [preview.width.max(1), preview.height.max(1)],
                    &preview.rgba,
                );
                let texture = ui.ctx().load_texture(
                    format!("lumino-text-preview-{}", preview.sig),
                    image,
                    TextureOptions::LINEAR,
                );
                let entry = PreviewCache {
                    sig: preview.sig.clone(),
                    texture,
                };
                ui.ctx()
                    .data_mut(|d| d.insert_temp(cache_id, entry.clone()));
                cache = Some(entry);
            }
            if let Some(cache) = &cache {
                painter.image(
                    cache.texture.id(),
                    preview.rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    }

    let mut action = None;
    // 输入框（框顶 28px）。
    if model.editing && model.input_rect.width() > 0.0 {
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(model.input_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let response = child.add_sized(
            [model.input_rect.width(), model.input_rect.height()],
            egui::TextEdit::singleline(&mut model.text)
                .hint_text(model.placeholder.clone())
                .margin(Margin::symmetric(6, 2)),
        );
        if !response.has_focus() {
            response.request_focus();
        }
        if response.changed() {
            action = Some(TextToolAction::TextChanged(model.text.clone()));
        }
        response.on_hover_cursor(egui::CursorIcon::Text);
    }

    // √ / × / 模式按钮。
    let painter = ui.painter();
    for (index, (rect, kind)) in [
        (model.confirm.map(|r| (r, ButtonKind::Confirm))),
        (model.cancel.map(|r| (r, ButtonKind::Cancel))),
        (model.mode.map(|r| (r, ButtonKind::Mode))),
    ]
    .into_iter()
    .flatten()
    .enumerate()
    {
        let response = ui.interact(
            rect,
            egui::Id::unique(("lumino.text-tool-button", index)),
            egui::Sense::click(),
        );
        let lighten = response.is_pointer_button_down_on() || response.hovered();
        let bg = match kind {
            ButtonKind::Confirm => CONFIRM_BG,
            ButtonKind::Cancel => CANCEL_BG,
            ButtonKind::Mode if model.mode_merged => MODE_MERGED_BG,
            ButtonKind::Mode => MODE_NORMAL_BG,
        };
        let bg = if lighten {
            Color32::from_rgb(
                bg.r().saturating_add(18),
                bg.g().saturating_add(18),
                bg.b().saturating_add(18),
            )
        } else {
            bg
        };
        painter.rect(
            rect,
            CornerRadius::same(8),
            bg,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        match kind {
            ButtonKind::Confirm => {
                icons.paint_tinted(
                    painter,
                    Icon::ConfirmCheck,
                    rect.shrink(9.0),
                    Some(Color32::WHITE),
                );
            }
            ButtonKind::Cancel => {
                icons.paint_tinted(
                    painter,
                    Icon::CancelCross,
                    rect.shrink(9.0),
                    Some(Color32::WHITE),
                );
            }
            ButtonKind::Mode => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if model.mode_merged { "M" } else { "N" },
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
            }
        }
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            action = Some(match kind {
                ButtonKind::Confirm => TextToolAction::Confirm,
                ButtonKind::Cancel => TextToolAction::Cancel,
                ButtonKind::Mode => TextToolAction::ModeToggle,
            });
        }
    }

    action
}

#[derive(Clone, Copy)]
enum ButtonKind {
    Confirm,
    Cancel,
    Mode,
}

/// 检查按钮尺寸（供测试/调试复用）。
pub fn button_size() -> f32 {
    BUTTON
}

/// 单行输入高度（与生产 `input_h` 一致）。
pub fn input_height(box_height: f32) -> f32 {
    box_height.min(INPUT_H)
}
