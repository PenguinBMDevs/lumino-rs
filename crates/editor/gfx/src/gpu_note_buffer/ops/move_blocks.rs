//! GPU 内部搬移分块序列计算（纯函数）

/// 计算搬移分块序列（纯函数，可单测）
///
/// 返回按执行顺序的 `(src, dst, n)` 列表：每块源/目标区间互不重叠
/// （staging 中转后安全）；方向序保证不覆盖未搬源块（后移从尾向前、前移从头向后）。
/// 正确性由测试用 `Vec::copy_within`（标准 memmove 语义）对照验证。
pub fn compute_move_blocks(
    src: usize,
    dst: usize,
    count: usize,
    max_block: usize,
) -> Vec<(usize, usize, usize)> {
    if count == 0 || src == dst || max_block == 0 {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    if dst > src {
        // 后移：从尾部向前（目标区在源区之后，先搬最后块不会覆盖未搬的源）
        let mut remaining = count;
        let mut s_end = src + count;
        let mut d_end = dst + count;
        while remaining > 0 {
            let n = remaining.min(max_block);
            s_end -= n;
            d_end -= n;
            blocks.push((s_end, d_end, n));
            remaining -= n;
        }
    } else {
        // 前移：从头部向后（目标区在源区之前，先搬最前块不会覆盖未搬的源）
        let mut s = src;
        let mut d = dst;
        let mut remaining = count;
        while remaining > 0 {
            let n = remaining.min(max_block);
            blocks.push((s, d, n));
            s += n;
            d += n;
            remaining -= n;
        }
    }
    blocks
}
