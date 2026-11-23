//! Runner 文件菜单：素材工程构建（.lmmaterial）
//!
//! 从选区音符直接构建素材工程，与 editor_midi.rs 分离，
//! 保持模块职责单一且文件长度合规。

use lumino_midi_loader::MidiDocument;

use super::editor_midi::EditorNotes;

/// 从选区音符**直接**构建素材工程（.lmmaterial 专用，不走 MIDI 字节中转）
///
/// 设计动机（修复音符长度错乱）：
/// - MIDI 中转（字节编码 → 回读）存在 f32→u32 截断与 midly NoteOn/NoteOff
///   配对语义差异，同 key 相接/重叠音符回读后长度错乱（部分变短部分变长）；
/// - 本函数直接以整数 tick 构建 `CompactEvent`（精确 round），配合稳定排序
///   （同 tick NoteOff 在 NoteOn 前）与回读端 FIFO 配对，长度无损。
///
/// 时间轴归一化（修复素材范围错位）：
/// - **开头对齐第一个音符**：所有事件（音符/控制/全局数据）平移，使素材
///   第一个音符从 tick 0 开始，片段前的空白不进入素材；
/// - **末尾为最后一个音符**：`total_ticks` = 最后一个音符结束 tick - 第一个
///   音符开始 tick，素材长度由音符内容决定，**不跟随用户框选范围**；
/// - 片段前的控制事件（CC/PC/弯音）、拍号/调号/歌词/SysEx/标记丢弃；
///   片段前的 tempo 平移后收敛到 0（素材必须保留速度信息）。
///
/// - 音符：仅选中音符（跨轨）；
/// - 控制事件（CC/PC/弯音）：仅选中轨道；
/// - 全局数据（tempo/拍号/调号/歌词/SysEx）：全量保留（含时间轴平移）。
///
/// 音轨编号压缩（修复跨轨素材"导出后没有产物"）：
/// - 归档按**槽位号**落盘（`data/project/tracks/{:03}.lmtrack`），加载端按
///   `metadata.audio.track_count` 从 0 连续读取；`build_metadata` 又把
///   `track_count` 覆写为 `project.tracks.len()`。
/// - 若沿用源工程文档轨索引（如选中轨 2、3），归档写入 `002/003` 且
///   `track_count = 4`，加载后前面多出两条空轨、音符被推迟到第 3/4 轨；
///   卷帘当前轨仍是轨 0（空）→ 用户看到"没有产物"。选中轨 0/1 时恰好
///   对齐，所以旧的双轨测试（轨 0/1）覆盖不到该缺陷。
/// - 素材是自包含片段：内部分配 `0..n-1` 连续编号，音符事件、`TrackMeta`、
///   控制事件与轨道名一并换算，归档无空洞，重新加载即为 n 轨。
///
/// tempo 直接读 `doc.tempo_changes`（单一权威源）：`EditorData` 的 tempo 写入入口
/// 都会写回 document，故它已是用户编辑后的值（2026-10-20 审计 §三：此前由调用方
/// 额外传入 `tempo_points` 补丁，现已删除）。
pub(super) fn build_material_project_from_selection(
    doc: &MidiDocument,
    selected: &EditorNotes,
) -> lumino_export::LuminoProject {
    use lumino_midi_loader::{CompactEvent, EventKind};
    use lumino_project::{LmtrackData, TrackMeta, TrackVisibilitySer};

    let mut project = lumino_export::LuminoProject::new("Material");
    project.metadata.audio.division = doc.division;

    // ── 素材时间范围：第一个音符开始 → 最后一个音符结束 ──
    let mut min_start = f32::INFINITY;
    let mut max_end = f32::NEG_INFINITY;
    for (_, notes) in selected {
        for &(tick, _, length, _, _) in notes {
            let start = tick.round();
            let end = (tick + length).round().max(start + 1.0);
            min_start = min_start.min(start);
            max_end = max_end.max(end);
        }
    }
    let offset = min_start as u32;
    let total_ticks = (max_end as u32).saturating_sub(offset).max(1);
    project.metadata.audio.total_ticks = total_ticks;

    // 全局数据：整体平移时间轴（片段前的拍号/调号/歌词/SysEx/标记丢弃，
    // 片段前的 tempo 收敛到 0 保留）。
    // 速度源直接取 document.tempo_changes（单一权威源，2026-10-20 审计 §三）：
    // `EditorData` 的 tempo 写入入口都会 sync 回 document，故这里就是编辑后的值，
    // 不再需要外部传入 tempo_points 补丁。
    project.tempo_changes = doc
        .tempo_changes
        .iter()
        .map(|&(tick, bpm)| (tick.saturating_sub(offset), bpm))
        .collect();
    project.time_signatures = doc
        .time_signatures
        .iter()
        .filter(|&&(tick, _, _)| tick >= offset)
        .map(|&(tick, num, den)| (tick - offset, num, den))
        .collect();
    project.key_signatures = doc
        .key_signatures
        .iter()
        .filter(|&&(tick, _, _)| tick >= offset)
        .map(|&(tick, sharps, minor)| (tick - offset, sharps, minor))
        .collect();
    project.lyrics = doc
        .lyrics
        .iter()
        .filter(|e| e.0 >= offset)
        .map(|e| (e.0 - offset, e.1, e.2.clone()))
        .collect();
    project.markers = doc
        .markers
        .iter()
        .filter(|e| e.0 >= offset)
        .map(|e| (e.0 - offset, e.1, e.2.clone()))
        .collect();
    project.sys_ex = doc
        .sys_ex
        .iter()
        .filter(|e| e.0 >= offset)
        .map(|e| (e.0 - offset, e.1, e.2.clone()))
        .collect();

    // 素材内部音轨编号：源文档轨索引 → 0..n-1 连续槽位（见函数文档「音轨编号压缩」）。
    // 迭代顺序沿用选区解析结果（走带按文档轨升序、卷帘单轨），保证落盘顺序稳定。
    let track_slots: std::collections::HashMap<usize, u16> = selected
        .iter()
        .enumerate()
        .map(|(slot, (doc_track, _))| (*doc_track, slot as u16))
        .collect();

    // 轨道名与素材槽位同序（归档里的 track_names 部件同样按槽位排列）
    project.track_names = selected
        .iter()
        .map(|(doc_track, _)| doc.track_name(*doc_track).map(str::to_owned))
        .collect();

    // 控制事件：仅保留选中轨道、且位于素材时间范围内的自动化数据；
    // 音轨号同步换算到素材槽位（否则 CC/PC/弯音会挂到空轨或越界）。
    for ev in &doc.control_events {
        let ev_track = ev.track; // packed 字段先拷贝，避免未对齐引用
        let Some(&material_track) = track_slots.get(&(ev_track as usize)) else {
            continue;
        };
        if ev.tick < offset {
            continue;
        }
        let new_tick = ev.tick - offset;
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                project.control_changes.push((
                    new_tick,
                    material_track,
                    ev.channel,
                    controller,
                    value,
                ));
            }
            1 => {
                let program = ev.as_program_change();
                project
                    .program_changes
                    .push((new_tick, material_track, ev.channel, program));
            }
            2 => {
                let normalized = ev.as_pitch_bend();
                let bend_offset = (normalized * 8192.0).round() as i16;
                project
                    .pitch_bends
                    .push((new_tick, material_track, ev.channel, bend_offset));
            }
            _ => {}
        }
    }

    // 每轨构建 CompactEvent（精确 round，无 f32→u32 截断）
    let mut total_notes: u64 = 0;
    for (track_id, notes) in selected {
        let Some(&material_track) = track_slots.get(track_id) else {
            continue;
        };
        let mut track_events: Vec<CompactEvent> = Vec::with_capacity(notes.len() * 2);
        for &(tick, key, length, velocity, channel) in notes {
            // 时间轴归一化：相对第一个音符平移，开头对齐 tick 0
            let start = (tick.round() as i64 - offset as i64).max(0) as u32;
            let end = ((tick + length).round() as i64 - offset as i64).max(start as i64 + 1) as u32;
            track_events.push(CompactEvent::new(
                start,
                material_track,
                EventKind::NoteOn,
                channel,
                key as u16,
                velocity as u16,
            ));
            track_events.push(CompactEvent::new(
                end,
                material_track,
                EventKind::NoteOff,
                channel,
                key as u16,
                velocity as u16,
            ));
        }
        // 稳定排序（保持声明顺序：同 tick 的 NoteOff 先于后续音符的 NoteOn）
        track_events.sort_by_key(|e| e.delta_tick());

        // 绝对 tick → 相对 delta_tick
        let mut last_tick = 0_u32;
        for ev in &mut track_events {
            let abs_tick = ev.delta_tick();
            ev.set_delta_tick(abs_tick.saturating_sub(last_tick));
            last_tick = abs_tick;
        }

        let channel = track_events
            .iter()
            .find(|ev| ev.kind().is_note())
            .map(|ev| ev.channel())
            .unwrap_or(0);
        let max_tick = track_events
            .iter()
            .scan(0_u32, |acc, ev| {
                *acc = acc.saturating_add(ev.delta_tick());
                Some(*acc)
            })
            .last()
            .unwrap_or(0);
        let name = doc.track_name(*track_id).unwrap_or("").to_string();

        let meta = TrackMeta {
            track_id: material_track,
            name,
            channel,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: channel == 9,
            max_tick,
        };
        let track_data = LmtrackData::from_compact_events(meta, &track_events);
        project.add_track(track_data);
        total_notes += notes.len() as u64;
    }

    project.metadata.audio.total_notes = total_notes;
    project.metadata.audio.track_count = selected.len() as u16;
    project
}

#[cfg(test)]
mod tests;
