//! 远端选择高亮渲染
//!
//! 与 `remote_cursors` 同源的 2D 画布覆盖层：为每位远端用户绘制其选中音符的
//! 半透明高亮矩形（按用户着色）。仅绘制与当前显示音轨（`current_track`）匹配的指纹，
//! 因为钢琴卷帘单轨视图无法呈现其它音轨的音符位置。

use super::utils::parse_color;
use crate::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke};
use lumino_ui_core::Renderer;

/// 根据远端用户 ID 派生一个稳定颜色（当服务器/对端未提供颜色时回退）
fn derive_color(user_id: &str) -> iced_core::Color {
    let palette = [
        iced_core::Color::from_rgb(0.95, 0.30, 0.35), // 红
        iced_core::Color::from_rgb(0.30, 0.80, 0.45), // 绿
        iced_core::Color::from_rgb(0.30, 0.55, 0.95), // 蓝
        iced_core::Color::from_rgb(0.95, 0.75, 0.20), // 黄
        iced_core::Color::from_rgb(0.70, 0.35, 0.95), // 紫
        iced_core::Color::from_rgb(0.20, 0.85, 0.85), // 青
    ];
    // FNV-1a 64 位哈希，保证同一 user_id 稳定映射到同一颜色
    let mut h: u64 = 1469598103934665603u64;
    for b in user_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211u64);
    }
    palette[(h % palette.len() as u64) as usize]
}

/// 绘制所有远端用户的选择高亮
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Vec<Geometry<Renderer>> {
    let mut geometries = Vec::new();
    let current_track = editor.editor_state.data.current_track;

    for (user_id, set) in editor.remote_selections.iter() {
        let base_color = if set.color.is_empty() {
            derive_color(user_id)
        } else {
            parse_color(&set.color).unwrap_or_else(|| derive_color(user_id))
        };

        let mut frame = Frame::new(renderer, bounds.size());
        let mut any = false;

        for (track, tick, key, length) in &set.fingerprints {
            // 仅绘制当前显示音轨的指纹（单轨卷帘视图）
            if *track != current_track {
                continue;
            }
            let left = editor.tick_to_x(*tick);
            let right = editor.tick_to_x(*tick + *length);
            let top = editor.key_to_y(key.saturating_add(1));
            let bottom = editor.key_to_y(*key);
            let rect = Rectangle::new(
                Point::new(left, top),
                Size::new((right - left).max(2.0), (bottom - top).max(2.0)),
            );
            let path = Path::rectangle(rect.position(), rect.size());
            frame.fill(&path, iced_core::Color { a: 0.22, ..base_color });
            frame.stroke(
                &path,
                Stroke::default()
                    .with_width(2.0)
                    .with_color(iced_core::Color { a: 0.8, ..base_color }),
            );
            any = true;
        }

        if any {
            geometries.push(frame.into_geometry());
        }
    }

    geometries
}
