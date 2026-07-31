use std::sync::mpsc::Receiver;

use crate::NoteEvent;

use super::super::params::RenderParams;
use super::Renderers;

/// 准备渲染器实例
///
/// `is_video_export` 为 true 时跳过音符事件处理与 CC 柱状条准备，
/// 视频导出期间无用户编辑事件，避免每帧空 channel 的 `try_recv` 开销。
pub fn prepare_renderers(
    renderers: &mut Renderers,
    params: &RenderParams,
    note_events_rx: &Receiver<NoteEvent>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    is_video_export: bool,
) {
    puffin::profile_scope!("prepare_renderers");

    // 音轨总览模式：准备走带渲染器，跳过钢琴卷帘相关渲染器
    if params.is_arrangement_mode {
        renderers.arrangement.prepare(
            device,
            queue,
            params.arrangement_uniform,
            &params.arrangement_note_instances,
        );
        return;
    }

    // 准备网格渲染器
    let grid_params = crate::grid_renderer::GridPrepareParams {
        viewport_size: params.logical_size,
        scroll_x: params.scroll.0,
        scroll_y: params.scroll.1,
        zoom_x: params.zoom.0,
        zoom_y: params.zoom.1,
        keyboard_width: params.keyboard_width,
        ruler_height: params.ruler_height,
        color_bg: params.color_bg,
        color_bg_black_key: params.color_bg_black_key,
        color_bar: params.color_bar,
        color_beat: params.color_beat,
        color_half_beat: params.color_half_beat,
        color_grid: params.color_grid,
        color_key_line: params.color_key_line,
        ppq: params.ppq,
        max_key_index: params.max_key_index,
        canvas_offset_x: params.canvas_offset.0,
        canvas_offset_y: params.canvas_offset.1,
        time_signatures: params.time_signatures.clone(),
    };
    renderers.grid.prepare(queue, &grid_params);

    // 准备标尺渲染器
    if !params.ruler_instances.is_empty() {
        let ruler_params = crate::RulerPrepareParams {
            viewport_size: params.logical_size,
            ruler_height: params.ruler_height,
            keyboard_width: params.keyboard_width,
            scroll_x: params.scroll.0,
            zoom_x: params.zoom.0,
            ticks_per_measure: params.ticks_per_measure,
            ticks_per_beat: params.ticks_per_beat,
            ppq: params.ppq as u32,
            time_signatures: params.time_signatures.clone(),
        };
        renderers.ruler.prepare(device, queue, &ruler_params);
    }

    // 准备 CC 柱状条渲染器（背景/网格/中心线）
    // 视频导出不使用力度面板，此处已受 `velocity_panel_rect.is_some()` 保护
    // 弯音编辑模式也需要 prepare（遮罩矩形数据需要上传 GPU）
    if params.velocity_panel_rect.is_some() || params.pitch_bend_mode {
        renderers
            .cc_bar
            .prepare(device, queue, &params.cc_bar_instances, params.logical_size);
    }

    // 视频导出期间无音符编辑事件，跳过空 channel 的 try_recv
    if !is_video_export {
        renderers.note.process_events(note_events_rx, device, queue);
    }
}
