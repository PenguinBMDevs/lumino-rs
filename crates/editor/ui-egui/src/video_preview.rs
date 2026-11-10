//! 渲染器预览 / 全屏瀑布流的纹理铺放（wgpu → egui 外部纹理）。

use egui::{Color32, Rect};

use crate::chrome::TexturePreviewModel;

/// 把外部纹理铺到模型给出的每个矩形（1:1，不缩放）。
///
/// `texture = None` 时不画任何东西：保留 iced 侧的占位文本（「等待渲染…」
/// 「播放指示线不在视频带上」），避免两套文案叠加。
pub fn show(ui: &egui::Ui, model: &TexturePreviewModel) {
    let Some(texture) = model.texture else {
        return;
    };
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    for rect in &model.rects {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        ui.painter().image(texture, *rect, uv, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_without_texture_keeps_iced_placeholder() {
        let model = TexturePreviewModel {
            rects: vec![Rect::from_min_size(
                egui::pos2(10.0, 20.0),
                egui::vec2(320.0, 180.0),
            )],
            texture: None,
        };
        assert!(model.texture.is_none());
        assert_eq!(model.rects.len(), 1);
    }
}
