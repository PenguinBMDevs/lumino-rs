//! 视频导出帧渲染参数构建
//!
//! 将 RenderParams 构建逻辑独立拆分，便于维护。

use lumino_event::window::video::RenderMode;
use lumino_gfx::{
    ARRANGEMENT_PALETTE, CometNoteGpu, CometRenderStyle, NoteInstance, RenderParams,
    generate_ruler_instances, pack_color,
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
/// - Comet 样式（Enhanced / MIDITrail / PFA / Velocities / Channels）：Comet compute shader 渲染
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
        RenderMode::Enhanced
        | RenderMode::MIDITrail
        | RenderMode::PFA
        | RenderMode::Velocities
        | RenderMode::Channels => build_comet_render_params(
            width,
            height,
            tick,
            document,
            ppq,
            key_count,
            render_mode,
            waterfall_scroll_speed,
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
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;

    // X 向缩放：视口 tick 范围 = 4 小节
    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let zoom_x = (w - keyboard_width) / viewport_tick_span;

    // Y 向缩放：覆盖整个键盘（固定 128 键）
    let key_count_f = KEY_COUNT as f32;
    let zoom_y = (h - ruler_height) / key_count_f;

    let scroll_x = tick as f32 * zoom_x;
    let scroll_y = 0.0f32;

    let grid_instances = Vec::new();
    let ruler_instances =
        generate_ruler_instances(w, keyboard_width, ruler_height, scroll_x, zoom_x);
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
        let color = ARRANGEMENT_PALETTE[n.track_idx as usize % ARRANGEMENT_PALETTE.len()];
        let color_packed = pack_color([color[0], color[1], color[2], 1.0]);
        note_instances_out.push(NoteInstance {
            position: [n.start_tick as f32, n.key as f32],
            size_x: (n.length as f32).max(1.0),
            color_packed,
        });
    }

    let max_key_index = (KEY_COUNT.saturating_sub(1)) as f32;
    let canvas_size = (w, h);

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (w, h),
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
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let mut notes = Vec::new();
    collect_visible_notes_for_gpu(
        document,
        tick,
        ppq,
        key_count,
        waterfall_scroll_speed,
        &mut notes,
    );

    let mut waterfall_notes = Vec::with_capacity(notes.len());
    for n in &notes {
        let color = ARRANGEMENT_PALETTE[n.track_idx as usize % ARRANGEMENT_PALETTE.len()];
        let color_packed = pack_color([color[0], color[1], color[2], 1.0]);
        waterfall_notes.push(lumino_gfx::WaterfallNoteGpu {
            key: n.key as u32,
            start_tick: n.start_tick,
            end_tick: n.end_tick,
            color_packed,
        });
    }

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (w, h),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (w, h),
        is_waterfall_mode: true,
        waterfall_speed: waterfall_scroll_speed.max(0.1),
        waterfall_notes,
        waterfall_current_tick: tick,
        ..Default::default()
    }
}

/// Comet 样式参数
#[allow(clippy::too_many_arguments)]
fn build_comet_render_params(
    width: u32,
    height: u32,
    tick: u32,
    document: &MidiDocument,
    ppq: u32,
    key_count: u16,
    render_mode: RenderMode,
    waterfall_scroll_speed: f32,
) -> RenderParams {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let style = match render_mode {
        RenderMode::Enhanced => CometRenderStyle::Enhanced,
        RenderMode::MIDITrail => CometRenderStyle::MIDITrail,
        RenderMode::PFA => CometRenderStyle::PFA,
        RenderMode::Velocities => CometRenderStyle::Velocities,
        RenderMode::Channels => CometRenderStyle::Channels,
        _ => panic!("build_comet_render_params 只应接收 Comet 样式"),
    };

    let mut notes = Vec::new();
    collect_visible_notes_for_gpu(
        document,
        tick,
        ppq,
        key_count,
        waterfall_scroll_speed,
        &mut notes,
    );

    let mut comet_notes = Vec::with_capacity(notes.len());
    for n in &notes {
        let color = ARRANGEMENT_PALETTE[n.track_idx as usize % ARRANGEMENT_PALETTE.len()];
        let color_packed = pack_color([color[0], color[1], color[2], 1.0]);
        comet_notes.push(CometNoteGpu {
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
        logical_size: (w, h),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (w, h),
        comet_style: Some(style),
        comet_speed: waterfall_scroll_speed.max(0.1),
        comet_notes,
        comet_current_tick: tick,
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
    out: &mut Vec<GpuVisibleNote>,
) {
    out.clear();
    let speed = waterfall_scroll_speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span = (ticks_per_measure * visible_measure_count).max(1);
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span);

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
