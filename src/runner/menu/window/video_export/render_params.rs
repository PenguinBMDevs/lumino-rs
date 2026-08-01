//! 视频导出帧渲染参数构建
//!
//! 将 RenderParams 构建逻辑独立拆分，便于维护。

use lumino_event::window::video::RenderMode;
use lumino_extras::palette::current_track_color_f32;
use lumino_gfx::{
    MiditrailNoteGpu, NoteInstance, RenderParams, generate_ruler_instances,
    miditrail_renderer::{MIDITRAIL_MAX_Z_FAR_DISTANCE, MIDITRAIL_SCENE_DEPTH},
    pack_color,
};
use lumino_midi_loader::MidiDocument;

/// 视频导出每帧可见音符的临时数据结构
#[derive(Clone)]
pub struct SortableNote {
    pub key: u8,
    pub start_tick: u32,
    pub length: u32,
    pub track_idx: u16,
}

/// 构建视频导出帧的 RenderParams
///
/// 根据 `render_mode` 选择渲染路径：
/// - `NoteRectangle`：传统 GPU 音符矩形渲染
/// - `Waterfall`：瀑布流 compute shader 渲染
/// - `MIDITrail`：3D MIDI 轨迹渲染
#[allow(clippy::too_many_arguments)]
pub fn build_video_export_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &MidiDocument,
    ppq: u32,
    key_count: u16,
    render_mode: RenderMode,
    waterfall_scroll_speed: f32,
    miditrail_z_far: f32,
    fps: f32,
    visible_notes: &mut Vec<SortableNote>,
    note_instances_out: &mut Vec<NoteInstance>,
) -> RenderParams {
    match render_mode {
        RenderMode::Waterfall => build_waterfall_render_params(
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            waterfall_scroll_speed,
        ),
        RenderMode::MIDITrail => build_miditrail_render_params(
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            waterfall_scroll_speed,
            miditrail_z_far,
            fps,
        ),
        RenderMode::NoteRectangle => build_note_rectangle_render_params(
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            visible_notes,
            note_instances_out,
        ),
    }
}

/// NoteRectangle 模式：传统钢琴卷帘音符矩形
#[allow(clippy::too_many_arguments)]
fn build_note_rectangle_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &MidiDocument,
    ppq: u32,
    _key_count: u16,
    visible_notes: &mut Vec<SortableNote>,
    note_instances_out: &mut Vec<NoteInstance>,
) -> RenderParams {
    // 视频导出始终使用标准 128 键 MIDI 键盘
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
        &document.time_signatures,
    );
    let keyboard_instances = Vec::new();

    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);

    visible_notes.clear();
    for (track_idx, notes) in document.notes.iter().enumerate() {
        for n in notes {
            if n.end_tick >= tick_start && n.start_tick <= tick_end {
                visible_notes.push(SortableNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    length: n.length(),
                    track_idx: track_idx as u16,
                });
            }
        }
    }
    visible_notes.sort_by_key(|n| (n.key, n.start_tick, u16::MAX - n.track_idx));
    note_instances_out.clear();
    note_instances_out.reserve(visible_notes.len());
    for n in visible_notes.iter() {
        let color_packed = pack_color(current_track_color_f32(n.track_idx as usize));
        note_instances_out.push(NoteInstance {
            position: [n.start_tick as f32, n.key as f32],
            size_x: (n.length as f32).max(1.0),
            color_packed,
            flags: 0,
            _padding: 0,
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
        keyboard_instances,
        ppq: ppq as f32,
        max_key_index,
        canvas_size,
        time_signatures: document.time_signatures.clone(),
        ..Default::default()
    }
}

/// 瀑布流模式参数
#[allow(clippy::too_many_arguments)]
fn build_waterfall_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &MidiDocument,
    ppq: u32,
    key_count: u16,
    waterfall_scroll_speed: f32,
) -> RenderParams {
    let waterfall_width = width.max(1) as f32;
    let waterfall_height = height.max(1) as f32;
    let mut notes = Vec::new();
    collect_visible_notes_for_gpu(
        document,
        tick,
        ppq,
        key_count,
        waterfall_scroll_speed,
        1.0,
        &mut notes,
    );

    let mut waterfall_notes = Vec::with_capacity(notes.len());
    for n in &notes {
        let color_packed = pack_color(current_track_color_f32(n.track_idx as usize));
        waterfall_notes.push(lumino_gfx::WaterfallNoteGpu {
            key: n.key as u32,
            start_tick: n.start_tick,
            end_tick: n.end_tick,
            color_packed,
        });
    }

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (waterfall_width, waterfall_height),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (waterfall_width, waterfall_height),
        is_waterfall_mode: true,
        waterfall_speed: waterfall_scroll_speed.max(0.1),
        waterfall_notes,
        waterfall_current_tick: tick,
        time_signatures: document.time_signatures.clone(),
        ..Default::default()
    }
}

