//! 颜料桶填充：泛洪填充曲线围成的封闭区域为**实心**
//!
//! ## 算法
//! 1. 收集全部完整路径的格点离散化（`curve_cell_points`）作为**边界**；
//!    弯曲段相邻采样格点间 Bresenham 补连，保证边界 8 连通（无跳格缝隙，
//!    泛洪不会漏穿——等同矢量编辑器"描边连续"）；
//! 2. 从点击格点出发 BFS 4 邻域泛洪（网格索引运算，整数无精度问题）；
//! 3. 被边界包围的内部格点 → 存入曲线工具编辑层（`LineToolState.fill`），
//!    与路径一起 √ 确认时生成实心音符（**不直接写入音符**）。
//!
//! 路径未封闭时填充会蔓延到视图可见范围边界（与绘图软件"填充到画布
//! 边缘"行为一致），Ctrl+Z 可撤销。
//!
//! 内部格点用 `(i64 格索引, u16 key)` 表示（tick = 索引 × snap），
//! 避免浮点加减误差导致边界匹配失败。

use super::geom;
use crate::Editor;
use iced_core::Point;
use std::collections::{HashSet, VecDeque};

/// 泛洪填充纯函数：从 `start` 出发 4 邻域扩散，遇 `boundary` 格点停止；
/// 超出 `tick_idx_range` / `key_range`（闭区间）也停止。
///
/// 返回被填充的内部格点（不含边界格点，不含起点若起点在边界上）。
pub fn fill_cells(
    boundary: &HashSet<(i64, u16)>,
    start: (i64, u16),
    tick_idx_range: (i64, i64),
    key_range: (u16, u16),
) -> Vec<(i64, u16)> {
    if boundary.contains(&start) {
        return Vec::new();
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some((ti, k)) = queue.pop_front() {
        for (dti, dk) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nti = ti + dti;
            let nk = k as i64 + dk;
            if nti < tick_idx_range.0
                || nti > tick_idx_range.1
                || nk < key_range.0 as i64
                || nk > key_range.1 as i64
            {
                continue;
            }
            let next = (nti, nk as u16);
            if boundary.contains(&next) {
                continue;
            }
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    visited.into_iter().collect()
}

impl Editor {
    /// 颜料桶填充处理：点击封闭区域内部 → 泛洪计算内部格点，
    /// **存入曲线工具编辑层**（`line_tool.fill`），不直接生成音符。
    ///
    /// - 新填充：格点合并进 `fill`（去重），记录一次路径历史（Ctrl+Z 可撤销）；
    /// - 点击已填充格点：清除**全部**填充（再点一次取消，也记录历史）；
    /// - √ 确认时 `confirm_line_tool` 将路径格点 + 填充格点合并生成实心音符；
    /// - × 清空时一并丢弃。
    ///
    /// 边界 = 全部完整路径格点；范围 = 画布可见 tick 区间 + 全键盘 key。
    /// 填充模式保持开启（开关式，可连续填充多个区域）。
    ///
    /// `pub(crate)`：pressed.rs（interaction 父模块）在 Curve 工具 + 填充
    /// 模式下调用。
    pub(crate) fn handle_fill_pressed(&mut self, _pos: Point, snapped_tick: f32, key: u16) {
        let snap = self.editor_state.view.snap_precision.max(1.0);
        // 1. 边界格点（全部完整路径）
        //
        //    弯曲段 `curve_cell_points` 是采样取整：贝塞尔陡峭处相邻采样点
        //    可能一次跳多个 key，边界格点在 4 邻域意义下出现缝隙，泛洪会
        //    从缝隙漏穿（封闭图形填不上、背景反而被填）。与矢量编辑器
        //    "描边是连续像素"对齐：相邻采样格点间用 Bresenham 补连，
        //    补连后边界 8 连通，4 邻域泛洪无法穿过。
        let mut boundary: HashSet<(i64, u16)> = HashSet::new();
        for path in &self.editor_state.line_tool.paths {
            if path.len() < 2 {
                continue;
            }
            let mut prev: Option<(f32, u16)> = None;
            for pair in path.windows(2) {
                for (tick, k) in geom::curve_cell_points(pair[0], pair[1], snap) {
                    if let Some((pt, pk)) = prev {
                        for (t, kk) in
                            geom::line_cell_points((pt, pk as f32), (tick, k as f32), snap)
                        {
                            boundary.insert(((t / snap).round() as i64, kk));
                        }
                    }
                    boundary.insert(((tick / snap).round() as i64, k));
                    prev = Some((tick, k));
                }
            }
        }
        // 2. 可见范围（tick 方向 = 画布可见区间；key 方向 = 全键盘）
        let tick_lo = self.x_to_tick(0.0).max(0.0);
        let tick_hi = self
            .x_to_tick(self.editor_state.canvas.size_x)
            .max(tick_lo + snap);
        let key_count = self.editor_state.view.key_count;
        let start = ((snapped_tick / snap).round() as i64, key);

        // 3. 泛洪填充 → 逻辑坐标格点
        let cells = fill_cells(
            &boundary,
            start,
            (
                (tick_lo / snap).floor() as i64,
                (tick_hi / snap).ceil() as i64,
            ),
            (0, key_count.saturating_sub(1)),
        );
        let cells: Vec<(f32, u16)> = cells
            .into_iter()
            .map(|(ti, k)| (ti as f32 * snap, k))
            .collect();
        if cells.is_empty() {
            tracing::debug!("颜料桶: 点击位置在边界上或无可用格点，未填充");
            return;
        }

        // 4. 点击已填充格点 → 取消全部填充；否则合并新格点。均记录历史。
        let line = &mut self.editor_state.line_tool;
        let click_on_fill = line.fill.contains(&(snapped_tick, key));
        let changed = if click_on_fill {
            line.clear_fill()
        } else {
            line.add_fill_cells(&cells) > 0
        };
        if !changed {
            return;
        }
        line.push_path_history();
        line.last_push_path = None;
        tracing::info!(
            "颜料桶: {} 个格点（累计 {}），累计填充 {} 格",
            cells.len(),
            line.fill.len(),
            line.fill.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::fill_cells;
    use std::collections::HashSet;

    /// 构造矩形边界轮廓（格索引坐标）：x∈[2,4]、y∈[2,5] 的四条边
    fn rect_boundary() -> HashSet<(i64, u16)> {
        let mut b = HashSet::new();
        for x in 2..=4 {
            b.insert((x, 2));
            b.insert((x, 5));
        }
        for y in 2..=5 {
            b.insert((2, y));
            b.insert((4, y));
        }
        b
    }

    #[test]
    fn test_fill_inside_rect() {
        let b = rect_boundary();
        let mut cells = fill_cells(&b, (3, 3), (0, 100), (0, 20));
        cells.sort();
        assert_eq!(cells, vec![(3, 3), (3, 4)], "矩形内部 2 格");
    }

    #[test]
    fn test_fill_larger_enclosed_region() {
        // 10x8 的矩形边界（x: 10..=19, y: 5..=12）→ 内部 8x6 = 48 格
        let mut b = HashSet::new();
        for x in 10..=19 {
            b.insert((x, 5));
            b.insert((x, 12));
        }
        for y in 5..=12 {
            b.insert((10, y));
            b.insert((19, y));
        }
        let cells = fill_cells(&b, (15, 8), (0, 1000), (0, 100));
        assert_eq!(cells.len(), 8 * 6, "内部格点数 = (宽-2)*(高-2)");
    }

    #[test]
    fn test_fill_start_on_boundary_returns_empty() {
        let b = rect_boundary();
        assert!(
            fill_cells(&b, (2, 2), (0, 100), (0, 20)).is_empty(),
            "边界格点不可填充"
        );
    }

    #[test]
    fn test_fill_outside_rect_spreads_to_bounds() {
        let b = rect_boundary();
        // 起点 (0,0) 在矩形外 → 蔓延到整个范围，但矩形内部 2 格被边界隔离无法进入
        let cells = fill_cells(&b, (0, 0), (-5, 5), (0, 5));
        // 66 格 - 边界 10 格 - 内部 2 格 = 54
        assert_eq!(cells.len(), 54, "外部连通区 = 全部 - 边界 - 被隔离内部");
        assert!(!cells.contains(&(3, 3)), "矩形内部格点不可达");
        assert!(!cells.contains(&(3, 4)), "矩形内部格点不可达");
    }

    #[test]
    fn test_fill_honors_tick_bounds() {
        let b = rect_boundary();
        // 起点 (3,3) 内部格点；tick 范围 [3,3]、key 范围 [3,3] →
        // 邻居 (3,4) 超 key 界被裁剪，只填起点
        let cells = fill_cells(&b, (3, 3), (3, 3), (3, 3));
        assert_eq!(cells, vec![(3, 3)], "超界邻居被裁剪");
    }
}
