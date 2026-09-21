//! NoteRectangle 模式共享的 RenderParams 构建（内存模式与流式模式共用）

use lumino_gfx::{RenderParams, calculate_border_width};

use super::{NoteRectangleParamsInput, pack_note_instances, sort_visible_notes};

/// 从可见音符构建 NoteRectangle 模式 RenderParams（内存模式与流式模式共享）。
///
/// 调用方负责收集可见音符（内存模式：轨道二分窗口；流式模式：线性过滤），
/// 本函数负责：计数分桶排序 + NoteInstance 构建 + RenderParams 组装。
pub(crate) fn build_note_rectangle_params_from_visible(
    input: NoteRectangleParamsInput,
) -> RenderParams {
    let NoteRectangleParamsInput {
        width,
        height,
        tick,
        visible_notes,
        note_instances_out,
        ppq,
        time_signatures,
    } = input;
    const KEY_COUNT: u16 = 128;

    let keyboard_width = 60.0f32;
    let ruler_height = 30.0f32;
    let rect_width = width.max(1) as f32;
    let rect_height = height.max(1) as f32;

    // X 向缩放：视口 tick 范围 = 4 小节
    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let zoom_x = (rect_width - keyboard_width) / viewport_tick_span;

    // Y 向缩放：覆盖整个键盘（固定 128 键）
    let key_count_f = KEY_COUNT as f32;
    let zoom_y = (rect_height - ruler_height) / key_count_f;

    let scroll_x = tick as f32 * zoom_x;
    let scroll_y = 0.0f32;

    let grid_instances = Vec::new();
    // 注：标尺刻度渲染线程按 scroll/zoom 内部重算并缓存（见 RulerRenderer::prepare），
    // 此处不再逐帧生成跨线程 Vec（渲染侧零读取，旧逻辑纯浪费）。

    // 按 key 计数分桶排序（O(N)，见 sort_visible_notes）
    // 钢琴模式稳态 visible 为空（首帧全量后 GPU 常驻），局部暂存零分配；
    // 首帧全量排序分配一次，与旧行为一致。
    let mut sort_scratch = Vec::new();
    sort_visible_notes(visible_notes, &mut sort_scratch);
    // wasabi 风格 border_width：CPU 端算一次填所有音符（D2=C 决策）
    // wasabi 场景视图键轴水平 → 用 image.extent()[0]（宽度）；
    // lumino 钢琴卷帘键轴垂直 → 等价映射为画布高度（减标尺），保持 wasabi 语义
    let border_width = calculate_border_width(rect_height - ruler_height, KEY_COUNT as f32);
    pack_note_instances(visible_notes, border_width, note_instances_out);

    let max_key_index = (KEY_COUNT.saturating_sub(1)) as f32;
    let canvas_size = (rect_width, rect_height);

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (rect_width, rect_height),
        scale_factor: 1.0,
        scroll: (scroll_x, scroll_y),
        zoom: (zoom_x, zoom_y),
        keyboard_width,
        ruler_height,
        note_instances: std::mem::take(note_instances_out),
        grid_instances,
        ruler_instances: Vec::new(),
        ppq: ppq as f32,
        max_key_index,
        canvas_size,
        time_signatures: time_signatures.to_vec(),
        ..Default::default()
    }
}
