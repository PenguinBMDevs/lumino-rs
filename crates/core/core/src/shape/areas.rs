//! 形状区域着色点（DRAW-032 的数据底座）
//!
//! 本模块承载**类型、1024 网格求解与洪泛着色**；音符生成接线在
//! `lumino-editor-state::shape_ops::areas`。`AreaSpot` 已随 `.lmshapes` v2
//! 的 `ShapeStyle.areas` 落盘（DRAW-040），本卡不提升部件版本。
//!
//! 区域求解口径（对齐 Spiderweb `notes/areas.py`）：
//! - 形状自身 box 归一化为 `0..=1`，在 `MAP_CELLS × MAP_CELLS` 网格上求解；
//! - `AreaSpot` 的 `(u, v)` 是区域内的一个点，`colour = 0` 表示该区域不产音符；
//! - 比一格还细的碎片可能取邻居色——这是 1024 网格的已知取舍，不在这里“修复”。

use std::fmt;

use serde::{Deserialize, Serialize};

use super::AREA_COLOUR_MAX;
use crate::error::{CoreError, Result};

/// 区域求解网格边长（1024 × 1024）
pub const MAP_CELLS: u16 = 1024;

/// 未启用通道 10 时的最大色号
pub const COLOURS: u8 = 15;

/// 启用通道 10 时的最大色号
pub const ALL_COLOURS: u8 = 16;

/// 网格中“尚未着色”的哨兵值（不是合法色号）
pub const AREA_UNASSIGNED: u8 = u8::MAX;

/// 网格中“形状外部”的哨兵值
pub const AREA_OUTSIDE: u8 = u8::MAX - 1;

/// 网格中“区域边界”的哨兵值
pub const AREA_BOUNDARY: u8 = u8::MAX - 2;

/// 区域色在中间音符合成阶段的锁定基址。
///
/// `ShapeNote.channel` 只有 0..=15 是合法 MIDI 通道；区域着色先写成
/// `AREA_LOCK_BASE + colour`，让 `Cycle` 等自动通道算子跳过，生成结束后再解锁。
pub const AREA_LOCK_BASE: u8 = 0x80;

/// 当前工程可用的最大色号
pub fn most_colours(use10: bool) -> u8 {
    if use10 { ALL_COLOURS } else { COLOURS }
}

/// 形状内部的一个着色点
///
/// 对应 Spiderweb `notes/areas.py` 的 `sh["areas"] = [[u, v, colour]]`：
/// `(u, v)` 是形状自身 box 内的**归一化坐标**（`0..=1`），`colour`：
/// - `0` = 空（不填色）
/// - `1..=15` = 颜色槽
/// - `16` = 通道 10（仅在工程启用通道 10 时可用；本类型不判断工程状态）
///
/// `u` / `v` 随形状移动而移动，因此存**比例**而非绝对 tick/key——
/// 形状平移后着色点必须跟着走（与 DRAW-007 的框选坐标同一口径）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AreaSpot {
    /// box 内归一化横坐标（0..=1）
    pub u: f32,
    /// box 内归一化纵坐标（0..=1）
    pub v: f32,
    /// 色号（0..=16；16 需工程启用通道 10）
    pub colour: u8,
}

impl AreaSpot {
    /// 校验单个着色点：坐标有限且落在 box 内、色号不越界
    ///
    /// 返回 `Err` 而不是 panic：手改/损坏的工程文件必须能被拒绝并给出可读原因。
    pub fn validate(&self) -> Result<()> {
        if !self.u.is_finite() || !self.v.is_finite() {
            return Err(CoreError::InvalidArgument(
                "区域着色点坐标含非有限值".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.u) || !(0.0..=1.0).contains(&self.v) {
            return Err(CoreError::InvalidArgument(format!(
                "区域着色点坐标越界（允许 0..=1，实际 u={}, v={}）",
                self.u, self.v
            )));
        }
        if self.colour > AREA_COLOUR_MAX {
            return Err(CoreError::InvalidArgument(format!(
                "区域色号越界（允许 0..={}，实际 {}）",
                AREA_COLOUR_MAX, self.colour
            )));
        }
        Ok(())
    }
}

/// 1024 × 1024 的区域网格
///
/// `cells` 中：
/// - `0..=16`：该区域的颜色（0 = 空）；
/// - [`AREA_UNASSIGNED`]：形状内部、尚未着色；
/// - [`AREA_OUTSIDE`]：形状外部；
/// - [`AREA_BOUNDARY`]：轮廓边界（洪泛不可穿越）。
#[derive(Clone, PartialEq)]
pub struct AreaMap {
    /// 行优先网格（`row * MAP_CELLS + col`）
    pub cells: Vec<u8>,
}

impl fmt::Debug for AreaMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let assigned = self
            .cells
            .iter()
            .filter(|cell| **cell <= ALL_COLOURS)
            .count();
        f.debug_struct("AreaMap")
            .field("cells", &self.cells.len())
            .field("assigned", &assigned)
            .finish()
    }
}

impl Default for AreaMap {
    fn default() -> Self {
        Self::new()
    }
}

impl AreaMap {
    /// 空网格：全部视为形状外部（无法着色）
    pub fn new() -> Self {
        Self {
            cells: vec![AREA_OUTSIDE; usize::from(MAP_CELLS) * usize::from(MAP_CELLS)],
        }
    }

