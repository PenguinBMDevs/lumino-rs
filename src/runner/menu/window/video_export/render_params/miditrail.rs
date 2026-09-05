//! Miditrail 3D 模式参数（产出统一 `note_instances` + 3D uniforms）
//!
//! 单一权威飞行格式：收集窗口可见音符并打包为 `NoteInstance`，
//! 3D 所需的 `MiditrailNoteGpu` 由渲染线程按需换算（只读 key/start/end/color）。
//! 注意：此处必须保留逐帧窗口过滤（而非首帧全量）——24M 级文档下，
//! 全量常驻意味着 390MB 显存＋390MB 镜像＋每帧两次全量扫描（约 100ms/帧，
//! 已实测），窗口传输按可见集 V 收敛才是正确 trade-off。

use lumino_gfx::{
    RenderParams,
    miditrail_renderer::{MIDITRAIL_MAX_Z_FAR_DISTANCE, MIDITRAIL_SCENE_DEPTH, MiditrailViewMode},
};
use lumino_message::events::window::video::MiditrailViewMode as EventViewMode;

use super::{MiditrailRenderInput, collect_window_notes, pack_note_instances};

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
        window_state,
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
    note_instances_out.clear();
    // 滑动窗口收集（O(窗口变化量)，与瀑布流共用游标语义；输出与旧逐帧全前缀扫描一致）。
    let t_collect = std::time::Instant::now();
    collect_window_notes(
        document,
        tick_start,
        tick_end,
        key_count,
        window_state,
        visible_notes,
    );
    let collect_us = t_collect.elapsed().as_micros() as u64;

    let t_sort = std::time::Instant::now();
    super::sort_visible_notes(visible_notes, &mut window_state.sort_scratch);
    let sort_us = t_sort.elapsed().as_micros() as u64;
    let t_pack = std::time::Instant::now();
    pack_note_instances(visible_notes, 0, note_instances_out);
    let pack_us = t_pack.elapsed().as_micros() as u64;
    super::diag_window_collect(
        "miditrail",
        collect_us,
        sort_us,
        pack_us,
        visible_notes.len(),
    );

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
