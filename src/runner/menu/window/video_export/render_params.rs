//! 视频导出帧渲染参数构建
//!
//! 将 RenderParams 构建逻辑按渲染模式拆分为子模块：
//! - `note_rectangle`：NoteRectangle 传统钢琴卷帘矩形
//! - `waterfall`：瀑布流 compute shader 渲染
//! - `miditrail`：3D MIDI 轨迹渲染
//! - `gpu_visible`：GPU 可见音符收集（瀑布流 / MIDITrail 共用）

use lumino_extras::palette::current_track_color_f32;
use lumino_gfx::{
    NoteInstance, RenderParams, calculate_border_width, generate_ruler_instances, pack_key_color,
};
use lumino_message::events::window::video::RenderMode;
use lumino_midi_loader::{MidiDocument, NoteEvent};

/// 视频导出每帧可见音符的临时数据结构
#[derive(Clone)]
pub struct SortableNote {
    pub key: u8,
    pub start_tick: u32,
    pub length: u32,
    pub track_idx: u16,
}

/// GPU 可见音符临时结构（render_params 内部共用，测试模块也需构造）
#[derive(Debug, PartialEq)]
pub(crate) struct GpuVisibleNote {
    key: u8,
    start_tick: u32,
    end_tick: u32,
    track_idx: u16,
    velocity: u8,
}

/// `build_video_export_render_params` 入参（替代 12 个位置参数，消除 `too_many_arguments`）
pub struct RenderParamsInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub render_mode: RenderMode,
    pub waterfall_scroll_speed: f32,
    pub miditrail_z_far: f32,
    pub fps: f32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
}

/// NoteRectangle 模式 `build_note_rectangle_params_from_visible` 入参
pub(crate) struct NoteRectangleParamsInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub ppq: u32,
    pub time_signatures: &'a [(u32, u8, u8)],
}

/// NoteRectangle 模式 `build_note_rectangle_render_params` 入参（内部分发用）
pub(crate) struct NoteRectangleRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
}

/// 瀑布流模式 `build_waterfall_render_params` 入参（内部分发用）
pub(crate) struct WaterfallRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub waterfall_scroll_speed: f32,
}

/// MIDITrail 模式 `build_miditrail_render_params` 入参（内部分发用）
pub(crate) struct MiditrailRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub waterfall_scroll_speed: f32,
    pub miditrail_z_far: f32,
    pub fps: f32,
}

mod gpu_visible;
mod miditrail;
mod note_rectangle;
mod waterfall;

#[cfg(test)]
mod tests;

pub(crate) use gpu_visible::collect_visible_notes_for_gpu;
pub(crate) use miditrail::build_miditrail_render_params;
pub(crate) use note_rectangle::build_note_rectangle_render_params;
pub(crate) use waterfall::build_waterfall_render_params;

/// 构建视频导出帧的 RenderParams
///
/// 根据 `render_mode` 选择渲染路径：
/// - `NoteRectangle`：传统 GPU 音符矩形渲染
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
        fps,
        visible_notes,
        note_instances_out,
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
        })),
        RenderMode::MIDITrail => Some(build_miditrail_render_params(MiditrailRenderInput {
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            waterfall_scroll_speed,
            miditrail_z_far,
            fps,
        })),
        RenderMode::NoteRectangle => {
            Some(build_note_rectangle_render_params(NoteRectangleRenderInput {
                width,
                height,
                tick,
                document,
                ppq,
                visible_notes,
                note_instances_out,
            }))
        }
        RenderMode::NoteCounter => None,
        RenderMode::DataCurve => None,
    }
}

