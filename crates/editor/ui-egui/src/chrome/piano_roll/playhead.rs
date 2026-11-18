//! 演奏指示线：纵贯卷帘网格的竖线 + 标尺顶部倒三角。
//!
//! **层级契约**：本层必须是卷帘骨架的最后一层——键盘列 / 标尺 / 两条滚动条
//! 之后绘制，且画在 `lumino-gfx` 离屏纹理（网格与音符）之上。
//!
//! 历史回归（本模块存在的原因）：egui 迁移时指示线被写进 `ruler_cell`，
//! 只画标尺行内那一段（24px），网格区域的整条线丢失——观感即「被卷帘盖住」。
//! 上一代 iced 实现的契约是 `playback_indicator::draw` 作为最后一个
//! `geometries.push`，线从 `y = 0`（标尺顶）贯穿到画布底。

use egui::{Pos2, Rect, Vec2};

use super::PianoRollView;
use crate::layout;
use crate::theme;

/// 演奏指示线宽度（上一代 `PLAYBACK_INDICATOR_WIDTH`）。
const PLAYHEAD_WIDTH: f32 = 2.0;
/// 演奏指示线顶部倒三角尺寸（上一代 `PLAYBACK_INDICATOR_TRIANGLE_SIZE`）。
const PLAYHEAD_TRIANGLE: f32 = 8.0;

/// 指示线几何：`(竖线矩形, 倒三角三顶点, 裁剪矩形)`。
///
/// 返回 `None` 表示指示线不在可见网格内（键盘列左侧 / 网格右缘之外）。
/// 纯函数便于单测；像素级覆盖由 `tests/piano_roll_skeleton.rs` 兜底。
pub(super) fn geometry(area: Rect, view: &PianoRollView) -> Option<(Rect, [Pos2; 3], Rect)> {
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return None;
    }
    let bar = layout::SCROLLBAR;
    let canvas_w = (area.width() - bar).max(0.0);
    let canvas_h = (area.height() - bar).max(0.0);
    if canvas_w <= 0.0 || canvas_h <= 0.0 {
        return None;
    }
    let keyboard_w = view.keyboard_width.clamp(0.0, canvas_w);
    // 负偏移 = 没有指示线（`PianoRollView::playhead_offset` 的哨兵）。
    if view.playhead_offset < 0.0 {
        return None;
    }
    let grid_left = area.left() + keyboard_w;
    let grid_right = area.left() + canvas_w;
    let center = grid_left + view.playhead_offset;
    // 上一代 判据：落在键盘列或网格右缘之外不画。
    if center > grid_right {
        return None;
    }

    let line = Rect::from_min_size(
        egui::pos2(center - PLAYHEAD_WIDTH / 2.0, area.top()),
        Vec2::new(PLAYHEAD_WIDTH, canvas_h),
    );
    let half = PLAYHEAD_TRIANGLE / 2.0;
    let triangle = [
        egui::pos2(center - half, area.top()),
        egui::pos2(center + half, area.top()),
        egui::pos2(center, area.top() + PLAYHEAD_TRIANGLE),
    ];
    // 裁剪到网格列：不得压住键盘列与右侧滚动条。
    let clip = Rect::from_min_max(
        egui::pos2(grid_left, area.top()),
        egui::pos2(grid_right, area.top() + canvas_h),
    );
    Some((line, triangle, clip))
}

/// 绘制演奏指示线（网格整高，标尺顶部带倒三角）。
pub(super) fn show(ui: &egui::Ui, area: Rect, view: &PianoRollView) {
    let Some((line, triangle, clip)) = geometry(area, view) else {
        return;
    };
    let painter = ui.painter().with_clip_rect(clip);
    painter.rect_filled(line, 0, theme::playhead());
    painter.add(egui::Shape::convex_polygon(
        triangle.to_vec(),
        theme::playhead(),
        egui::Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::geometry;
    use crate::chrome::piano_roll::PianoRollView;

    fn area() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0))
    }

    fn view(playhead_offset: f32) -> PianoRollView {
        PianoRollView {
            keyboard_width: 120.0,
            playhead_offset,
            ..Default::default()
        }
    }

    #[test]
    fn line_spans_full_grid_height_from_ruler_top() {
        let (line, triangle, clip) = geometry(area(), &view(100.0)).expect("网格内应绘制");
        assert_eq!(line.top(), 0.0, "线必须从标尺顶开始（含标尺行）");
        assert_eq!(line.height(), 580.0, "必须贯穿到水平滚动条上沿（600-20）");
        assert_eq!(line.center().x, 220.0, "x = 键盘宽 120 + 偏移 100");
        assert_eq!(line.width(), 2.0, "线宽沿用 上一代 2px");
        assert_eq!(triangle[2].y, 8.0, "倒三角高 8px");
        assert_eq!(triangle[0].x, 216.0);
        assert_eq!(triangle[1].x, 224.0);
        assert_eq!(clip.left(), 120.0, "裁剪从网格左缘起（键盘列不可压）");
        assert_eq!(clip.right(), 780.0, "裁剪到网格右缘（滚动条不可压）");
        assert_eq!(clip.bottom(), 580.0, "裁剪到水平滚动条上沿");
    }

    #[test]
    fn negative_offset_skips_playhead() {
        assert!(
            geometry(area(), &view(-1.0)).is_none(),
            "负偏移是「无指示线」哨兵"
        );
    }

    #[test]
    fn offset_beyond_grid_right_edge_skips_playhead() {
        // 网格右缘 = 800 - 20 = 780，键盘 120 ⇒ 偏移 > 660 时越界。
        assert!(geometry(area(), &view(660.0)).is_some(), "恰在右缘仍绘制");
        assert!(geometry(area(), &view(661.0)).is_none(), "越过右缘不绘制");
    }

    #[test]
    fn zero_area_draws_nothing() {
        let empty = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(0.0, 0.0));
        assert!(geometry(empty, &view(10.0)).is_none());
    }
}
