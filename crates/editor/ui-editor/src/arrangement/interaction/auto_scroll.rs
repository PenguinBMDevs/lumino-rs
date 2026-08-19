//! 拖拽时自动滚动视口
//!
//! 移植自 yinhe `selection/drag.rs` 的 `auto_scroll_on_drag`。

use std::time::Instant;

use iced_core::{Point, Rectangle};

use crate::Message;
use crate::arrangement::ArrangementViewport;

/// 边缘触发距离（像素）。
const MARGIN: f32 = 20.0;
/// 基础滚动速度（像素/秒）。
const BASE_SPEED: f32 = 300.0;
/// 最大单帧时间步长，避免首次事件 dt 过大导致跳跃。
const MAX_DT: f32 = 0.05;

/// 当指针靠近 Canvas 边缘时自动滚动视口。
///
/// 返回实际应用的滚动偏移 `(dx, dy)`，供调用者补偿拖拽锚点（若需要）。
/// 通过 `output` 发布 `ArrangementScrollX` / `ArrangementScrollY` 消息，
/// 由 `Root` 应用到真实视口。
pub fn auto_scroll_on_drag(
    pos: Point,
    bounds: Rectangle,
    viewport: &mut ArrangementViewport,
    track_count: usize,
    last_time: &mut Option<Instant>,
    output: &mut Vec<Message>,
) -> (f32, f32) {
    let now = Instant::now();
    let dt = last_time
        .map(|t| (now.duration_since(t).as_secs_f32()).min(MAX_DT))
        .unwrap_or(0.0);
    *last_time = Some(now);

    let mut dx = 0.0f32;
    let mut dy = 0.0f32;

    if pos.x < bounds.x + MARGIN {
        dx = -(bounds.x + MARGIN - pos.x) * BASE_SPEED * dt;
    } else if pos.x > bounds.x + bounds.width - MARGIN {
        dx = (pos.x - (bounds.x + bounds.width - MARGIN)) * BASE_SPEED * dt;
    }

    if pos.y < bounds.y + MARGIN {
        dy = -(bounds.y + MARGIN - pos.y) * BASE_SPEED * dt;
    } else if pos.y > bounds.y + bounds.height - MARGIN {
        dy = (pos.y - (bounds.y + bounds.height - MARGIN)) * BASE_SPEED * dt;
    }

    if dx == 0.0 && dy == 0.0 {
        return (0.0, 0.0);
    }

    let old_x = viewport.scroll_x;
    let old_y = viewport.scroll_y;
    viewport.scroll_x += dx;
    viewport.scroll_y += dy;
    viewport.clamp_scroll(track_count);

    let actual_dx = viewport.scroll_x - old_x;
    let actual_dy = viewport.scroll_y - old_y;

    if actual_dx != 0.0 {
        output.push(Message::ArrangementScrollX(viewport.scroll_x));
    }
    if actual_dy != 0.0 {
        output.push(Message::ArrangementScrollY(viewport.scroll_y));
    }

    (actual_dx, actual_dy)
}
