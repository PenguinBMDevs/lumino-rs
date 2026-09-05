//! Miditrail 3D 模式参数（产出统一 `note_instances` + 3D uniforms）
//!
//! 单一权威飞行格式：只收集可见音符并打包为 `NoteInstance`，
//! 3D 所需的 `MiditrailNoteGpu` 由渲染线程按需换算（只读 key/start/end/color）。

use lumino_gfx::{
    RenderParams,
    miditrail_renderer::{MIDITRAIL_MAX_Z_FAR_DISTANCE, MIDITRAIL_SCENE_DEPTH, MiditrailViewMode},
};
use lumino_message::events::window::video::MiditrailViewMode as EventViewMode;

use super::{MiditrailRenderInput, SortableNote, note_search_bounds, pack_note_instances};

/// 事件层视图枚举 → GPU 层视图枚举（同构映射，无静默降级）。
fn map_view_mode(view: EventViewMode) -> MiditrailViewMode {
    match view {
        EventViewMode::Normal => MiditrailViewMode::Normal,
        EventViewMode::Top => MiditrailViewMode::Top,
    }
}

/// Miditrail 3D 模式参数
pub(crate) fn build_miditrail_render_params(input: MiditrailRenderInput) -> RenderParams {
    let MiditrailRenderInput {
        width,
        height,
        tick,
        document,
        ppq,
        key_count,
        miditrail_speed,
        miditrail_view_mode,
        miditrail_z_far,
        fps,
        visible_notes,
        note_instances_out,
    } = input;
    let miditrail_width = width.max(1) as f32;
    let miditrail_height = height.max(1) as f32;
    // 收集范围按实际 Z 显示距离缩放（而非写死最大值）：
    // GPU 实例构建中音符可见条件为 `start_tick - tick < span × z_far/SCENE_DEPTH`，
    // 因此收集窗口上界取 `tick + span × z_far/SCENE_DEPTH` 即可精确覆盖，
    // 避免默认 z_far=7.5（=SCENE_DEPTH）时白收集 2 倍音符（10 万级场景 CPU 复制与
    // GPU 扫描均减半）。z_far 拉满 15.0 时退化为 2.0×，行为与旧实现一致。
    let z_far_scale = (miditrail_z_far.max(0.1) / MIDITRAIL_SCENE_DEPTH).clamp(
        0.1 / MIDITRAIL_SCENE_DEPTH,
        MIDITRAIL_MAX_Z_FAR_DISTANCE / MIDITRAIL_SCENE_DEPTH,
    );

    // 可见 tick 窗口（与旧 collect_visible_notes_for_gpu 同公式）。
    let speed = miditrail_speed.max(0.1);
    let ticks_per_measure = ppq * 4;
    let visible_measure_count = ((4.0 / speed).round()).max(1.0) as u32;
    let viewport_tick_span =
        ((ticks_per_measure * visible_measure_count).max(1) as f32 * z_far_scale) as u32;
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span.max(1));

    visible_notes.clear();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        let (_, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in track_notes.iter().take(search_end) {
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

    super::sort_visible_notes(visible_notes);
    pack_note_instances(visible_notes, 0, note_instances_out);

    // 光晕环动画时间基准：当前 tick 处每秒 tick 数（BPM × ppq / 60）。
    // 参考 Zenith-MIDI MidiTrailRender 的 tempoFrameStep（每帧 tick 数），
    // 使光晕的按下闪光/收缩动画只随真实时间变化，与滚动速度/帧率无关。
    let ticks_per_second = (ppq as f64 * super::super::current_bpm(&document.tempo_changes, tick)
        / 60.0)
        .max(1.0) as f32;

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (miditrail_width, miditrail_height),
        scale_factor: 1.0,
        ppq: ppq as f32,
        max_key_index: (key_count.saturating_sub(1)) as f32,
        canvas_size: (miditrail_width, miditrail_height),
        miditrail_enabled: true,
        miditrail_speed: miditrail_speed.max(0.1),
        miditrail_view_mode: map_view_mode(miditrail_view_mode),
        miditrail_current_tick: tick,
        miditrail_z_far: miditrail_z_far.max(0.1),
        miditrail_ticks_per_second: ticks_per_second,
        fps,
        note_instances: std::mem::take(note_instances_out),
        time_signatures: document.time_signatures.clone(),
        ..Default::default()
    }
}
