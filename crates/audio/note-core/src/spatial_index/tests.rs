//! 空间索引单测。

use super::*;
use crate::note::Note;
use std::time::Instant;

#[test]
fn test_spatial_index_performance() {
    puffin::set_scopes_on(true);

    let mut notes = Vec::new();
    let num_notes = 100_000;
    for i in 0..num_notes {
        notes.push(Note {
            tick: (i % 10000) as f32 * 10.0,
            key: (i % 128) as u16,
            length: 20.0,
            velocity: 100,
            channel: 0,
        });
    }

    let start = Instant::now();
    let index = NoteSpatialIndex::from_notes(&notes);
    println!("Build tree took: {:?}", start.elapsed());

    let mut result = Vec::new();
    let start = Instant::now();
    for i in 0..1000 {
        let tick_start = (i % 1000) as f32 * 50.0;
        let tick_end = tick_start + 1000.0;
        index.update_query(tick_start, tick_end, 40, 80, &mut result);
    }
    println!("1000 queries took: {:?}", start.elapsed());
    assert!(!result.is_empty());
}

/// 构造三条**首尾相接**的边界音符（用于验证两种 tick 口径的差异）：
/// - C = `[0, 100)`
/// - A = `[100, 200)`
/// - B = `[200, 300)`
fn boundary_notes() -> Vec<Note> {
    vec![
        Note {
            tick: 0.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
        Note {
            tick: 100.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
        Note {
            tick: 200.0,
            key: 60,
            length: 100.0,
            velocity: 100,
            channel: 0,
        },
    ]
}

fn sorted(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v
}

/// 闭区间 `[100, 200]`：三条音符的首尾边界全部与之相触 → 全命中。
///
/// 这是视口裁剪/播放头活跃判定依赖的语义（守卫：不得被框选半开化改动污染）。
#[test]
fn test_update_query_closed_interval_keeps_touching_notes() {
    let index = NoteSpatialIndex::from_notes(&boundary_notes());
    let mut result = Vec::new();
    index.update_query(100.0, 200.0, 0, 127, &mut result);
    assert_eq!(
        sorted(result),
        vec![0, 1, 2],
        "闭区间下：终点恰等于左边界(C)、起点恰等于右边界(B) 都应命中"
    );
}

/// 播放活跃判定守卫：`update_query(tick, tick, ..)` 零宽闭区间必须命中
/// 「起点恰在播放头」与「终点恰在播放头」的音符。
///
/// 回归背景：`impls/playback.rs` 以 `update_query(tick, tick, 0, 255, ..)`
/// 取播放头活跃音符，随后自行以 `end_tick > tick` 收口。若把本方法半开化，
/// 起点恰在播放头的音符会漏取 → 起播瞬间第一个音符不高亮。
#[test]
fn test_update_query_zero_width_still_hits_playhead_notes() {
    let index = NoteSpatialIndex::from_notes(&boundary_notes());
    let mut result = Vec::new();
    index.update_query(100.0, 100.0, 0, 127, &mut result);
    assert_eq!(
        sorted(result.clone()),
        vec![0, 1],
        "零宽闭区间：终点=100(C) 与 起点=100(A) 均须命中（播放头活跃判定依赖）"
    );

    // 半开口径下零宽区间必然为空（» 用于对照，说明两者不可互换）
    index.update_query_marquee(100.0, 100.0, 0, 127, &mut result);
    assert!(
        result.is_empty(),
        "半开零宽区间为空——框选语义与播放活跃判定语义不同，禁止合并"
    );
}

/// 半开区间 `[100, 200)`：仅边界相触的 C、B 被排除，内部交叠的 A 保留。
///
/// 这是框选（marquee）依赖的语义：低精度模式下框缘贴边音符不被误选。
#[test]
fn test_update_query_marquee_half_open_excludes_touching_notes() {
    let index = NoteSpatialIndex::from_notes(&boundary_notes());
    let mut result = Vec::new();
    index.update_query_marquee(100.0, 200.0, 0, 127, &mut result);
    assert_eq!(
        sorted(result),
        vec![1],
        "半开区间下：仅内部有交叠的 A[100,200) 入选；\
         C 终点=100 与 B 起点=200 仅边界相触，必须排除"
    );
}

/// 跨越框边界的长音符（内部有交叠）仍须入选——半开不等于「包含」。
#[test]
fn test_update_query_marquee_keeps_straddling_long_note() {
    let notes = vec![Note {
        tick: 0.0,
        key: 60,
        length: 1000.0,
        velocity: 100,
        channel: 0,
    }];
    let index = NoteSpatialIndex::from_notes(&notes);
    let mut result = Vec::new();
    index.update_query_marquee(500.0, 600.0, 0, 127, &mut result);
    assert_eq!(
        sorted(result),
        vec![0],
        "长音符 [0,1000) 跨越 [500,600) 且内部交叠，必须入选"
    );
}

/// key 轴保持闭区间（整格覆盖语义）：key 上下边界都必须命中。
#[test]
fn test_update_query_marquee_key_axis_stays_closed() {
    let notes: Vec<Note> = (60..=62)
        .map(|key| Note {
            tick: 0.0,
            key,
            length: 100.0,
            velocity: 100,
            channel: 0,
        })
        .collect();
    let index = NoteSpatialIndex::from_notes(&notes);
    let mut result = Vec::new();
    index.update_query_marquee(0.0, 100.0, 60, 62, &mut result);
    assert_eq!(
        sorted(result.clone()),
        vec![0, 1, 2],
        "key 轴闭区间：上下边界 key 均须命中"
    );

    index.update_query_marquee(0.0, 100.0, 61, 61, &mut result);
    assert_eq!(sorted(result), vec![1], "单个 key 只命中该 key");
}

/// 多节点树（> `MAX_LEAF_CAPACITY`）下，半开查询必须与暴力过滤完全一致。
///
/// 覆盖节点级剪枝（`node_tick_may_hit`）与叶子判定的同口径一致性：
/// 剪枝不能剪掉可能命中的节点，否则会出现「树大小影响结果」的口径分裂。
#[test]
fn test_update_query_marquee_matches_brute_force_on_multi_node_tree() {
    // 400 条音符：长度 1..=8，tick 密集重叠，必然触发多级节点切分
    let notes: Vec<Note> = (0..400u32)
        .map(|i| Note {
            tick: (i % 97) as f32 * 10.0,
            key: (i % 128) as u16,
            length: ((i % 8) + 1) as f32 * 10.0,
            velocity: 100,
            channel: 0,
        })
        .collect();
    let index = NoteSpatialIndex::from_notes(&notes);
    assert!(notes.len() > NoteSpatialIndex::MAX_LEAF_CAPACITY);

    let mut result = Vec::new();
    // 遍历多条查询窗，含与音符边界精确对齐的场景（步长 10 = tick 粒度）
    for t0 in (0..960).step_by(10) {
        for span in [0.0_f32, 5.0, 10.0, 100.0] {
            let t1 = t0 as f32 + span;
            index.update_query_marquee(t0 as f32, t1, 0, 127, &mut result);
            let got = sorted(result.clone());
            let expected: Vec<usize> = notes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.tick + n.length > t0 as f32 && n.tick < t1)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                got, expected,
                "半开查询 [{t0}, {t1}) 与暴力过滤不一致（树剪枝口径错误）"
            );
        }
    }
}
