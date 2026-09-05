use crate::miditrail_renderer::pack_color;
use crate::render_thread::export_pipeline::ExportPipeline;
use crate::render_thread::render_loop::Renderers;
use crate::render_thread::render_loop::runner::context::{
    RenderFrameState, VideoExportFrameContext, VideoExportSetupContext,
};
use crate::{MiditrailNoteGpu, NoteInstance, WaterfallNoteGpu, unpack_key_color};

use super::handle_video_frame;

/// 启动视频导出：初始化 GPU→CPU 读回管线与专用 2D 渲染器。
pub(crate) fn start_video_export(context: VideoExportSetupContext<'_>) {
    tracing::info!(
        "视频导出开始: {}x{}, 初始化 GPU→CPU 读回管线",
        context.width,
        context.height
    );
    let mut pipeline = ExportPipeline::new(&context.ctx.device, context.width, context.height);
    pipeline.set_recycle_receiver(context.recycle_rx);
    *context.export_pipeline = Some(pipeline);
    *context.export_frame_tx = Some(context.frame_tx);
    // 创建无 depth attachment 的视频导出专用渲染器
    *context.export_renderers = Some(Renderers::new_for_video_export(
        &context.ctx.device,
        &context.ctx.queue,
        context.ctx.texture_format,
    ));
}

/// 渲染一帧视频导出：构造每帧状态并交给对应渲染模式。
pub(crate) fn render_video_frame_command(context: VideoExportFrameContext<'_>) {
    // 视频导出帧使用纯 2D 渲染器，确保 pipeline 与无 depth 的 RenderPass 兼容。
    let video_renderers = context
        .export_renderers
        .as_mut()
        .unwrap_or(&mut *context.renderers);
    let mut frame = RenderFrameState {
        renderers: video_renderers,
        current_texture: context.current_texture,
        depth_texture: context.depth_texture,
        depth_texture_view: context.depth_texture_view,
        texture_view: context.texture_view,
        current_size: context.current_size,
        last_note_version: context.last_note_version,
        latest_texture_clone: &context.channels.latest_texture_clone,
        texture_waterfall_renderer: context.texture_waterfall_renderer,
        texture_waterfall_meta: context.texture_waterfall_meta,
        texture_waterfall_config: context.texture_waterfall_config,
        export_pipeline: context.export_pipeline,
        export_frame_tx: context.export_frame_tx,
        waterfall_renderer: context.waterfall_renderer,
        miditrail_renderer: context.miditrail_renderer,
    };
    handle_video_frame(context.ctx, context.params, &mut frame, context.channels);
}

/// 从权威 `note_instances` 换算瀑布流派生数据（key 分桶排序 + 偏移表）。
///
/// 替代已删除的 `RenderParams.waterfall_notes / waterfall_key_offsets` 跨线程 Vec：
/// 颜色经 `key_color` 打包往返（通道误差 ≤1/255，视觉无差），起止由
/// `start_length` 还原，排序语义与旧生产侧一致（桶内按 start 稳定排序，
/// 满足 shader 桶内二分回溯前提）。
pub(crate) fn note_instances_to_waterfall(
    notes: &[NoteInstance],
    key_count: usize,
) -> (Vec<WaterfallNoteGpu>, Vec<u32>) {
    let key_count = key_count.max(1);
    let mut derived = Vec::with_capacity(notes.len());
    for n in notes {
        let (key, rgb) = unpack_key_color(n.key_color);
        if (key as usize) >= key_count {
            continue;
        }
        let start = n.start_length[0].max(0.0) as u32;
        let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
        derived.push(WaterfallNoteGpu {
            key: key as u32,
            start_tick: start,
            end_tick: end,
            color_packed: pack_color([rgb[0], rgb[1], rgb[2], 1.0]),
        });
    }
    let mut counts = vec![0u32; key_count];
    for n in &derived {
        counts[n.key as usize] += 1;
    }
    let mut offsets = vec![0u32; key_count + 1];
    for k in 0..key_count {
        offsets[k + 1] = offsets[k] + counts[k];
    }
    let mut sorted = vec![
        WaterfallNoteGpu {
            key: 0,
            start_tick: 0,
            end_tick: 0,
            color_packed: 0,
        };
        derived.len()
    ];
    let mut cursor = offsets[..key_count].to_vec();
    for n in &derived {
        let k = n.key as usize;
        sorted[cursor[k] as usize] = *n;
        cursor[k] += 1;
    }
    let mut seg_start = 0usize;
    for k in 0..key_count {
        let seg_end = offsets[k + 1] as usize;
        sorted[seg_start..seg_end].sort_by_key(|n| n.start_tick);
        seg_start = seg_end;
    }
    (sorted, offsets)
}

/// 从权威 `note_instances` 换算 3D 派生数据。
///
/// 实例构建只读 key/start/end/color（见 `build_note_instances`），
/// track/力度/通道填默认值即可，行为与旧生产侧一致。
pub(crate) fn note_instances_to_miditrail(notes: &[NoteInstance]) -> Vec<MiditrailNoteGpu> {
    let mut derived = Vec::with_capacity(notes.len());
    for n in notes {
        let (key, rgb) = unpack_key_color(n.key_color);
        let start = n.start_length[0].max(0.0) as u32;
        let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
        derived.push(MiditrailNoteGpu {
            key: key as u32,
            start_tick: start,
            end_tick: end,
            color_packed: pack_color([rgb[0], rgb[1], rgb[2], 1.0]),
            track_idx: 0,
            velocity: 100,
            channel: 0,
            _padding: 0,
        });
    }
    derived
}
