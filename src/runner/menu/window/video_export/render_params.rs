//! 视频导出帧渲染参数构建
//!
//! 将 RenderParams 构建逻辑独立拆分，便于维护。

use lumino_event::window::video::RenderMode;
use lumino_extras::palette::current_track_color_f32;
use lumino_gfx::{
    MiditrailNoteGpu, NoteInstance, RenderParams, calculate_border_width, generate_ruler_instances,
    miditrail_renderer::{MIDITRAIL_MAX_Z_FAR_DISTANCE, MIDITRAIL_SCENE_DEPTH},
    pack_color, pack_key_color,
};
use lumino_midi_loader::{MidiDocument, NoteEvent};

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

    // 每轨按 start_tick 有序 → 二分窗口定位，避免每帧 O(N) 全量遍历
    visible_notes.clear();
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        let (search_start, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in &track_notes[search_start..search_end] {
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

    // 按 (key, start_tick) 稳定排序，生成动态 key 桶偏移表。
    // shader 每像素 O(1) 定位所在 key 的桶，桶内二分回溯扫描，
    // 避免 10W+ 密集音符时全量遍历（O(N×P) → O(P×log(N/K))）。
    // 偏移表语义：`offsets[k]` = 第一个 `key >= k` 的音符索引，
    // 桶 k 的区间为 `[offsets[k], offsets[k+1])`，空桶区间自然为空。
    waterfall_notes.sort_by(|a, b| a.key.cmp(&b.key).then(a.start_tick.cmp(&b.start_tick)));
    let mut waterfall_key_offsets = vec![0u32; key_count as usize + 1];
    {
        let mut idx = 0usize;
        for (k, slot) in waterfall_key_offsets.iter_mut().enumerate() {
            while idx < waterfall_notes.len() && waterfall_notes[idx].key < k as u32 {
                idx += 1;
            }
            *slot = idx as u32;
        }
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
        waterfall_key_offsets,
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
    // 收集范围按实际 Z 显示距离缩放（而非写死最大值）：
    // GPU 实例构建中音符可见条件为 `start_tick - tick < span × z_far/SCENE_DEPTH`，
    // 因此收集窗口上界取 `tick + span × z_far/SCENE_DEPTH` 即可精确覆盖，
    // 避免默认 z_far=7.5（=SCENE_DEPTH）时白收集 2 倍音符（10 万级场景 CPU 复制与
    // GPU 扫描均减半）。z_far 拉满 15.0 时退化为 2.0×，行为与旧实现一致。
    let z_far_scale = (miditrail_z_far.max(0.1) / MIDITRAIL_SCENE_DEPTH).clamp(
        0.1 / MIDITRAIL_SCENE_DEPTH,
        MIDITRAIL_MAX_Z_FAR_DISTANCE / MIDITRAIL_SCENE_DEPTH,
    );

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

    // 每轨按 start_tick 有序 → 二分窗口定位，避免每帧 O(N) 全量遍历
    for (track_idx, track_notes) in document.notes.iter().enumerate() {
        if track_notes.is_empty() {
            continue;
        }
        let (search_start, search_end) = note_search_bounds(track_notes, tick_start, tick_end);
        for n in &track_notes[search_start..search_end] {
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
/// `pub(super)`：供 waterfall_frame.rs（CPU 瀑布流）复用同一窗口逻辑。
pub(super) fn note_search_bounds(
    track_notes: &[NoteEvent],
    _tick_start: u32,
    tick_end: u32,
) -> (usize, usize) {
    // 下界固定为 0：超长音符的 start_tick 可能远早于 tick_start - TICK_SEARCH_BUFFER，
    // 但 end_tick 仍在当前 tick 之后，必须被纳入搜索窗口。
    let search_end = track_notes.partition_point(|n| n.start_tick <= tick_end);
    (0, search_end)
}

/// GPU 可见音符临时结构
#[derive(Clone, Copy, PartialEq, Debug)]
struct GpuVisibleNote {
    key: u8,
    start_tick: u32,
    end_tick: u32,
    track_idx: u16,
    velocity: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_loader::{NoteEvent, TrackManager};

    fn make_track(notes: &[(u32, u32, u8)]) -> Vec<NoteEvent> {
        let mut v: Vec<NoteEvent> = notes
            .iter()
            .map(|&(s, e, k)| NoteEvent::new(s, e, k, 100, 0))
            .collect();
        v.sort_unstable_by_key(|n| n.start_tick);
        v
    }

    /// 正确性护栏：下界固定为 0 后，窗口从文件头开始，但上界仍通过二分查找
    /// 限制在 `tick_end` 以内，不会退化为全量扫描。
    #[test]
    fn test_note_search_bounds_window_is_small() {
        // 100 万音符均匀分布在 [0, 10_000_000) tick
        const TOTAL: usize = 1_000_000;
        let mut track = Vec::with_capacity(TOTAL);
        for i in 0..TOTAL {
            let t = (i as u32) * 10;
            track.push(NoteEvent::new(t, t + 240, 60, 100, 0));
        }

        // 视口：tick 5_000_000 起，窗口 4 小节（ppq=480 → 7680 ticks）
        let (start, end) = note_search_bounds(&track, 5_000_000, 5_007_680);
        let window_len = end - start;

        // 下界为 0，窗口从文件头开始
        assert_eq!(start, 0, "下界应固定为 0");
        // 上界仍通过二分查找限制在 tick_end 以内，不会扫描文件末尾
        assert!(window_len < TOTAL, "窗口不应覆盖全部音符");
        assert!(window_len > 0, "窗口不应为空");
        // 窗口应包含所有 start_tick <= tick_end 的音符
        assert!(track[end - 1].start_tick <= 5_007_680);
        if end < TOTAL {
            assert!(track[end].start_tick > 5_007_680);
        }
    }

    /// 正确性：二分窗口收集结果必须与全量遍历完全一致
    /// （覆盖：视口前已结束、跨视口长音符、视口内、视口后未开始）
    ///
    /// 注意：下界固定为 0 后，窗口包含所有 `start_tick <= tick_end` 的音符，
    /// 不再受固定 `TICK_SEARCH_BUFFER` 限制，超长音符也能正确保留。
    /// 最终过滤由 `end_tick > tick_start && start_tick < tick_end` 完成，
    /// 结果应与全量遍历严格一致。
    #[test]
    fn test_visible_notes_collection_matches_full_scan() {
        let doc = MidiDocument {
            notes: vec![
                make_track(&[
                    (0, 480, 40),               // 视口前很远，已结束
                    (4_985_000, 5_001_000, 50), // 跨视口长音符（时长 16000 < BUFFER）
                    (5_000_100, 5_001_000, 60), // 视口内
                    (5_007_000, 5_009_000, 62), // 跨视口右边界
                    (5_007_680, 5_008_000, 64), // 视口上界恰好开始
                    (6_000_000, 6_000_480, 70), // 视口后很远，未开始
                ]),
                make_track(&[(5_000_200, 5_000_700, 65)]),
            ],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: Vec::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("T1".into()), Some("T2".into())],
            total_ticks: 6_000_480,
            track_count: 2,
            tracks: TrackManager::new(2),
            division: 480,
            track_ports: vec![],
        };

        let tick_start = 5_000_000;
        let tick_end = tick_start + 7680;
        const KEY_COUNT: u16 = 128;

        // 窗口版（被测代码）
        let mut windowed = Vec::new();
        collect_visible_notes_for_gpu(&doc, tick_start, 480, KEY_COUNT, 1.0, 1.0, &mut windowed);

        // 全量遍历版（参考实现）
        let mut full = Vec::new();
        for (track_idx, track_notes) in doc.notes.iter().enumerate() {
            for n in track_notes {
                if n.end_tick > tick_start && n.start_tick < tick_end && n.key < KEY_COUNT as u8 {
                    full.push(GpuVisibleNote {
                        key: n.key,
                        start_tick: n.start_tick,
                        end_tick: n.end_tick,
                        track_idx: track_idx as u16,
                        velocity: n.velocity,
                    });
                }
            }
        }

        assert_eq!(windowed, full, "二分窗口收集结果与全量遍历不一致");
        // 预期可见：跨视口长音符 + 视口内 2 个 + 跨右边界 1 个
        assert_eq!(windowed.len(), 4);
    }

    /// 分桶偏移表正确性：偏移表将音符按 key 分组，桶区间非重叠且覆盖全部音符。
    /// 覆盖：空桶、稀疏 key、连续 key、哨兵偏移。
    fn build_offsets(notes: &[lumino_gfx::WaterfallNoteGpu], key_count: u16) -> Vec<u32> {
        let mut sorted = notes.to_vec();
        sorted.sort_by(|a, b| a.key.cmp(&b.key).then(a.start_tick.cmp(&b.start_tick)));
        let mut offsets = vec![0u32; key_count as usize + 1];
        let mut idx = 0usize;
        for (k, slot) in offsets.iter_mut().enumerate() {
            while idx < sorted.len() && sorted[idx].key < k as u32 {
                idx += 1;
            }
            *slot = idx as u32;
        }
        // 校验排序后的桶区间
        for k in 0..key_count as u32 {
            let start = offsets[k as usize] as usize;
            let end = offsets[k as usize + 1] as usize;
            for n in &sorted[start..end] {
                assert_eq!(n.key, k, "桶 {k} 包含错误 key 的音符");
            }
        }
        assert_eq!(offsets[key_count as usize] as usize, sorted.len());
        offsets
    }

    #[test]
    fn test_waterfall_key_offsets_partition() {
        // 稀疏 key：0、1、3（2 为空桶）、127
        let notes = vec![
            lumino_gfx::WaterfallNoteGpu {
                key: 127,
                start_tick: 100,
                end_tick: 200,
                color_packed: 0,
            },
            lumino_gfx::WaterfallNoteGpu {
                key: 0,
                start_tick: 300,
                end_tick: 400,
                color_packed: 0,
            },
            lumino_gfx::WaterfallNoteGpu {
                key: 3,
                start_tick: 50,
                end_tick: 150,
                color_packed: 0,
            },
            lumino_gfx::WaterfallNoteGpu {
                key: 1,
                start_tick: 10,
                end_tick: 20,
                color_packed: 0,
            },
        ];
        let key_count = 128u16;
        let offsets = build_offsets(&notes, key_count);

        // 桶 0/1/3 各 1 个音符，桶 2 为空
        assert_eq!(offsets[0], 0);
        assert_eq!(offsets[1], 1);
        assert_eq!(offsets[2], 2, "空桶 2 应与桶 1 末尾对齐");
        assert_eq!(offsets[3], 2);
        assert_eq!(offsets[4], 3);
        // 哨兵：全部音符数
        assert_eq!(offsets[128], 4);
    }

    #[test]
    fn test_waterfall_key_offsets_empty_and_single() {
        // 空音符：全 0
        let offsets = build_offsets(&[], 88);
        assert!(offsets.iter().all(|&o| o == 0));
        assert_eq!(offsets.len(), 89);

        // 单 key 连续多个音符
        let notes: Vec<lumino_gfx::WaterfallNoteGpu> = (0..5)
            .map(|i| lumino_gfx::WaterfallNoteGpu {
                key: 60,
                start_tick: i * 100,
                end_tick: i * 100 + 50,
                color_packed: 0,
            })
            .collect();
        let offsets = build_offsets(&notes, 88);
        assert_eq!(offsets[60], 0);
        assert_eq!(offsets[61], 5);
        // 前面的 key 全部为空桶
        assert_eq!(offsets[0], 0);
        assert_eq!(offsets[59], 0);
    }
}
