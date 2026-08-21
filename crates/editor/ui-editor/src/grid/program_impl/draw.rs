//! Program trait `draw` 实现逻辑 — 各图层绘制

use crate::grid::{keyboard, playback_indicator, remote_cursors, ruler, selection_box};
use crate::{Editor, Renderer, Theme};
use iced_core::Rectangle;
use iced_widget::canvas::Geometry;

pub(crate) fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Vec<Geometry<Renderer>> {
    puffin::profile_scope!("grid_widget_draw");
    crate::puffin_profiler::grid_widget_draw();
    let mut geometries = Vec::new();

    {
        puffin::profile_scope!("draw::keyboard");
        let keyboard_geom = editor
            .keyboard_cache
            .draw(renderer, bounds.size(), |frame| {
                keyboard::draw(editor, frame, bounds, theme);
            });
        geometries.push(keyboard_geom);
    }

    // 洋葱皮颜色覆盖层（不使用缓存，每帧独立绘制）
    {
        puffin::profile_scope!("draw::onion_overlay");
        if let Some(onion_geom) = keyboard::draw_onion_overlay(editor, renderer, bounds) {
            geometries.push(onion_geom);
        }
    }

    {
        puffin::profile_scope!("draw::ruler");
        let ruler_geom = editor.ruler_cache.draw(renderer, bounds.size(), |frame| {
            ruler::draw(editor, frame, bounds, theme);
        });
        geometries.push(ruler_geom);
    }

    {
        puffin::profile_scope!("draw::selection_box");
        if let Some(selection_geom) = selection_box::draw(editor, renderer, theme, bounds) {
            geometries.push(selection_geom);
        }
    }

    {
        puffin::profile_scope!("draw::i2m_box");
        if let Some(i2m_geom) = crate::grid::i2m_box::draw(editor, renderer, theme, bounds) {
            geometries.push(i2m_geom);
        }
    }

    {
        puffin::profile_scope!("draw::line_tool_box");
        if let Some(line_geom) = crate::grid::line_tool_box::draw(editor, renderer, theme, bounds) {
            geometries.push(line_geom);
        }
    }

    {
        puffin::profile_scope!("draw::remote_cursors");
        let remote_cursor_geometries = remote_cursors::draw(editor, renderer, bounds);
        geometries.extend(remote_cursor_geometries);
    }

    {
        puffin::profile_scope!("draw::playback_indicator");
        let playback_indicator_geom = playback_indicator::draw(editor, renderer, bounds);
        geometries.push(playback_indicator_geom);
    }

    geometries
}
