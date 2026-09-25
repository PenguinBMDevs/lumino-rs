//! 段表纯函数测试（自 `onion_segments.rs` 拆出，保持主文件 < 400 行）

use super::*;

fn seg(track_id: usize, offset: usize, len: usize) -> OnionSegment {
    OnionSegment {
        track_id,
        offset,
        len,
    }
}

fn layout() -> Vec<OnionSegment> {
    vec![
        seg(0, 0, 100),
        seg(1, 100, 50),
        seg(2, 150, 30),
        seg(3, 180, 20),
    ]
}

/// 变长替换后，计算新的段表偏移（纯函数，可单测）
///
/// `segments` 为替换前的段表，`idx` 为被替换段，`new_len` 为替换后段长。
/// 返回替换后的完整段表（后续段 offset 平移 delta）。
pub(crate) fn shifted_segments_after_replace(
    segments: &[OnionSegment],
    idx: usize,
    new_len: usize,
) -> Vec<OnionSegment> {
    let old_len = segments[idx].len;
    let delta = new_len as isize - old_len as isize;
    segments
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut s = *s;
            if i == idx {
                s.len = new_len;
            }
            if i > idx {
                s.offset = (s.offset as isize + delta) as usize;
            }
            s
        })
        .collect()
}

#[test]
fn shift_equal_len_keeps_offsets() {
    let after = shifted_segments_after_replace(&layout(), 1, 50);
    assert_eq!(after[1].len, 50);
    assert_eq!(after[2].offset, 150);
    assert_eq!(after[3].offset, 180);
}

#[test]
fn shift_grow_moves_following_tracks_forward() {
    // 段1 从 50 变 70（delta=+20）：后续段 offset 全部 +20
    let after = shifted_segments_after_replace(&layout(), 1, 70);
    assert_eq!(after[1].len, 70);
    assert_eq!(after[2].offset, 170);
    assert_eq!(after[3].offset, 200);
}

#[test]
fn shift_shrink_moves_following_tracks_backward() {
    // 段1 从 50 变 20（delta=-30）：后续段 offset 全部 -30
    let after = shifted_segments_after_replace(&layout(), 1, 20);
    assert_eq!(after[1].len, 20);
    assert_eq!(after[2].offset, 120);
    assert_eq!(after[3].offset, 150);
}

#[test]
fn shift_first_segment() {
    // 段0（首段）增长：delta = +40
    let after = shifted_segments_after_replace(&layout(), 0, 140);
    assert_eq!(after[0].len, 140);
    assert_eq!(after[1].offset, 140);
    assert_eq!(after[2].offset, 190);
    assert_eq!(after[3].offset, 220);
}

#[test]
fn shift_last_segment_no_followers() {
    // 段3（末段）缩短：无后续段可平移
    let after = shifted_segments_after_replace(&layout(), 3, 5);
    assert_eq!(after[3].len, 5);
    assert_eq!(after[3].offset, 180);
}

#[test]
fn shift_shrink_to_zero_len() {
    // 段变 0（整轨清空）：delta = -len
    let after = shifted_segments_after_replace(&layout(), 1, 0);
    assert_eq!(after[1].len, 0);
    assert_eq!(after[2].offset, 100);
    assert_eq!(after[3].offset, 130);
}

#[test]
fn layout_grow_appends_zero_len_segments() {
    // 4 段（末段 offset=180,len=20 → 末端 200）增长到 6 段：
    // 追加两个零长段（offset=200），无实例删除
    let (after, removed) = layout_segments_after_track_count(&layout(), 6);
    assert_eq!(after.len(), 6);
    assert_eq!(removed, None);
    assert_eq!(after[4], seg(4, 200, 0));
    assert_eq!(after[5], seg(5, 200, 0));
    // 既有段不受影响
    assert_eq!(&after[..4], &layout()[..]);
}

#[test]
fn layout_shrink_removes_tail_range() {
    // 4 段缩短到 2 段：删除区间 = 段2 起点 150、长度 30+20=50
    let (after, removed) = layout_segments_after_track_count(&layout(), 2);
    assert_eq!(after.len(), 2);
    assert_eq!(removed, Some((150, 50)));
    assert_eq!(&after[..2], &layout()[..2]);
}

#[test]
fn layout_shrink_to_zero_removes_whole_buffer() {
    let (after, removed) = layout_segments_after_track_count(&layout(), 0);
    assert!(after.is_empty());
    assert_eq!(removed, Some((0, 200)));
}

#[test]
fn layout_shrink_ignores_zero_len_tail() {
    // 尾部段全零长：无实例需删除
    let segments = vec![seg(0, 0, 10), seg(1, 10, 0), seg(2, 10, 0)];
    let (after, removed) = layout_segments_after_track_count(&segments, 1);
    assert_eq!(after.len(), 1);
    assert_eq!(removed, None);
}

#[test]
fn layout_same_count_is_noop() {
    let (after, removed) = layout_segments_after_track_count(&layout(), 4);
    assert_eq!(after, layout());
    assert_eq!(removed, None);
}
