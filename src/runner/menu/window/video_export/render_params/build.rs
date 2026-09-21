//! 视频导出 RenderParams 构建入口：按渲染模式分发
//!
//! 自 `render_params.rs` 逐字节搬移；各模式的具体构建见
//! `note_rectangle` / `waterfall` / `miditrail` 子模块。

use lumino_gfx::RenderParams;
use lumino_message::events::window::video::RenderMode;

use super::{
    MiditrailRenderInput, NoteRectangleRenderInput, RenderParamsInput, WaterfallRenderInput,
    build_miditrail_render_params, build_note_rectangle_render_params,
    build_waterfall_render_params, resolve_miditrail_speed,
};

/// 构建视频导出帧的 RenderParams
///
/// 根据 `render_mode` 选择渲染路径：
/// - `NoteRectangle`（Lumino卷帘）：传统 GPU 音符矩形渲染
/// - `Waterfall`：瀑布流 compute shader 渲染
/// - `MIDITrail`：3D MIDI 轨迹渲染
pub fn build_video_export_render_params(input: RenderParamsInput) -> Option<RenderParams> {
    let RenderParamsInput {
        width,
        height,
        tick,
        document,
        ppq,
        key_count,
        render_mode,
        waterfall_scroll_speed,
        miditrail_z_far,
        miditrail_view_mode,
        miditrail_normal_speed,
        miditrail_top_speed,
        miditrail_3d_notes,
        fps,
        visible_notes,
        note_instances_out,
        collect_all,
        window_state,
    } = input;
    match render_mode {
        RenderMode::Waterfall => Some(build_waterfall_render_params(WaterfallRenderInput {
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            waterfall_scroll_speed,
            visible_notes,
            note_instances_out,
            window_state,
            collect_all,
        })),
        RenderMode::MIDITrail => Some(build_miditrail_render_params(MiditrailRenderInput {
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            miditrail_speed: resolve_miditrail_speed(
                miditrail_view_mode,
                miditrail_normal_speed,
                miditrail_top_speed,
            ),
            miditrail_view_mode,
            miditrail_z_far,
            miditrail_3d_notes,
            fps,
            visible_notes,
            note_instances_out,
            window_state,
            collect_all,
        })),
        RenderMode::NoteRectangle => Some(build_note_rectangle_render_params(
            NoteRectangleRenderInput {
                width,
                height,
                tick,
                document,
                ppq,
                visible_notes,
                note_instances_out,
                collect_all,
            },
        )),
        RenderMode::NoteCounter => None,
        RenderMode::DataCurve => None,
        RenderMode::MidiConsole => None,
    }
}
