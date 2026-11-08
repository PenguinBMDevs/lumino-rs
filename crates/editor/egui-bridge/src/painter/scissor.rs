//! 逻辑矩形 → 物理像素 scissor 换算。

use egui::Rect;

/// 逻辑矩形 → 物理像素 scissor（`x, y, width, height`）。
pub(super) fn scissor_rect(
    clip: Rect,
    pixels_per_point: f32,
    screen: [u32; 2],
) -> Option<[u32; 4]> {
    // 与 `egui-wgpu` 的 `ScissorRect::new` 逐行一致：四边先 round 再 clamp。
    let to_pixel = |value: f32| (value * pixels_per_point).round() as u32;
    let min_x = to_pixel(clip.min.x).clamp(0, screen[0]);
    let min_y = to_pixel(clip.min.y).clamp(0, screen[1]);
    let max_x = to_pixel(clip.max.x).clamp(min_x, screen[0]);
    let max_y = to_pixel(clip.max.y).clamp(min_y, screen[1]);
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    Some([min_x, min_y, max_x - min_x, max_y - min_y])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scissor_clamps_to_target() {
        let clip = Rect::from_min_max(egui::pos2(-10.0, -10.0), egui::pos2(20.0, 20.0));
        assert_eq!(scissor_rect(clip, 1.0, [64, 64]), Some([0, 0, 20, 20]));

        let clip = Rect::from_min_max(egui::pos2(60.0, 60.0), egui::pos2(90.0, 90.0));
        assert_eq!(scissor_rect(clip, 1.0, [64, 64]), Some([60, 60, 4, 4]));

        let clip = Rect::from_min_max(egui::pos2(90.0, 90.0), egui::pos2(120.0, 120.0));
        assert_eq!(scissor_rect(clip, 1.0, [64, 64]), None);
    }

    #[test]
    fn scissor_scales_with_dpi() {
        let clip = Rect::from_min_max(egui::pos2(8.0, 8.0), egui::pos2(24.0, 24.0));
        assert_eq!(scissor_rect(clip, 2.0, [128, 128]), Some([16, 16, 32, 32]));
    }
}
