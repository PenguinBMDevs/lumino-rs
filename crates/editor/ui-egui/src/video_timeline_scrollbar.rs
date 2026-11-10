//! 剪辑带水平滚动条（滑块拖拽滚动 + 左右边缘拖拽缩放）。
//!
//! 几何与交互语义照搬 iced `ScrollbarWidget`（`ui-editor/src/scrollbar_widget`）：
//! 轨道内缩 2px、滑块与轨道间隙 2px、滑块最小 20px、边缘命中 6px。
//! 本模块只负责绘制与命中；滚动/缩放经 [`VideoTimelineAction`] 回传生产侧。

use egui::{CornerRadius, Rect, Vec2};

use crate::theme;
use crate::video_clip_timeline::VideoTimelineAction;

/// 滑块最小尺寸（与 ui-core `scrollbar::THUMB_MIN_SIZE_PX` 一致）。
const THUMB_MIN: f32 = 20.0;
/// 轨道与滑块间距（`TRACK_THUMB_GAP`）。
const TRACK_THUMB_GAP: f32 = 2.0;
/// 滑块与轨道边缘间距（`THUMB_TRACK_EDGE_GAP`）。
const THUMB_EDGE_GAP: f32 = 2.0;
/// 边缘命中宽度（`EDGE_WIDTH_PX`）。
const EDGE_W: f32 = 6.0;

/// 滑块所在侧（拖拽会话）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Start,
    End,
}

/// 拖拽会话（跨帧保存在 ctx data）。
#[derive(Debug, Clone, Copy)]
enum Session {
    Thumb {
        start_x: f32,
        start_scroll: f32,
    },
    Edge {
        start_x: f32,
        start_zoom: f32,
        start_thumb_w: f32,
        edge: Edge,
    },
}

/// 滚动条几何（逻辑像素）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollGeometry {
    /// 轨道可用宽度（已扣两侧间隙）。
    pub track_w: f32,
    /// 可滚动范围（内容宽 − 视口宽，≥0）。
    pub scrollable: f32,
    /// 滑块宽度。
    pub thumb_w: f32,
    /// 滑块矩形。
    pub thumb: Rect,
}

/// 计算几何；`content_w` = 内容像素宽，`viewport_w` = 轨道可用宽度。
pub fn geometry(rect: Rect, scroll: f32, content_w: f32, viewport_w: f32) -> ScrollGeometry {
    let track_w = (rect.width() - TRACK_THUMB_GAP * 2.0).max(0.0);
    let scrollable = (content_w - viewport_w).max(0.0);
    let thumb_w = if scrollable <= 0.0 {
        track_w
    } else {
        (track_w * viewport_w / content_w.max(1.0))
            .max(THUMB_MIN)
            .min(track_w)
    };
    let clamped = scroll.clamp(0.0, scrollable);
    let thumb_x = rect.left()
        + THUMB_EDGE_GAP
        + (clamped / scrollable.max(1.0)) * (track_w - thumb_w).max(0.0);
    let thumb = Rect::from_min_size(
        egui::pos2(thumb_x, rect.top() + THUMB_EDGE_GAP),
        Vec2::new(thumb_w, (rect.height() - TRACK_THUMB_GAP * 2.0).max(0.0)),
    );
    ScrollGeometry {
        track_w,
        scrollable,
        thumb_w,
        thumb,
    }
}

/// 滑块拖拽：返回新的滚动量（与 iced 同式）。
pub fn thumb_drag_scroll(start_scroll: f32, dx: f32, geo: &ScrollGeometry) -> f32 {
    let ratio = dx / (geo.track_w - geo.thumb_w).max(1.0);
    (start_scroll + ratio * geo.scrollable).clamp(0.0, geo.scrollable)
}

/// 边缘拖拽缩放：返回新的缩放倍率；内容已满屏且继续缩小 → `None`。
pub fn edge_drag_zoom(
    start_zoom: f32,
    dx: f32,
    edge_end: bool,
    start_thumb_w: f32,
    track_w: f32,
    content_fits: bool,
) -> Option<f32> {
    let effective = if edge_end { dx } else { -dx };
    if content_fits && effective > 0.0 {
        return None;
    }
    let max_delta = (track_w - start_thumb_w).max(track_w * 0.5);
    let clamped = effective.clamp(-track_w * 0.9, max_delta);
    let ratio = (1.0 + clamped / start_thumb_w.max(1.0)).max(0.05);
    Some(start_zoom / ratio)
}

