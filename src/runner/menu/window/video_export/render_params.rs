//! 视频导出帧渲染参数构建
//!
//! 单一权威飞行格式：所有 GPU 模式统一产出 `note_instances`（`NoteInstance`），
//! 瀑布流 / 3D 所需的派生数据由渲染线程按需换算，不再各存一份。
//! 将 RenderParams 构建逻辑按渲染模式拆分为子模块：
//! - `note_rectangle`：NoteRectangle 传统钢琴卷帘矩形
//! - `waterfall`：瀑布流（产出 note_instances + 瀑布流 uniforms）
//! - `miditrail`：3D MIDI 轨迹（产出 note_instances + 3D uniforms）

use lumino_extras::palette::current_track_color_f32;
use lumino_gfx::{NoteInstance, RenderParams, calculate_border_width, pack_key_color};
use lumino_message::events::window::video::MiditrailViewMode;
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

/// 窗口收集滑动状态（瀑布流/MIDITrail 逐帧窗口收集复用，避免每帧 O(前缀) 扫描）。
///
/// 视频导出 tick 严格单调递增（frame_idx 递增），每轨窗口下界 `lo` 只向前推进：
/// 首帧（或 tick 回退/轨道数变化时重置）做一次 O(前缀) 建窗，后续每帧仅为
/// O(log 块数) 上界二分 + O(新进入/退出) 增量推进，全导出摊还 O(N)。
/// 输出集合与旧"每帧从 0 扫描"逐元素一致（含跨视口长音符）。
#[derive(Default)]
pub struct WindowCollectState {
    /// 每轨窗口下界（全局索引）：`[0, cursors[t])` 内音符已确认过期（`end <= 上帧 tick`），
    /// 直接跳过；`[cursors[t], …)` 仍需逐音符判断 `end_tick`（超长音符后的过期短音符）。
    cursors: Vec<usize>,
    /// 上帧 `tick_start`（回退检测：tick 减小即重置全部游标）。
    last_tick: u32,
    /// `sort_visible_notes` 计数排序暂存（常驻复用，消每帧 V×SortableNote 分配）。
    sort_scratch: Vec<SortableNote>,
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
    pub miditrail_view_mode: MiditrailViewMode,
    pub miditrail_normal_speed: f32,
    pub miditrail_top_speed: f32,
    pub fps: f32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    /// 首帧全量收集（全文档音符一次上传）；后续帧跳过收集，渲染线程复用 GPU 常驻数据。
    pub collect_all: bool,
    /// 瀑布流/MIDITrail 窗口收集滑动状态（tick 单调递增时增量推进，见 `WindowCollectState`）。
    pub window_state: &'a mut WindowCollectState,
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
    pub collect_all: bool,
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
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub window_state: &'a mut WindowCollectState,
}

/// MIDITrail 模式 `build_miditrail_render_params` 入参（内部分发用）
pub(crate) struct MiditrailRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub miditrail_speed: f32,
    pub miditrail_view_mode: MiditrailViewMode,
    pub miditrail_z_far: f32,
    pub fps: f32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub window_state: &'a mut WindowCollectState,
}

mod miditrail;
mod note_rectangle;
mod waterfall;

#[cfg(test)]
mod tests;

/// 按视图解析 MIDITrail 滚动速度（Normal/Top 各自独立，防互相污染）。
fn resolve_miditrail_speed(view_mode: MiditrailViewMode, normal_speed: f32, top_speed: f32) -> f32 {
    if view_mode.is_top() {
        top_speed.max(0.1)
    } else {
        normal_speed.max(0.1)
    }
}

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
        miditrail_view_mode,
        miditrail_normal_speed,
        miditrail_top_speed,
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
            fps,
            visible_notes,
            note_instances_out,
            window_state,
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

/// 滑动窗口收集：`[tick_start, tick_end)` 内可见音符（含跨视口长音符）。
///
/// 与旧"每帧从 0 扫描到 `search_end`"输出集合逐元素一致，但均摊 O(窗口变化量)：
/// - 上界：`partition_point` 二分定位（O(log 块数)），与 `note_search_bounds` 同公式；
/// - 下界：`state.cursors[t]` 只向前推进（跳过 `end_tick <= tick_start` 的过期音符），
///   tick 单调递增保证每个音符全导出最多被跳过一次；
/// - 重置：轨道数变化或 tick 回退时游标清零，下一帧重建（单次 O(前缀)，与旧行为一致）。
///
/// 调用方须保证同一导出任务内复用同一个 `state`（跨任务/Seek 必须用新 state 或触发重置）。
pub(crate) fn collect_window_notes(
    document: &MidiDocument,
    tick_start: u32,
    tick_end: u32,
    key_count: u16,
    state: &mut WindowCollectState,
    visible_notes: &mut Vec<SortableNote>,
) {
    let track_count = document.notes.len();
    if state.cursors.len() != track_count || tick_start < state.last_tick {
        state.cursors.clear();
        state.cursors.resize(track_count, 0);
    }
    state.last_tick = tick_start;

    visible_notes.clear();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        // 下界推进：踢掉已结束音符（单调 tick 下每音符最多推进一次）。
        // `get` O(log 块数)，推进总量全导出摊还 O(N)。
        let mut lo = state.cursors[track_idx];
        while let Some(n) = track_notes.get(lo) {
            if n.end_tick > tick_start {
                break;
            }
            lo += 1;
        }
        state.cursors[track_idx] = lo;
        // 上界二分：第一个 `start_tick > tick_end` 的索引（与 `note_search_bounds` 同公式）。
        let search_end = track_notes.partition_point(tick_end.wrapping_add(1));
        // 窗口内过滤：`iter_window` 经块偏移直接定位 lo，规避 `iter().skip(lo)` 的
        // O(lo) 平铺扫描。`end_tick > tick_start` 必须逐音符判断——游标只保证
        // `[0, lo)` 全过期，lo 之后的短音符仍可能已结束（如超长音符排在过期
        // 短音符之前时游标停在长音符处）。`start_tick < tick_end` 排除右边界。
        for (_, n) in track_notes.iter_window(lo, search_end) {
            if n.end_tick > tick_start && n.start_tick < tick_end && n.key < key_count as u8 {
                visible_notes.push(SortableNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    length: n.end_tick.saturating_sub(n.start_tick),
                    track_idx: track_idx as u16,
                });
            }
        }
    }
}

