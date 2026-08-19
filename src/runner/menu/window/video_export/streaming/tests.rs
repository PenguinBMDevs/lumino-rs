use super::super::seconds_to_tick;
use super::*;
use lumino_midi_loader::TICK_SEARCH_BUFFER;

const PPQN: u32 = 480;
const FPS: f64 = 10.0;
const TEMPOS: [(u32, f32); 1] = [(0, 120.0)];

fn make_records(n: u32, total_ticks: u32) -> Vec<NoteRecord> {
    let mut records: Vec<NoteRecord> = (0..n)
        .map(|i| {
            // 均匀分布：间隔约 total_ticks / n，音符时长 240 ticks
            let t = i * (total_ticks / n).max(1);
            NoteRecord {
                start_tick: t,
                end_tick: t.saturating_add(240),
                key: 60,
                velocity: 100,
                track: 0,
                channel: 0,
            }
        })
        .collect();
    // 跨视口长音符（时长 = 2400 < TICK_SEARCH_BUFFER，必须被窗口覆盖）
    records.push(NoteRecord {
        start_tick: 1_000,
        end_tick: 3_400,
        key: 61,
        velocity: 100,
        track: 0,
        channel: 0,
    });
    records.sort_unstable_by_key(|r| r.start_tick);
    records
}

/// 正确性：每个帧的窗口必须覆盖该帧视口内所有**时长不超过
/// `TICK_SEARCH_BUFFER`** 的可见音符（超集性质）。
///
/// 可见判定与 `build_video_render_params_from_notes` 的过滤条件一致：
/// `end_tick >= vp_start && start_tick <= vp_end`。
///
/// 注意：时长超过 `TICK_SEARCH_BUFFER` 的超长跨视口音符会被窗口下界跳过，
/// 这是与内存模式 `MidiDocument::get_track_notes_in_range` 一致的既有取舍。
#[test]
fn test_frame_index_window_covers_all_visible_notes() {
    let total_ticks = 576_000u32; // 10 分钟 @120bpm
    let records = make_records(200, total_ticks);

    let index = build_frame_index(&records, PPQN, total_ticks, &TEMPOS, FPS, 16.0)
        .expect("build_frame_index 不应失败");
    assert!(!index.is_empty());

    let viewport_span = (PPQN as f64 * 16.0) as u32;
    for (frame_idx, entry) in index.iter().enumerate() {
        let frame_time = frame_idx as f64 / FPS;
        let vp_start = seconds_to_tick(frame_time, &TEMPOS, PPQN);
        let vp_end = vp_start.saturating_add(viewport_span);
        let range = entry.note_offset as usize..(entry.note_offset + entry.note_count) as usize;

        for (i, r) in records.iter().enumerate() {
            let is_visible = r.end_tick >= vp_start && r.start_tick <= vp_end;
            let within_buffer = r.end_tick.saturating_sub(r.start_tick) <= TICK_SEARCH_BUFFER;
            if is_visible && within_buffer {
                assert!(
                    range.contains(&i),
                    "帧 {frame_idx} (vp {vp_start}..{vp_end}) 遗漏可见记录 {i}: \
                      start={} end={}",
                    r.start_tick,
                    r.end_tick,
                );
            }
        }
    }
}

/// 性能护栏：大文件下每帧窗口大小必须远小于总记录数。
///
/// 旧实现窗口 = [视口起点, 文件末尾)，帧 0 即读取全部记录（O(N) 每帧），
/// 导出速度随总音符数线性下降。修复后窗口仅覆盖视口附近
/// （±TICK_SEARCH_BUFFER 扩展），大小与总记录数无关。
#[test]
fn test_frame_index_window_stays_small_for_large_files() {
    let total_ticks = 576_000u32; // 10 分钟 @120bpm
    const RECORD_COUNT: usize = 10_000;
    let records = make_records(RECORD_COUNT as u32, total_ticks);

    let index = build_frame_index(&records, PPQN, total_ticks, &TEMPOS, FPS, 16.0)
        .expect("build_frame_index 不应失败");

    let max_window = index
        .iter()
        .map(|e| e.note_count as usize)
        .max()
        .expect("帧索引不应为空");
    // 窗口仅覆盖视口 ± 缓冲区（~34560 ticks / 576000 ticks ≈ 6% 的记录），
    // 远小于总数；旧实现首帧窗口 = RECORD_COUNT。
    assert!(
        max_window * 20 < RECORD_COUNT,
        "帧索引窗口过大: max={max_window}, 总记录={RECORD_COUNT}"
    );
}