/// Miditrail 3D 模式参数
#[allow(clippy::too_many_arguments)]
fn build_miditrail_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &MidiDocument,
    ppq: u32,
    key_count: u16,
    waterfall_scroll_speed: f32,
    miditrail_z_far: f32,
    fps: f32,
) -> RenderParams {
    let miditrail_width = width.max(1) as f32;
    let miditrail_height = height.max(1) as f32;
    // 为支持 Z 显示距离拉到最大，收集范围按最大倍数扩展；
    // 实际截断由 `miditrail_z_far` 在 GPU 实例构建阶段控制。
    let z_far_scale = MIDITRAIL_MAX_Z_FAR_DISTANCE / MIDITRAIL_SCENE_DEPTH;

    let mut notes = Vec::new();
    collect_visible_notes_for_gpu(
        document,
        tick,
        ppq,
        key_count,
        waterfall_scroll_speed,
        z_far_scale,
        &mut notes,
    );

    let mut miditrail_notes = Vec::with_capacity(notes.len());
    for n in &notes {
        let color_packed = pack_color(current_track_color_f32(n.track_idx as usize));
        miditrail_notes.push(MiditrailNoteGpu {
            key: n.key as u32,
            start_tick: n.start_tick,
            end_tick: n.end_tick,
            color_packed,
            track_idx: n.track_idx as u32,
            velocity: n.velocity as u32,
            channel: (n.track_idx % 16) as u32,
            _padding: 0,
        });
    }

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (miditrail_width, miditrail_height),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (miditrail_width, miditrail_height),
        miditrail_enabled: true,
        miditrail_speed: waterfall_scroll_speed.max(0.1),
        miditrail_notes,
        miditrail_current_tick: tick,
        miditrail_z_far: miditrail_z_far.max(0.1),
        fps,
        time_signatures: document.time_signatures.clone(),
        ..Default::default()
    }
}

/// 收集 GPU 渲染所需的可见音符
fn collect_visible_notes_for_gpu(
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    key_count: u16,
    waterfall_scroll_speed: f32,
    viewport_scale: f32,
    out: &mut Vec<GpuVisibleNote>,
) {
    out.clear();
    let speed = waterfall_scroll_speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span =
        (ticks_per_measure * visible_measure_count).max(1) as f32 * viewport_scale;
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);

    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        for n in track_notes {
            if n.end_tick > tick_start && n.start_tick < tick_end && n.key < key_count as u8 {
                out.push(GpuVisibleNote {
                    key: n.key,
                    start_tick: n.start_tick,
                    end_tick: n.end_tick,
                    track_idx: track_idx as u16,
                    velocity: n.velocity,
                });
            }
        }
    }
}

/// GPU 可见音符临时结构
#[derive(Clone, Copy)]
struct GpuVisibleNote {
    key: u8,
    start_tick: u32,
    end_tick: u32,
    track_idx: u16,
    velocity: u8,
}
