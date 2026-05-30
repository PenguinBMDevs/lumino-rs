use std::sync::mpsc::Receiver;

use iced_wgpu::wgpu;
use lumino_gfx::NoteEvent;

use super::super::params::RenderParams;

/// 准备渲染器实例
pub fn prepare_renderers(
    grid_renderer: &mut lumino_gfx::GridRenderer,
    note_renderer: &mut lumino_gfx::NoteRenderer,
    ruler_renderer: &mut lumino_gfx::RulerRenderer,
    params: &RenderParams,
    note_events_rx: &Receiver<NoteEvent>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    puffin::profile_scope!("prepare_renderers");
    // 准备网格渲染器
    grid_renderer.prepare(
        queue,
        params.logical_size,
        params.scroll.0,
        params.scroll.1,
        params.zoom.0,
        params.zoom.1,
        params.keyboard_width,
        params.ruler_height,
        params.color_bg,
        params.color_bg_black_key,
        params.color_bar,
        params.color_beat,
        params.color_half_beat,
        params.color_grid,
        params.color_key_line,
        params.ppq,
        params.max_key_index,
        params.canvas_offset.0,
        params.canvas_offset.1,
    );

    // 处理音符事件
    note_renderer.process_events(note_events_rx, device, queue);

    // 准备标尺渲染器
    if !params.ruler_instances.is_empty() {
        ruler_renderer.prepare(
            device,
            queue,
            params.logical_size,
            params.keyboard_width,
            params.ruler_height,
            params.scroll.0,
            params.zoom.0,
            params.ticks_per_measure,
            params.ticks_per_beat,
        );
    }
}
