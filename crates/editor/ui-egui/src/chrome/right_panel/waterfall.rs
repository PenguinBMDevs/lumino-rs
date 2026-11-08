//! 右侧栏「钢琴瀑布流」面板渲染。
//!
//! 几何对齐生产 `right_sidebar/piano_waterfall.rs`：
//! - 标题区 `padding 8`、标题 14px、标题与内容间距 8；
//! - 瀑布流内容（离屏纹理）铺满剩余宽度（不含左右内边距），1:1 采样，
//!   纹理尺寸与内容区一致（生产按面板内容宽 × 窗口逻辑高渲染）；
//! - 纹理未就绪时显示 11px 占位文案（生产同款）。
//!
//! 纹理本体由宿主注册成 egui 外部纹理（[`WATERFALL_TEXTURE_ID`]），
//! 本模块只负责把给定 ID 画到正确矩形内，不持有任何 wgpu 资源。

use egui::{Align2, FontId, Rect};

use super::super::model::WaterfallPanelModel;
use crate::theme;

/// 内容内边距（生产 `PANEL_PADDING`）。
const PAD: f32 = 8.0;
/// 标题区高度（14px 文本行高）。
const HEADER_H: f32 = 20.0;
/// 标题与内容间距（生产 `Column::spacing(8)`）。
const GAP: f32 = 8.0;

/// 绘制面板（`rect` = 手柄右侧的内容区）。
pub(super) fn show(ui: &mut egui::Ui, rect: Rect, model: &WaterfallPanelModel) {
    let left = rect.left() + PAD;
    ui.painter().text(
        egui::pos2(left, rect.top() + PAD + HEADER_H / 2.0),
        Align2::LEFT_CENTER,
        &model.title,
        FontId::proportional(theme::font::LABEL),
        theme::text(),
    );

    // 生产：`top_col` 有 8px 内边距，瀑布流内容本身不带左右内边距。
    let content = Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + PAD + HEADER_H + GAP),
        egui::pos2(rect.right(), rect.bottom() - PAD),
    );
    if content.width() <= 0.0 || content.height() <= 0.0 {
        return;
    }

    match model.texture {
        Some(texture) => {
            // 纹理按内容区尺寸离屏渲染，这里 1:1 铺满，不做拉伸以外的事。
            let image = egui::Image::new(egui::load::SizedTexture::new(texture, content.size()))
                .fit_to_exact_size(content.size());
            ui.put(content, image);
        }
        None => {
            ui.painter().text(
                content.center(),
                Align2::CENTER_CENTER,
                &model.empty_hint,
                FontId::proportional(theme::font::SMALL),
                theme::text_dim(),
            );
        }
    }
}