/// 绘制并处理拖拽；返回本帧动作（拖动中每帧都可能产出）。
pub fn show(
    ui: &egui::Ui,
    rect: Rect,
    scroll: f32,
    content_w: f32,
    viewport_w: f32,
    zoom: f32,
) -> Option<VideoTimelineAction> {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let geo = geometry(rect, scroll, content_w, viewport_w);
    let painter = ui.painter().with_clip_rect(rect);
    // 轨道 + 滑块（与 iced 同色系：轨道用弱底色、滑块用选中底色）。
    painter.rect_filled(rect, 0, theme::toolbar_bg());
    painter.rect_filled(geo.thumb, CornerRadius::same(4), theme::selected_bg());

    crate::chrome::menu::extend_blocked(ui.ctx(), [rect]);
    let id = egui::Id::unique("lumino.timeline.scrollbar");
    let session_id = id.with("session");
    let response = ui.interact(rect, id, egui::Sense::click_and_drag());
    let pointer = ui.ctx().pointer_latest_pos();

    let mut action = None;
    // 按下：根据命中区建立会话（边缘优先于滑块本体）。
    if response.drag_started()
        && let Some(pos) = pointer
    {
        let start_edge = pos.x <= geo.thumb.left() + EDGE_W;
        let end_edge = pos.x >= geo.thumb.right() - EDGE_W;
        let session = if start_edge || end_edge {
            Session::Edge {
                start_x: pos.x,
                start_zoom: zoom,
                start_thumb_w: geo.thumb_w,
                edge: if end_edge { Edge::End } else { Edge::Start },
            }
        } else if geo.thumb.contains(pos) {
            Session::Thumb {
                start_x: pos.x,
                start_scroll: scroll,
            }
        } else {
            // 点击轨道空白：居中跳转（与 iced 的 page 行为近似：直接定位）。
            let ratio =
                ((pos.x - rect.left() - THUMB_EDGE_GAP) / geo.track_w.max(1.0)).clamp(0.0, 1.0);
            let target = ratio * geo.scrollable;
            ui.ctx()
                .data_mut(|d| d.insert_temp(session_id, None::<Session>));
            return Some(VideoTimelineAction::ScrollX(target - scroll));
        };
        ui.ctx()
            .data_mut(|d| d.insert_temp(session_id, Some(session)));
    }

    // 拖拽中：按会话增量计算。
    if response.dragged()
        && let Some(pos) = pointer
        && let Some(session) = ui
            .ctx()
            .data(|d| d.get_temp::<Option<Session>>(session_id))
            .flatten()
    {
        match session {
            Session::Thumb {
                start_x,
                start_scroll,
            } => {
                let target = thumb_drag_scroll(start_scroll, pos.x - start_x, &geo);
                action = Some(VideoTimelineAction::ScrollX(target - scroll));
            }
            Session::Edge {
                start_x,
                start_zoom,
                start_thumb_w,
                edge,
            } => {
                if let Some(new_zoom) = edge_drag_zoom(
                    start_zoom,
                    pos.x - start_x,
                    edge == Edge::End,
                    start_thumb_w,
                    geo.track_w,
                    geo.scrollable <= 0.0,
                ) {
                    action = Some(VideoTimelineAction::Zoom {
                        factor: new_zoom / zoom.max(0.01),
                        fixed_ratio: if edge == Edge::End { 0.0 } else { 1.0 },
                    });
                }
            }
        }
    }

    if response.drag_stopped() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(session_id, None::<Session>));
    }

    // 悬停时给个手型提示（iced 同款语义：可拖）。
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 20.0))
    }

    #[test]
    fn geometry_fills_track_when_content_fits() {
        let g = geometry(rect(), 0.0, 200.0, 396.0);
        assert_eq!(g.scrollable, 0.0);
        assert!((g.thumb_w - g.track_w).abs() < 0.01);
        assert!((g.thumb.left() - 2.0).abs() < 0.01);
    }

    #[test]
    fn geometry_clamps_scroll_and_min_thumb() {
        let g = geometry(rect(), 1.0e9, 100_000.0, 396.0);
        assert!((g.thumb_w - 20.0).abs() < 0.01, "滑块不得小于 20px");
        assert!(
            (g.thumb.right() - (2.0 + g.track_w)).abs() < 0.01,
            "滚到尽头时滑块贴右缘"
        );
    }

    #[test]
    fn geometry_keeps_thumb_inside_track() {
        let g = geometry(rect(), 500.0, 4000.0, 396.0);
        assert!(g.thumb.left() >= 1.9 && g.thumb.right() <= 398.1);
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 20.0))
    }

    #[test]
    fn thumb_drag_is_proportional_to_travel() {
        let g = geometry(rect(), 0.0, 4000.0, 396.0);
        let half = (g.track_w - g.thumb_w) / 2.0;
        let mid = thumb_drag_scroll(0.0, half, &g);
        assert!(
            (mid - g.scrollable / 2.0).abs() < 1.0,
            "半程拖拽 ≈ 半程滚动"
        );
        // 超程钳到尽头。
        assert!((thumb_drag_scroll(0.0, 1.0e5, &g) - g.scrollable).abs() < 0.01);
        assert_eq!(thumb_drag_scroll(0.0, -1.0e5, &g), 0.0);
    }

    #[test]
    fn edge_drag_scales_both_directions_and_blocks_shrink_when_fit() {
        // `zoom = start / (1 + effective/thumb)`：向外拖（effective>0）→ zoom 变小，
        // 向内拖 → zoom 变大。左右缘的差异只在 effective 的符号（由 edge 决定）。
        let outward_left = edge_drag_zoom(1.0, -20.0, false, 39.0, 396.0, false).expect("左缘");
        assert!(
            outward_left < 1.0,
            "向左拖（外）应缩小 zoom：{outward_left}"
        );
        let inward_left = edge_drag_zoom(1.0, 20.0, false, 39.0, 396.0, false).expect("左缘");
        assert!(inward_left > 1.0, "向右拖（内）应放大 zoom：{inward_left}");
        // 右缘：向右拖 = 向外。
        let outward_right = edge_drag_zoom(1.0, 20.0, true, 39.0, 396.0, false).expect("右缘");
        assert!(
            outward_right < 1.0,
            "右缘向外拖应缩小 zoom：{outward_right}"
        );
        // 内容已满屏时禁止继续向外（缩小）。
        assert!(edge_drag_zoom(1.0, 20.0, true, 39.0, 396.0, true).is_none());
        assert!(edge_drag_zoom(1.0, -20.0, false, 39.0, 396.0, true).is_none());
    }
}