/// 窗口收集分段打点（首 3 帧 + 每 300 帧）：用数据验证收集/排序/打包耗时，
/// 替代"感觉慢"的体感归因。输出示例：
/// `waterfall收集打点: collect=120us sort=3400us pack=900us visible=262713`。
pub(crate) fn diag_window_collect(
    mode: &'static str,
    collect_us: u64,
    sort_us: u64,
    pack_us: u64,
    visible: usize,
) {
    static DIAG_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = DIAG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < 3 || n % 300 == 0 {
        tracing::info!(
            "{mode}收集打点[{n}]: collect={collect_us}us sort={sort_us}us pack={pack_us}us visible={visible}"
        );
    }
}
/// （单帧 10W+ 音符）排序是每帧 CPU 热点，key 范围固定时用计数分桶省去 log 因子。
/// 桶内按 (start_tick, track 倒序) 稳定排序，与原 (key, start_tick, u16::MAX - track_idx)
/// 排序键去掉 key 维度后等价。三种 GPU 模式共用，保持派生换算输入顺序一致。
///
/// `scratch` 为调用方常驻暂存（`WindowCollectState.sort_scratch`），消每帧
/// V×`SortableNote` 整块分配；返回前内容无意义，调用方不得依赖。
pub(crate) fn sort_visible_notes(
    visible_notes: &mut Vec<SortableNote>,
    scratch: &mut Vec<SortableNote>,
) {
    const KEY_BUCKETS: usize = 256;
    let mut counts = [0u32; KEY_BUCKETS];
    for n in visible_notes.iter() {
        counts[n.key as usize] += 1;
    }
    let mut offsets = [0u32; KEY_BUCKETS + 1];
    for k in 0..KEY_BUCKETS {
        offsets[k + 1] = offsets[k] + counts[k];
    }
    scratch.clear();
    scratch.reserve(visible_notes.len());
    scratch.extend(visible_notes.iter().cloned());
    let sorted_notes = scratch;
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
    std::mem::swap(visible_notes, sorted_notes);
}

/// 将已排序可见音符打包为 `NoteInstance`（wasabi 风格 border_width 由调用方按视口算好传入）。
/// 瀑布流 / 3D 模式传入 0 即可——渲染线程换算时只读 key/start/length/color，忽略边框。
pub(crate) fn pack_note_instances(
    visible_notes: &[SortableNote],
    border_width: u32,
    note_instances_out: &mut Vec<NoteInstance>,
) {
    note_instances_out.clear();
    note_instances_out.reserve(visible_notes.len());
    for n in visible_notes.iter() {
        let key_color = pack_key_color(n.key, current_track_color_f32(n.track_idx as usize));
        note_instances_out.push(NoteInstance {
            start_length: [n.start_tick as f32, (n.length as f32).max(1.0)],
            key_color,
            border_width,
        });
    }
}

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

/// 计算音符数组的二分搜索窗口 `[start, end)`（半开区间）
///
/// 保留供流式重做使用（全量常驻模式暂不需要逐帧窗口）：
/// `MidiDocument.notes` 每轨按 `start_tick` 升序排列（见 document.rs）。
/// 视口 `[tick_start, tick_end]` 内的可见音符必然满足：
/// - `start_tick <= tick_end`（音符必须已开始）；
/// - 任意时长的跨视口长音符（即使 `start_tick` 远早于 `tick_start`）只要
///   `end_tick >= tick_start` 即为可见——因此下界固定为 0，不使用固定
///   `TICK_SEARCH_BUFFER`，否则时长超过该缓冲区的超长音符在半路消失。
///   （见：`build_note_rectangle_render_params` 各模式收集逻辑）
///
/// 上界仍通过二分查找定位，避免扫描文件末尾的未开始音符。
/// `pub(crate)`：供 `waterfall_frame.rs`（CPU 瀑布流）与各 GPU 模式收集复用同一窗口逻辑。
#[allow(dead_code)] // 内存路径已切滑动窗口收集；流式重做时复用，测试仍覆盖
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
