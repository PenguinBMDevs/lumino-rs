use super::compute_move_blocks;

/// 用标准库 memmove 语义（Vec::copy_within）对照验证搬移序列的最终结果
fn assert_move_correct(src: usize, dst: usize, count: usize, buf_len: usize) {
    let mut buf: Vec<u64> = (0..buf_len as u64).collect();
    let mut reference = buf.clone();
    reference.copy_within(src..src + count, dst);

    let blocks = compute_move_blocks(src, dst, count, 7); // 小块强制多分块
    for (s, d, n) in blocks {
        buf.copy_within(s..s + n, d);
    }
    assert_eq!(buf, reference, "src={src} dst={dst} count={count}");
}

#[test]
fn move_blocks_noop_when_zero_or_same() {
    assert!(compute_move_blocks(10, 20, 0, 8).is_empty());
    assert!(compute_move_blocks(10, 10, 5, 8).is_empty());
}

#[test]
fn move_blocks_forward_non_overlapping() {
    // 后移且不重叠：目标区完全在源区之后
    assert_move_correct(0, 50, 10, 70);
}

#[test]
fn move_blocks_backward_non_overlapping() {
    // 前移且不重叠：目标区完全在源区之前
    assert_move_correct(50, 0, 10, 70);
}

#[test]
fn move_blocks_forward_overlapping() {
    // 后移且重叠（经典 memmove 场景）：源 [0,10) → 目标 [5,15)
    assert_move_correct(0, 5, 10, 20);
}

#[test]
fn move_blocks_backward_overlapping() {
    // 前移且重叠：源 [5,15) → 目标 [0,10)
    assert_move_correct(5, 0, 10, 20);
}

#[test]
fn move_blocks_large_shift_small_block() {
    // 大幅移动 + 小块上限（多分块）
    assert_move_correct(100, 3, 40, 200);
    assert_move_correct(3, 100, 40, 200);
}

#[test]
fn move_blocks_adjacent_no_gap() {
    // 相邻不重叠边界：源 [0,10) → 目标 [10,20)
    assert_move_correct(0, 10, 10, 20);
    // 反向：源 [10,20) → 目标 [0,10)
    assert_move_correct(10, 0, 10, 20);
}

#[test]
fn move_blocks_single_instance() {
    assert_move_correct(3, 9, 1, 12);
    assert_move_correct(9, 3, 1, 12);
}

#[test]
fn move_blocks_partial_last_block() {
    // count 不是 max_block 的整数倍 → 最后一块是部分块
    let blocks = compute_move_blocks(0, 10, 15, 4);
    let total: usize = blocks.iter().map(|(_, _, n)| n).sum();
    assert_eq!(total, 15);
    // 每块 ≤ max_block
    assert!(blocks.iter().all(|(_, _, n)| *n <= 4));
    assert_move_correct(0, 10, 15, 30);
}