    /// 由归一化闭合路径求解内部网格
    ///
    /// `paths` 中每个点都在 `0..=1`；每个路径自动闭合。`union = true` 时用
    /// 非零环绕数，否则用奇偶规则（与 ShapeStyle 的 `union` 口径一致）。
    pub fn rasterize(paths: &[Vec<(f32, f32)>], union: bool) -> Self {
        let n = usize::from(MAP_CELLS);
        let mut map = Self::new();
        for row in 0..n {
            let y = (row as f32 + 0.5) / n as f32;
            let mut crossings: Vec<(f32, i32)> = Vec::new();
            for path in paths {
                if path.len() < 2 {
                    continue;
                }
                for index in 0..path.len() {
                    let a = path[index];
                    let b = path[(index + 1) % path.len()];
                    if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
                        let dy = b.1 - a.1;
                        if dy.abs() > f32::EPSILON {
                            let x = a.0 + (y - a.1) * (b.0 - a.0) / dy;
                            let direction = if b.1 > a.1 { 1 } else { -1 };
                            crossings.push((x, direction));
                        }
                    }
                }
            }
            if crossings.is_empty() {
                continue;
            }
            crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let mut spans: Vec<(f32, f32)> = Vec::new();
            if union {
                let mut winding = 0;
                let mut start = 0.0;
                for (x, direction) in crossings {
                    let before = winding;
                    winding += direction;
                    if before == 0 && winding != 0 {
                        start = x;
                    } else if before != 0 && winding == 0 {
                        spans.push((start, x));
                    }
                }
            } else {
                let mut index = 0;
                while index + 1 < crossings.len() {
                    spans.push((crossings[index].0, crossings[index + 1].0));
                    index += 2;
                }
            }

            for (x0, x1) in spans {
                if !x0.is_finite() || !x1.is_finite() || x1 <= x0 {
                    continue;
                }
                let first = ((x0 * n as f32) - 0.5).ceil().max(0.0) as usize;
                let last = ((x1 * n as f32) - 0.5).floor().min((n - 1) as f32) as usize;
                if last < first {
                    continue;
                }
                for col in first..=last.min(n - 1) {
                    map.cells[row * n + col] = AREA_UNASSIGNED;
                }
            }
        }

        // 轮廓最后写入，保证洪泛不会穿越边界。
        for path in paths {
            if path.len() < 2 {
                continue;
            }
            for index in 0..path.len() {
                let a = path[index];
                let b = path[(index + 1) % path.len()];
                raster_line(&mut map.cells, n, a, b);
            }
        }
        map
    }

    /// 取归一化坐标处的网格值
    ///
    /// 越界或非有限坐标返回 [`AREA_OUTSIDE`]；返回 `AREA_UNASSIGNED` 表示
    /// 形状内部但尚未着色。
    pub fn area_at(&self, u: f32, v: f32) -> u8 {
        let Some((row, col)) = self.cell_of(u, v) else {
            return AREA_OUTSIDE;
        };
        let n = usize::from(MAP_CELLS);
        self.cells[row * n + col]
    }

    /// 从归一化点开始洪泛着色
    ///
    /// 只填充**同一旧值**的连通区域；边界/外部不填。`colour = 0` 表示把该区域
    /// 标为空。返回是否真的改变了网格。
    pub fn paint_area(&mut self, u: f32, v: f32, colour: u8) -> bool {
        if colour > ALL_COLOURS {
            return false;
        }
        let Some((row, col)) = self.cell_of(u, v) else {
            return false;
        };
        let n = usize::from(MAP_CELLS);
        let start = row * n + col;
        let old = self.cells[start];
        if old == colour || old == AREA_OUTSIDE || old == AREA_BOUNDARY {
            return false;
        }

        let mut stack = vec![start];
        let mut changed = false;
        while let Some(index) = stack.pop() {
            if self.cells[index] != old {
                continue;
            }
            self.cells[index] = colour;
            changed = true;
            let row = index / n;
            let col = index % n;
            if col > 0 {
                stack.push(index - 1);
            }
            if col + 1 < n {
                stack.push(index + 1);
            }
            if row > 0 {
                stack.push(index - n);
            }
            if row + 1 < n {
                stack.push(index + n);
            }
        }
        changed
    }

    /// 归一化坐标 → 网格行列；越界/非有限返回 `None`
    fn cell_of(&self, u: f32, v: f32) -> Option<(usize, usize)> {
        if !u.is_finite()
            || !v.is_finite()
            || !(0.0..=1.0).contains(&u)
            || !(0.0..=1.0).contains(&v)
        {
            return None;
        }
        let n = usize::from(MAP_CELLS);
        let col = ((u * n as f32) as usize).min(n - 1);
        let row = ((v * n as f32) as usize).min(n - 1);
        Some((row, col))
    }
}

/// Bresenham 画一条边界线（归一化坐标）
fn raster_line(cells: &mut [u8], n: usize, a: (f32, f32), b: (f32, f32)) {
    let mut x0 = (a.0 * n as f32).round().clamp(0.0, (n - 1) as f32) as i32;
    let mut y0 = (a.1 * n as f32).round().clamp(0.0, (n - 1) as f32) as i32;
    let x1 = (b.0 * n as f32).round().clamp(0.0, (n - 1) as f32) as i32;
    let y1 = (b.1 * n as f32).round().clamp(0.0, (n - 1) as f32) as i32;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 {
            let index = y0 as usize * n + x0 as usize;
            if let Some(cell) = cells.get_mut(index) {
                *cell = AREA_BOUNDARY;
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

#[cfg(test)]
mod tests;
