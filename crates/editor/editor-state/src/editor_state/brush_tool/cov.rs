//! 画刷笔画覆盖计算 —— 线段栅格化（断墨修复的核心）
//!
//! **根因**：旧实现「每进入一个网格单元盖一列音符」，以**采样点**为单位落笔，
//! 采样频率一降（快速拖动一帧跨多格）中间格全部丢失 → 预览/生成的音符出现空洞。
//!
//! **修复**：把落笔语义从「采样点集合」改为「笔画几何的覆盖集」——
//! 相邻采样点之间做线段栅格化，输出线段**经过的全部网格单元**：
//!
//! - 覆盖成本只与**覆盖格数**相关（O(|Δ格|)），与采样点数/采样频率无关：
//!   拖得再快也不会丢格，拖得再慢也不会重复计算；
//! - 采用「超覆盖（supercover）」步进：线段恰好穿过网格角点时把两侧相邻格
//!   一并计入，保证覆盖集**4 连通**（相邻格共边而非只共角）→ 无洞；
//! - 预览与音符生成共用本函数（预览层只做屏幕空间抽稀，见 UI 层），
//!   杜绝「预览连续、生成有洞」这类口径分裂。

use std::collections::HashSet;

/// 覆盖格：`(tick 格索引, key)`
///
/// tick 格索引 = `floor(tick / snap)`（可为负，交由上层钳制）；
/// key 为整数半音。
pub type CoveredCell = (i64, u16);

/// 音符 key 上限（与 `Note` / MIDI 约定一致）
pub const MAX_KEY: u16 = 255;

/// tick → tick 格索引（向下取整；snap 非法时退化为 1）
pub fn tick_cell(tick: f32, snap: f32) -> i64 {
    let snap = normalize_snap(snap);
    (tick as f64 / snap).floor() as i64
}

/// tick 格索引 → 格起始 tick（= 音符 start_tick）
pub fn cell_tick(cell: i64, snap: f32) -> f32 {
    let snap = normalize_snap(snap);
    (cell as f64 * snap) as f32
}

/// 单点 → 覆盖格
pub fn point_cell(point: (f32, f32), snap: f32) -> CoveredCell {
    let key = point.1.round().clamp(0.0, MAX_KEY as f32) as u16;
    (tick_cell(point.0, snap), key)
}

