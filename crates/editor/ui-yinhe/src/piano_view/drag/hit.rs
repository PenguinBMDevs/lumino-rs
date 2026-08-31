//! 命中测试 — 对应 `yinhe piano_view/drag/hit.rs` 与 `drag/types.rs`
//!
//! 提供 `hit_test_note` / `hit_test_sel_edge` / `rect_has_notes` 的 iced 侧桩，
//! 复用 `lumino_editor_state::hit_test` 与 `lumino_core::ViewState` 坐标系。

use iced_core::Point;
use lumino_core::ViewState;

/// 命中类型（对齐 yinhe `HitMode` / lumino `HitType`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    /// 音符起始边缘（ResizeLeft）
    Start,
    /// 音符中部（Move）
    Middle,
    /// 音符结束边缘（ResizeRight）
    End,
    /// 选中框边缘
    SelectionEdge,
}

/// 音符命中结果（单音符 hit test）
#[derive(Debug, Clone)]
pub struct HitNote {
    /// 音符索引（在当前音轨 notes 中的索引）
    pub note_index: usize,
    /// 命中部位
    pub kind: HitKind,
    /// 音符原始 tick/key（用于 DragState 初始化）
    pub tick: f32,
    pub key: u16,
    pub length: f32,
}

/// 命中测试：判断本地坐标是否命中某音符
///
/// `threshold_px` 对齐 yinhe `EDGE_THRESHOLD_PX = 6.0`
#[must_use]
pub fn hit_test_note(
    view: &ViewState,
    notes: &[(f32, u16, f32)],
    local_pos: Point,
    threshold_px: f32,
) -> Option<HitNote> {
    for (idx, (tick, key, len)) in notes.iter().enumerate() {
        let x0 = view.tick_to_x(*tick);
        let x1 = view.tick_to_x(tick + len);
        let y0 = view.key_to_y(*key);
        let y1 = y0 + view.zoom_y;
        let (left, right) = (x0.min(x1), x0.max(x1));
        let (top, bottom) = (y0.min(y1), y0.max(y1));
        if local_pos.x < left || local_pos.x > right || local_pos.y < top || local_pos.y > bottom {
            continue;
        }
        let dist_left = (local_pos.x - left).abs();
        let dist_right = (local_pos.x - right).abs();
        let kind = if dist_left < threshold_px {
            HitKind::Start
        } else if dist_right < threshold_px {
            HitKind::End
        } else {
            HitKind::Middle
        };
        return Some(HitNote {
            note_index: idx,
            kind,
            tick: *tick,
            key: *key,
            length: *len,
        });
    }
    None
}

/// 选中框边缘命中测试（复用 lumino `SelectionHitType` 语义）
///
/// `sel_rect` 为本地像素矩形（`SelectionHitType::LeftEdge/RightEdge/Inside`）。
#[must_use]
pub fn hit_test_sel_edge(
    local_pos: Point,
    sel_rect: iced_core::Rectangle,
    threshold_px: f32,
) -> Option<HitKind> {
    if !sel_rect.contains(local_pos) {
        return None;
    }
    if (local_pos.x - sel_rect.x).abs() < threshold_px
        || (local_pos.x - (sel_rect.x + sel_rect.width)).abs() < threshold_px
    {
        Some(HitKind::SelectionEdge)
    } else {
        Some(HitKind::Middle)
    }
}

/// 框选矩形内是否有音符（用于 marquee 空选判定）
#[must_use]
pub fn rect_has_notes(
    view: &ViewState,
    notes: &[(f32, u16, f32)],
    sel_rect: iced_core::Rectangle,
) -> bool {
    for (tick, key, len) in notes {
        let x = view.tick_to_x(*tick);
        let w = len * view.zoom_x;
        let y = view.key_to_y(*key);
        let r = iced_core::Rectangle::new(Point::new(x, y), iced_core::Size::new(w, view.zoom_y));
        if sel_rect.intersects(&r) {
            return true;
        }
    }
    false
}
