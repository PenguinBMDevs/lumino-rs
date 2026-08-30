use super::super::params::RenderParams;
use super::Renderers;

/// 准备渲染器实例
///
/// 音符事件处理（主音轨段内增量）与洋葱皮流式上传均由 render_loop 主循环
/// 驱动，不在本函数中处理。
pub fn prepare_renderers(
    renderers: &mut Renderers,
    params: &RenderParams,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    puffin::profile_scope!("prepare_renderers");

    // 音轨总览模式：准备走带渲染器，跳过钢琴卷帘相关渲染器
    if params.is_arrangement_mode {
        puffin::profile_scope!("arrangement::prepare");
        // 音符层直接复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存）：
        // lane_index / note_uniform 已由渲染主循环依据 onion_segments + 侧栏音轨顺序
        // 预计算并写入 params；可见性裁剪交由 GPU 计算着色器一次性完成。
        let (note_source, note_instance_count) = renderers.onion_skin.gpu_note_buffer_for_sharing();
        renderers.arrangement.prepare(
            device,
            queue,
            params.arrangement_uniform,
            &params.arrangement_overlay_instances,
            params.arrangement_overlay_back_len,
            &note_source,
            note_instance_count,
            params.arrangement_note_uniform,
            &params.arrangement_lane_index,
        );
        return;
    }

    // 准备网格渲染器（纵向转置版头部对齐键盘顶部，时间向上；纵向隐藏横向标尺故 ruler=0）
    let is_vertical = params.is_vertical_roll;
    let grid_params = crate::grid_renderer::GridPrepareParams {
        viewport_size: params.logical_size,
        scroll_x: params.scroll.0,
        scroll_y: params.scroll.1,
        zoom_x: params.zoom.0,
        zoom_y: params.zoom.1,
        keyboard_width: params.keyboard_width,
        ruler_height: if is_vertical {
            0.0
        } else {
            params.ruler_height
        },
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
        canvas_size: params.canvas_size,
        time_signatures: params.time_signatures.clone(),
    };
    if is_vertical {
        renderers.vertical_grid.prepare(queue, &grid_params);
    } else {
        renderers.grid.prepare(queue, &grid_params);
    }

    // 准备标尺渲染器（纵向 wgpu 转置版已由网格着色器内置小节号文本，横向仍需 RulerRenderer）
    if !params.is_arrangement_mode && !params.is_vertical_roll {
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
    if params.velocity_panel_rect.is_some() {
        renderers
            .cc_bar
            .prepare(device, queue, &params.cc_bar_instances, params.logical_size);
    }

    // 洋葱皮流式上传：由 render_loop 主循环 drain onion_skin_streaming_rx 驱动，
    // 不在 prepare_renderers 中处理（避免与 RenderParams 全量传输耦合）。
    //
    // 主音轨事件级增量：由 render_loop 主循环 process_main_track_events 处理
    // （段内应用，GPU 布局 = 全量轨段），不在 prepare_renderers 中处理。
}
