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
    let chunked = lumino_midi_loader::ChunkedList::from_sorted(track);
    let (start, end) = note_search_bounds(&chunked, 5_000_000, 5_007_680);
    let window_len = end - start;

    // 下界为 0，窗口从文件头开始
    assert_eq!(start, 0, "下界应固定为 0");
    // 上界仍通过二分查找限制在 tick_end 以内，不会扫描文件末尾
    assert!(window_len < TOTAL, "窗口不应覆盖全部音符");
    assert!(window_len > 0, "窗口不应为空");
    // 窗口应包含所有 start_tick <= tick_end 的音符
    assert!(chunked.get(end - 1).expect("窗口内应有音符").start_tick <= 5_007_680);
    if end < TOTAL {
        assert!(
            chunked
                .get(end)
                .expect("end < TOTAL 时 end 处应有音符")
                .start_tick
                > 5_007_680
        );
    }
}

/// 正确性：二分窗口收集结果必须与全量遍历完全一致
/// （覆盖：视口前已结束、跨视口长音符、视口内、视口后未开始）
#[test]
fn test_visible_notes_collection_matches_full_scan() {
    let doc = MidiDocument {
        notes: vec![
            lumino_midi_loader::ChunkedList::from_sorted(make_track(&[
                (0, 480, 40),               // 视口前很远，已结束
                (4_985_000, 5_001_000, 50), // 跨视口长音符（时长 16000 < BUFFER）
                (5_000_100, 5_001_000, 60), // 视口内
                (5_007_000, 5_009_000, 62), // 跨视口右边界
                (5_007_680, 5_008_000, 64), // 视口上界恰好开始
                (6_000_000, 6_000_480, 70), // 视口后很远，未开始
            ])),
            lumino_midi_loader::ChunkedList::from_sorted(make_track(&[(5_000_200, 5_000_700, 65)])),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T1".into()), Some("T2".into())],
        total_ticks: 6_000_480,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
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

/// 等价性护栏：计数分桶排序 + 偏移表必须与旧 O(N log N) 全量排序 + 扫描偏移完全一致。
/// 覆盖：多 key 交错、同 key 乱序 start_tick、空 key 桶、叠音。
#[test]
fn test_waterfall_bucket_sort_matches_full_sort() {
    const KEY_COUNT: u16 = 128;
    // 确定性伪随机（xorshift64*），避免依赖外部 rng crate
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    // 构造乱序输入：key 0-127 稀疏分布，start_tick 乱序，含叠音（同 key 同 start）
    let input: Vec<lumino_gfx::WaterfallNoteGpu> = (0..20_000)
        .map(|i| {
            let key = (next() % KEY_COUNT as u64) as u32;
            let start_tick = if i % 997 == 0 {
                // 人为制造叠音：同 key 同 start_tick
                (next() % 50_000) as u32
            } else {
                (next() % 100_000) as u32
            };
            lumino_gfx::WaterfallNoteGpu {
                key,
                start_tick,
                end_tick: start_tick + 240,
                color_packed: 0,
            }
        })
        .collect();

    // 旧实现：全量稳定排序
    let expected = {
        let mut v = input.clone();
        v.sort_by(|a, b| a.key.cmp(&b.key).then(a.start_tick.cmp(&b.start_tick)));
        v
    };
    // 新实现：计数分桶 + 桶内稳定排序
    let key_count_usize = KEY_COUNT as usize;
    let mut counts = vec![0u32; key_count_usize];
    for n in input.iter() {
        counts[n.key as usize] += 1;
    }
    let mut offsets = vec![0u32; key_count_usize + 1];
    for k in 0..key_count_usize {
        offsets[k + 1] = offsets[k] + counts[k];
    }
    let mut sorted = vec![
        lumino_gfx::WaterfallNoteGpu {
            key: 0,
            start_tick: 0,
            end_tick: 0,
            color_packed: 0,
        };
        input.len()
    ];
    let mut cursor = offsets[..key_count_usize].to_vec();
    for n in input.iter() {
        let k = n.key as usize;
        sorted[cursor[k] as usize] = *n;
        cursor[k] += 1;
    }
    let mut seg_start = 0usize;
    for k in 0..key_count_usize {
        let seg_end = offsets[k + 1] as usize;
        sorted[seg_start..seg_end].sort_by_key(|n| n.start_tick);
        seg_start = seg_end;
    }

    assert_eq!(
        tuple_of(&sorted),
        tuple_of(&expected),
        "计数分桶排序结果与全量稳定排序不一致"
    );

    // 偏移表语义：offsets[k] = 第一个 key >= k 的音符索引（与扫描实现一致）
    let mut ref_offsets = vec![0u32; key_count_usize + 1];
    {
        let mut idx = 0usize;
        for (k, slot) in ref_offsets.iter_mut().enumerate() {
            while idx < expected.len() && (expected[idx].key as usize) < k {
                idx += 1;
            }
            *slot = idx as u32;
        }
    }
    assert_eq!(offsets, ref_offsets, "计数分桶偏移表与扫描偏移表不一致");

    // 桶区间不重叠且覆盖全部音符（哨兵校验）
    for k in 0..KEY_COUNT as u32 {
        let start = offsets[k as usize] as usize;
        let end = offsets[k as usize + 1] as usize;
        for n in &sorted[start..end] {
            assert_eq!(n.key, k, "桶 {k} 包含错误 key 的音符");
        }
    }
    assert_eq!(offsets[KEY_COUNT as usize] as usize, sorted.len());

    // 确保输入未被意外修改（函数应只排序，不改变元素内容）
    let mut reconstituted = sorted.clone();
    reconstituted.sort_by_key(|n| (n.key, n.start_tick));
    let mut input_sorted = input.clone();
    input_sorted.sort_by_key(|n| (n.key, n.start_tick));
    assert_eq!(
        tuple_of(&reconstituted),
        tuple_of(&input_sorted),
        "元素内容在排序中被改变"
    );
}

/// WaterfallNoteGpu 无 PartialEq，转为字段元组比较
fn tuple_of(v: &[lumino_gfx::WaterfallNoteGpu]) -> Vec<(u32, u32, u32, u32)> {
    v.iter()
        .map(|n| (n.key, n.start_tick, n.end_tick, n.color_packed))
        .collect()
}

/// 性能对比：计数分桶 vs 全量排序 + 扫描偏移（高密集度段落，release 下运行）
/// 运行：`cargo test --release test_waterfall_bucket_sort_bench -- --nocapture`
#[test]
fn test_waterfall_bucket_sort_bench() {
    use std::time::Instant;
    const KEY_COUNT: u16 = 128;
    // 10 万音符密集段（模拟高密度段落的视口负载）
    const N: usize = 100_000;
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    let input: Vec<lumino_gfx::WaterfallNoteGpu> = (0..N)
        .map(|_| lumino_gfx::WaterfallNoteGpu {
            key: (next() % KEY_COUNT as u64) as u32,
            start_tick: (next() % 1_000_000) as u32,
            end_tick: 0,
            color_packed: 0,
        })
        .collect();

    let mut t_sort = 0u64;
    let mut t_bucket = 0u64;
    const ITERS: u32 = 20;
    for _ in 0..ITERS {
        // 旧实现：全量稳定排序 + 扫描偏移表
        let mut v = input.clone();
        let t0 = Instant::now();
        v.sort_by(|a, b| a.key.cmp(&b.key).then(a.start_tick.cmp(&b.start_tick)));
        let mut offsets = vec![0u32; KEY_COUNT as usize + 1];
        let mut idx = 0usize;
        for (k, slot) in offsets.iter_mut().enumerate() {
            while idx < v.len() && v[idx].key < k as u32 {
                idx += 1;
            }
            *slot = idx as u32;
        }
        t_sort += t0.elapsed().as_micros() as u64;
        std::hint::black_box((v, offsets));
    }
    for _ in 0..ITERS {
        // 新实现：计数分桶 + 桶内排序（偏移表即分桶结果）
        let v = input.clone();
        let t0 = Instant::now();
        let key_count_usize = KEY_COUNT as usize;
        let mut counts = vec![0u32; key_count_usize];
        for n in v.iter() {
            counts[n.key as usize] += 1;
        }
        let mut offsets = vec![0u32; key_count_usize + 1];
        for k in 0..key_count_usize {
            offsets[k + 1] = offsets[k] + counts[k];
        }
        let mut sorted = vec![
            lumino_gfx::WaterfallNoteGpu {
                key: 0,
                start_tick: 0,
                end_tick: 0,
                color_packed: 0,
            };
            v.len()
        ];
        let mut cursor = offsets[..key_count_usize].to_vec();
        for n in v.iter() {
            let k = n.key as usize;
            sorted[cursor[k] as usize] = *n;
            cursor[k] += 1;
        }
        let mut seg_start = 0usize;
        for k in 0..key_count_usize {
            let seg_end = offsets[k + 1] as usize;
            sorted[seg_start..seg_end].sort_by_key(|n| n.start_tick);
            seg_start = seg_end;
        }
        t_bucket += t0.elapsed().as_micros() as u64;
        std::hint::black_box((sorted, offsets));
    }
    eprintln!(
        "[waterfall_bench] N={N} 全量排序+扫描 {:.2}ms | 计数分桶 {:.2}ms | 加速 {:.1}x",
        t_sort as f64 / ITERS as f64 / 1000.0,
        t_bucket as f64 / ITERS as f64 / 1000.0,
        t_sort as f64 / t_bucket as f64
    );
}
