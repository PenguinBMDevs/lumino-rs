//! 键盘演奏洋葱皮：把 `PianoRollView::key_colors` 叠到键底色上。
//!
//! 数据由 `lumino-ui-editor` 的 `Editor::update_playback_key_colors` 每帧计算
//!（`key * 4` 起的 RGBA），本模块只负责按上一代的 60% 不透明度规则合成。
//! 视频导出键盘（`video_export::keyboard`）同口径，两处不得漂移。

use egui::Color32;

use super::PianoRollView;

/// 洋葱皮覆盖层的不透明度（0.6 × 255，上一代 `draw_onion_overlay`）。
pub(super) const ONION_ALPHA: u16 = 153;

/// 某键的洋葱皮叠加色；`None` = 该键无高亮。
///
/// 返回半透明色由 egui 与键底色做标准 src-over 混合，等价于上一代
/// `Color::from_rgba8(r, g, b, key_alpha / 255 * 0.6)` 的填充。
pub(super) fn key_overlay(view: &PianoRollView, key: u16) -> Option<Color32> {
    let offset = usize::from(key) * 4;
    let color = view.key_colors.get(offset..offset + 4)?;
    if color[3] == 0 {
        return None;
    }
    // 与视频导出 `composite_keyboard` 的 `scaled_alpha` 完全一致：整数截断。
    let alpha = (u16::from(color[3]) * ONION_ALPHA / 255) as u8;
    if alpha == 0 {
        return None;
    }
    Some(Color32::from_rgba_unmultiplied(
        color[0], color[1], color[2], alpha,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_with(key: u16, rgba: [u8; 4]) -> PianoRollView {
        let mut view = PianoRollView::default();
        let offset = usize::from(key) * 4;
        view.key_colors[offset..offset + 4].copy_from_slice(&rgba);
        view
    }

    #[test]
    fn test_alpha_zero_is_no_overlay() {
        let view = view_with(60, [255, 0, 0, 0]);
        assert!(key_overlay(&view, 60).is_none(), "alpha=0 必须视为无高亮");
    }

    #[test]
    fn test_overlay_scales_alpha_to_sixty_percent() {
        let view = view_with(60, [200, 100, 50, 255]);
        let overlay = key_overlay(&view, 60).expect("alpha=255 必须产生高亮");
        // 153/255 = 60%：Color32 内部是预乘 alpha，比较解预乘后的通道即可。
        assert_eq!(overlay.a(), 153);
    }

    #[test]
    fn test_key_outside_buffer_is_ignored() {
        let view = PianoRollView::default();
        assert!(key_overlay(&view, 256).is_none(), "越界键必须安全返回 None");
        assert!(key_overlay(&view, 0).is_none(), "全零数组必须无高亮");
    }
}
