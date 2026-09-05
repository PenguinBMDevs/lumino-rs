//! Miditrail 3D 模式参数（产出统一 `note_instances` + 3D uniforms）
//!
//! 单一权威飞行格式：只收集可见音符并打包为 `NoteInstance`，
//! 3D 所需的 `MiditrailNoteGpu` 由渲染线程按需换算（只读 key/start/end/color）。

use lumino_gfx::{RenderParams, miditrail_renderer::MiditrailViewMode};
use lumino_message::events::window::video::MiditrailViewMode as EventViewMode;

use super::{MiditrailRenderInput, SortableNote, pack_note_instances};

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
        collect_all,
    } = input;
    let miditrail_width = width.max(1) as f32;
    let miditrail_height = height.max(1) as f32;

    // 首帧全量收集（一次上传常驻 GPU）；后续帧跳过，可见过滤由渲染线程读镜像完成。
    visible_notes.clear();
    note_instances_out.clear();
    if collect_all {
        for (track_idx, track_notes) in document.notes.iter().enumerate() {
            for n in track_notes.iter() {
                if n.key < key_count as u8 {
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
    }

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