/// 线段栅格化：把 `a → b` **经过的全部网格单元**追加到 `out`（含端点，未去重）
///
/// 超覆盖步进（Amanatides–Woo）：
/// - 每次步进推进到下一个网格边界（t 较小的一轴）；
/// - 两轴同时到达边界（线段穿过格点）时，把 `(i+1, j)`、`(i, j+1)`、`(i+1, j+1)`
///   一并计入，保证 4 连通。
///
/// 复杂度 O(|Δi| + |Δj|)：迭代次数取两轴边界穿越次数之和（整数上界），
/// **不依赖浮点比较终止**——早期版本用 `t > 1.0` 做安全网，因 `1/150` 累加漂移
/// 偶发提前一格雷断（末格丢失），已改为整数步数上界 + 终点兜底。
pub fn segment_cells(a: (f32, f32), b: (f32, f32), snap: f32, out: &mut Vec<CoveredCell>) {
    let snap = normalize_snap(snap);
    // 连续格坐标：x 为 tick 格坐标（可为小数），y 为 key（整数端点上必为整数）
    let x0 = a.0 as f64 / snap;
    let y0 = a.1 as f64;
    let x1 = b.0 as f64 / snap;
    let y1 = b.1 as f64;

    let mut i = x0.floor() as i64;
    let mut j = y0.floor() as i64;
    let i_end = x1.floor() as i64;
    let j_end = y1.floor() as i64;

    push_cell(out, i, j);
    if i == i_end && j == j_end {
        return;
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    if dx == 0.0 && dy == 0.0 {
        return;
    }

    let step_i = sign_i(dx);
    let step_j = sign_i(dy);
    let t_delta_x = if dx != 0.0 {
        (1.0 / dx).abs()
    } else {
        f64::INFINITY
    };
    let t_delta_y = if dy != 0.0 {
        (1.0 / dy).abs()
    } else {
        f64::INFINITY
    };
    // 到下一个网格边界的归一化参数（t ∈ [0,1] 覆盖整条线段）
    let mut t_max_x = next_boundary_t(x0, i, dx);
    let mut t_max_y = next_boundary_t(y0, j, dy);

    // 步数 = 两轴网格边界穿越次数之和（整数上界，与浮点无关）：
    // 线段穿过格点时一次迭代推进两轴，因此实际迭代数 ≤ 该上界。
    let steps = ((i_end - i).unsigned_abs() + (j_end - j).unsigned_abs()) as usize;
    const EPS: f64 = 1e-12;

    for _ in 0..steps {
        if i == i_end && j == j_end {
            break;
        }
        if t_max_x < t_max_y - EPS {
            i += step_i;
            t_max_x += t_delta_x;
            push_cell(out, i, j);
        } else if t_max_y < t_max_x - EPS {
            j += step_j;
            t_max_y += t_delta_y;
            push_cell(out, i, j);
        } else {
            // 恰好穿过网格角点：两侧相邻格都经过（含对角格），保证无洞
            let ni = i + step_i;
            let nj = j + step_j;
            push_cell(out, ni, j);
            push_cell(out, i, nj);
            push_cell(out, ni, nj);
            i = ni;
            j = nj;
            t_max_x += t_delta_x;
            t_max_y += t_delta_y;
        }
    }
    // 兜底：浮点边界误差下若未精确落到终点格，仍保证终点被覆盖
    //（重复由 `cover_cells` 去重；绝不因浮点漂移漏掉笔画末格 → 断墨）
    push_cell(out, i_end, j_end);
}

/// 折线 → 去重后的覆盖格（保序：先出现的先保留）
///
/// 单点笔画返回该点所在格；重复点/重合段由去重吸收。
pub fn cover_cells(points: &[(f32, f32)], snap: f32) -> Vec<CoveredCell> {
    let snap = normalize_snap(snap);
    let mut raw = Vec::new();
    match points {
        [] => {}
        [only] => raw.push(point_cell(*only, snap as f32)),
        _ => {
            for pair in points.windows(2) {
                segment_cells(pair[0], pair[1], snap as f32, &mut raw);
            }
        }
    }
    let mut seen = HashSet::with_capacity(raw.len());
    raw.retain(|cell| seen.insert(*cell));
    raw
}

/// 覆盖格 × 粗细度 → `(格, 音符 key, 层级 level)`
///
/// 每格向上展开 `thickness` 层（`level` 从 0 起，key = 底键 + level，
/// `key > MAX_KEY` 截断）；`level` 供调用方解析该层分配的音轨
/// （见 `BrushConfig::track_for_level`）与层颜色。
pub fn expand_layers(cells: &[CoveredCell], thickness: u8, out: &mut Vec<(CoveredCell, u8)>) {
    if thickness == 0 {
        return;
    }
    for &(cell, key) in cells {
        for level in 0..thickness as u16 {
            let k = key.saturating_add(level);
            if k > MAX_KEY {
                break;
            }
            out.push(((cell, k), level as u8));
        }
    }
}

/// snap 归一化：非有限值或 ≤ 0 退化为 1（避免除零/NaN 传播）
fn normalize_snap(snap: f32) -> f64 {
    if snap.is_finite() && snap > 0.0 {
        snap as f64
    } else {
        1.0
    }
}

/// 浮点符号（0 → 0）
fn sign_i(v: f64) -> i64 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// 从连续坐标 `v`（所在格 `cell`）沿 `d` 方向到下一个网格边界的归一化参数
///
/// `d == 0` → `INFINITY`（该轴永不步进）。
fn next_boundary_t(v: f64, cell: i64, d: f64) -> f64 {
    if d > 0.0 {
        ((cell + 1) as f64 - v) / d
    } else if d < 0.0 {
        (cell as f64 - v) / d
    } else {
        f64::INFINITY
    }
}

/// 追加覆盖格（key 钳制到合法范围）
fn push_cell(out: &mut Vec<CoveredCell>, cell: i64, key: i64) {
    out.push((cell, key.clamp(0, MAX_KEY as i64) as u16));
}
