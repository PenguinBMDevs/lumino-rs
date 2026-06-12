use std::sync::mpsc::Receiver;

use crate::NoteEvent;

use super::super::params::RenderParams;

/// 准备渲染器实例
#[allow(clippy::too_many_arguments)]
pub fn prepare_renderers(
    grid_renderer: &mut crate::GridRenderer,
    note_renderer: &mut crate::NoteRenderer,
    ruler_renderer: &mut crate::RulerRenderer,
    arrangement_renderer: &mut crate::ArrangementRenderer,
    cc_bar_renderer: &mut crate::CcBarRenderer,
    params: &RenderParams,
    note_events_rx: &Receiver<NoteEvent>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    puffin::profile_scope!("prepare_renderers");

    // 音轨总览模式：准备走带渲染器，跳过钢琴卷帘相关渲染器
    if params.is_arrangement_mode {
        arrangement_renderer.prepare(
            device,
            queue,
            params.arrangement_uniform,
            &params.arrangement_note_instances,
        );
        return;
    }

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

    // 准备 CC 柱状条渲染器（背景/网格/中心线）
    if params.velocity_panel_rect.is_some() {
        cc_bar_renderer.prepare(device, queue, &params.cc_bar_instances, params.logical_size);
    }

    // 音符事件始终处理（不影响走带模式，但需要保持事件管道畅通）
    note_renderer.process_events(note_events_rx, device, queue);
}