/// 从可见音符构建 NoteRectangle 模式 RenderParams（内存模式与流式模式共享）。
///
/// 调用方负责收集可见音符（内存模式：轨道二分窗口；流式模式：线性过滤），
/// 本函数负责：计数分桶排序 + NoteInstance 构建 + RenderParams 组装。
///
/// 排序说明：按 key 计数分桶（O(N)），替代 O(N log N) 全量排序——高密集度段落
/// （单帧 10W+ 音符）排序是每帧 CPU 热点，key 范围固定时用计数分桶省去 log 因子。
/// 桶内按 (start_tick, track 倒序) 稳定排序，与原 (key, start_tick, u16::MAX - track_idx)
/// 排序键去掉 key 维度后等价。
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
    let ruler_instances = generate_ruler_instances(
        rect_width,
        keyboard_width,
        ruler_height,
        scroll_x,
        zoom_x,
        ppq,
        time_signatures,
    );

    // 按 key 计数分桶（O(N)）：可见音符已由调用方收集，此处只排序
    const KEY_BUCKETS: usize = 256;
    let mut counts = [0u32; KEY_BUCKETS];
    for n in visible_notes.iter() {
        counts[n.key as usize] += 1;
    }
    let mut offsets = [0u32; KEY_BUCKETS + 1];
    for k in 0..KEY_BUCKETS {
        offsets[k + 1] = offsets[k] + counts[k];
    }
    let mut sorted_notes = vec![
        SortableNote {
            key: 0,
            start_tick: 0,
            length: 0,
            track_idx: 0,
        };
        visible_notes.len()
    ];
    let mut cursor = offsets[..KEY_BUCKETS].to_vec();
    for n in visible_notes.iter() {
        let k = n.key as usize;
        sorted_notes[cursor[k] as usize] = n.clone();
        cursor[k] += 1;
    }
    let mut seg_start = 0usize;
    for k in 0..KEY_BUCKETS {
        let seg_end = offsets[k + 1] as usize;
        sorted_notes[seg_start..seg_end].sort_by_key(|n| (n.start_tick, u16::MAX - n.track_idx));
        seg_start = seg_end;
    }
    *visible_notes = sorted_notes;
    note_instances_out.clear();
    note_instances_out.reserve(visible_notes.len());
    // wasabi 风格 border_width：CPU 端算一次填所有音符（D2=C 决策）
    // wasabi 场景视图键轴水平 → 用 image.extent()[0]（宽度）；
    // lumino 钢琴卷帘键轴垂直 → 等价映射为画布高度（减标尺），保持 wasabi 语义
    let border_width = calculate_border_width(rect_height - ruler_height, KEY_COUNT as f32);
    for n in visible_notes.iter() {
        let key_color = pack_key_color(n.key, current_track_color_f32(n.track_idx as usize));
        note_instances_out.push(NoteInstance {
            start_length: [n.start_tick as f32, (n.length as f32).max(1.0)],
            key_color,
            border_width,
        });
    }

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
        ruler_instances,
        ppq: ppq as f32,
        max_key_index,
        canvas_size,
        time_signatures: time_signatures.to_vec(),
        ..Default::default()
    }
}

/// 计算音符数组的二分搜索窗口 `[start, end)`（半开区间）
///
/// `MidiDocument.notes` 每轨按 `start_tick` 升序排列（见 document.rs）。
/// 视口 `[tick_start, tick_end]` 内的可见音符必然满足：
/// - `start_tick <= tick_end`（音符必须已开始）；
/// - 任意时长的跨视口长音符（即使 `start_tick` 远早于 `tick_start`）只要
///   `end_tick >= tick_start` 即为可见——因此下界固定为 0，不使用固定
///   `TICK_SEARCH_BUFFER`，否则时长超过该缓冲区的超长音符在半路消失。
///   （见：`build_note_rectangle_render_params` / `collect_visible_notes_for_gpu`）
///
/// 上界仍通过二分查找定位，避免扫描文件末尾的未开始音符。
/// `pub(crate)`：供 `waterfall_frame.rs`（CPU 瀑布流）与 `gpu_visible` 复用同一窗口逻辑。
pub(crate) fn note_search_bounds(
    track_notes: &lumino_midi_loader::ChunkedList<NoteEvent>,
    _tick_start: u32,
    tick_end: u32,
) -> (usize, usize) {
    // 下界固定为 0：超长音符的 start_tick 可能远早于 tick_start - TICK_SEARCH_BUFFER，
    // 但 end_tick 仍在当前 tick 之后，必须被纳入搜索窗口。
    // 上界：第一个 start_tick > tick_end 的索引（等价于旧 partition_point(|n| n.start_tick <= tick_end)）
    let search_end = track_notes.partition_point(tick_end.wrapping_add(1));
    (0, search_end)
}
