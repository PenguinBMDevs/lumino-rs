//! Canvas 命中测试与布局计算。

use crate::sidebar::event_browser::canvas::{
    DIVIDER_HIT_WIDTH, HEADER_HEIGHT, MIN_COL_WIDTH, ROW_HEIGHT,
};

/// 计算可见行范围（0-based，前闭后开）。
///
/// `scroll_y` 为内容滚动偏移，`viewport_height` 为可视高度。
/// 结果基于行高，保守多取一行以覆盖顶部/底部被截断的行。
#[allow(dead_code)] // 当前由测试覆盖，后续虚拟滚动优化时接入
pub fn visible_range(scroll_y: f32, viewport_height: f32, total: usize) -> (usize, usize) {
    if total == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }
    let first = (scroll_y / ROW_HEIGHT).floor().max(0.0) as usize;
    let visible_count = (viewport_height / ROW_HEIGHT).ceil() as usize + 1;
    let last = (first + visible_count).min(total);
    (first, last)
}

/// 根据列宽计算各分割线的 x 坐标。
pub fn divider_positions(column_widths: &[f32]) -> Vec<f32> {
    let mut xs = Vec::with_capacity(column_widths.len().saturating_sub(1));
    let mut x = 0.0;
    for &w in column_widths
        .iter()
        .take(column_widths.len().saturating_sub(1))
    {
        x += w;
        xs.push(x);
    }
    xs
}

/// 判断鼠标是否落在某条分割线上。
pub fn hit_divider(x: f32, column_widths: &[f32]) -> Option<usize> {
    let xs = divider_positions(column_widths);
    xs.iter()
        .position(|&div_x| (x - div_x).abs() <= DIVIDER_HIT_WIDTH)
}

/// 应用列宽拖拽增量。
///
/// `start_widths` 为拖拽开始时的列宽快照，`idx` 为正在拖拽的列索引。
#[allow(dead_code)] // 当前由测试覆盖，后续拖拽优化时接入
pub fn apply_divider_drag(idx: usize, delta: f32, start_widths: &[f32]) -> f32 {
    let base = start_widths.get(idx).copied().unwrap_or(MIN_COL_WIDTH);
    (base + delta).max(MIN_COL_WIDTH)
}

/// 命中测试树行，返回树项索引。
pub fn hit_test_tree(y: f32, scroll_y: f32) -> Option<usize> {
    if y < 0.0 {
        return None;
    }
    let idx = ((scroll_y + y) / ROW_HEIGHT).floor() as usize;
    Some(idx)
}

/// 命中测试表格行（不含表头），返回当前页内的行索引。
pub fn hit_test_row(y: f32, scroll_y: f32) -> Option<usize> {
    let row_y = y - HEADER_HEIGHT;
    if row_y < 0.0 {
        return None;
    }
    let idx = ((scroll_y + row_y) / ROW_HEIGHT).floor() as usize;
    Some(idx)
}

/// 命中测试表格单元格，返回列索引。
pub fn hit_test_cell(x: f32, column_widths: &[f32]) -> Option<usize> {
    if x < 0.0 {
        return None;
    }
    let mut acc = 0.0;
    for (i, &w) in column_widths.iter().enumerate() {
        acc += w;
        if x < acc {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_empty() {
        assert_eq!(visible_range(0.0, 100.0, 0), (0, 0));
    }

    #[test]
    fn visible_range_first_page() {
        // 100px 视口 ≈ 5.5 行，保守取 7 行
        let (first, last) = visible_range(0.0, 100.0, 20);
        assert_eq!(first, 0);
        assert_eq!(last, 7);
    }

    #[test]
    fn visible_range_scrolled() {
        // 滚动 40px = 2 行
        let (first, last) = visible_range(40.0, 36.0, 20);
        assert_eq!(first, 2);
        assert!(last <= 20);
    }

    #[test]
    fn hit_test_tree_basic() {
        assert_eq!(hit_test_tree(0.0, 0.0), Some(0));
        assert_eq!(hit_test_tree(ROW_HEIGHT * 2.5, 0.0), Some(2));
        assert_eq!(hit_test_tree(0.0, ROW_HEIGHT * 3.0), Some(3));
    }

    #[test]
    fn hit_test_row_skips_header() {
        assert_eq!(hit_test_row(HEADER_HEIGHT - 1.0, 0.0), None);
        assert_eq!(hit_test_row(HEADER_HEIGHT, 0.0), Some(0));
        assert_eq!(hit_test_row(HEADER_HEIGHT + ROW_HEIGHT * 1.5, 0.0), Some(1));
        assert_eq!(hit_test_row(HEADER_HEIGHT, ROW_HEIGHT * 2.0), Some(2));
    }

    #[test]
    fn hit_test_cell_basic() {
        let widths = vec![30.0, 50.0, 70.0];
        assert_eq!(hit_test_cell(10.0, &widths), Some(0));
        assert_eq!(hit_test_cell(30.0, &widths), Some(1));
        assert_eq!(hit_test_cell(80.0, &widths), Some(2));
        assert_eq!(hit_test_cell(160.0, &widths), None);
    }

    #[test]
    fn divider_positions_sum() {
        let widths = vec![30.0, 50.0, 70.0];
        assert_eq!(divider_positions(&widths), vec![30.0, 80.0]);
    }

    #[test]
    fn apply_divider_drag_clamps_min() {
        let widths = vec![30.0, 50.0];
        assert_eq!(apply_divider_drag(0, -100.0, &widths), MIN_COL_WIDTH);
        assert_eq!(apply_divider_drag(0, 20.0, &widths), 50.0);
    }

    #[test]
    fn hit_divider_near_split() {
        let widths = vec![30.0, 50.0];
        assert!(hit_divider(30.0, &widths).is_some());
        assert!(hit_divider(35.0, &widths).is_none());
    }
}
