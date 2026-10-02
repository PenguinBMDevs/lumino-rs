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

/// 覆盖格窗口（**格坐标**，闭区间）——预览只栅格化窗口内的格
///
/// 语义分工（§17 长笔画掉帧修复）：
/// - **预览**（每帧热路径）用 [`cover_cells_in_window`]：成本 O(段数 + 窗口内格数)，
///   与笔画总长度**无关**——旧实现每帧对整笔做全量栅格化，长笔画时 96%+ 的算力
///   花在视口外（实测 2 万点笔画：每帧 35.7ms、单帧分配 70MB，必然掉帧）；
/// - **生成**（√ 一次性）用 [`cover_cells`]：全量 + 去重，成本 O(总格数)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellWindow {
    /// 可见最小 tick 格索引（含）
    pub cell_lo: i64,
    /// 可见最大 tick 格索引（含）
    pub cell_hi: i64,
    /// 可见最小 key（含）
    pub key_lo: u16,
    /// 可见最大 key（含）
    pub key_hi: u16,
}

impl CellWindow {
    /// 全量窗口：不裁任何格（用于生成、测试与"窗口化 == 全量"等价性验证）
    pub const FULL: Self = Self {
        cell_lo: i64::MIN,
        cell_hi: i64::MAX,
        key_lo: 0,
        key_hi: MAX_KEY,
    };

    /// 格是否在窗口内
    pub fn contains(&self, cell: i64, key: u16) -> bool {
        cell >= self.cell_lo && cell <= self.cell_hi && key >= self.key_lo && key <= self.key_hi
    }

    /// 线段两端点是否都在窗口内（是则走原栅格化，逐格与全量路径完全一致）
    fn contains_segment(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
        let (x_lo, x_hi) = (self.cell_lo as f64, self.cell_hi as f64);
        let (y_lo, y_hi) = (self.key_lo as f64, self.key_hi as f64);
        x0 >= x_lo
            && x0 <= x_hi
            && x1 >= x_lo
            && x1 <= x_hi
            && y0 >= y_lo
            && y0 <= y_hi
            && y1 >= y_lo
            && y1 <= y_hi
    }
}

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
    segment_cells_cell_space(
        a.0 as f64 / snap,
        a.1 as f64,
        b.0 as f64 / snap,
        b.1 as f64,
        out,
    );
}

/// 线段栅格化核心（**格坐标** f64）：`x` = tick 格坐标（可小数）、`y` = key
///
/// 拆出 f64 版本是为了让窗口化路径**全程停留在 f64**：裁剪后的端点若先落回 f32
/// 再换算，几何会偏移 ~1e-7 相对量，恰好穿过格点的角点判定就会与全量路径不一致
/// （实测丢 1 格对角线，见 §17 等价性测试）。
fn segment_cells_cell_space(x0: f64, y0: f64, x1: f64, y1: f64, out: &mut Vec<CoveredCell>) {
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
    /// 角点判定的**相对**容差（§17）：同一几何在"裁剪后"是原线段的参数缩放，
    /// 绝对 EPS 会让"恰好穿过格点"的判定在两条路径上不一致——实测表现为
    /// 窗口化预览比全量覆盖少 1 格对角线（角点格），破坏"所见即生成"。
    /// 相对容差对参数缩放不敏感（两侧同比例缩放 → 判定不变），
    /// 1e-9 远高于浮点噪声（1e-16~1e-13）、远低于任何可见几何差异。
    const EPS_REL: f64 = 1e-9;

    for _ in 0..steps {
        if i == i_end && j == j_end {
            break;
        }
        // 容差只由**有限**的 t 值决定：平行于某轴时该轴 t = INFINITY，
        // 若把 ∞ 纳入 max 会得到 ∞ 容差 → 每步都被判成"角点"（水平/垂直笔画全错）
        let finite_x = if t_max_x.is_finite() {
            t_max_x.abs()
        } else {
            0.0
        };
        let finite_y = if t_max_y.is_finite() {
            t_max_y.abs()
        } else {
            0.0
        };
        let tol = finite_x.max(finite_y).max(1.0) * EPS_REL;
        if t_max_x < t_max_y - tol {
            i += step_i;
            t_max_x += t_delta_x;
            push_cell(out, i, j);
        } else if t_max_y < t_max_x - tol {
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

/// 线段 → **窗口内**覆盖格，追加到 `out`（保序；不做全局去重）
///
/// - 两端都在窗口内：直接走 [`segment_cells`]，逐格与全量路径完全一致；
/// - 否则用 Liang–Barsky 把线段裁到窗口矩形，只对裁出的那一段做栅格化——
///   视口外的部分**一次比较就跳过**，这是长笔画每帧成本从 O(总格数) 降到
///   O(段数 + 窗口内格数) 的关键。
///
/// 语义保证：输出 ⊆ 全量覆盖格，且窗口内可见的格**一个不少**
/// （进入点所在格的 `floor` 落在窗口边上，仍属窗口内 → 边缘行段不会被截断丢失）。
pub fn segment_cells_in_window(
    a: (f32, f32),
    b: (f32, f32),
    snap: f32,
    window: CellWindow,
    out: &mut Vec<CoveredCell>,
) {
    let snap = normalize_snap(snap);
    let x0 = a.0 as f64 / snap;
    let y0 = a.1 as f64;
    let x1 = b.0 as f64 / snap;
    let y1 = b.1 as f64;

    if window.contains_segment(x0, y0, x1, y1) {
        segment_cells(a, b, snap as f32, out);
        return;
    }
    let start_len = out.len();
    let Some((cx0, cy0, cx1, cy1)) = clip_to_window(x0, y0, x1, y1, window) else {
        return; // 整段在窗口外：成本 O(1)
    };
    // 直接进 f64 核心：不经过 f32，避免裁剪端点精度损失破坏角点判定
    segment_cells_cell_space(cx0, cy0, cx1, cy1, out);
    // 裁剪边界上的"半格"清理：`floor(cell_hi + 1)` 会落在窗口外一格，
    // 以及浮点误差可能让进入点落到 cell_lo - 1 —— 原地过滤后，
    // 输出严格满足"窗口内一格不少、窗口外一格不多"（等价性测试锁死该不变式）。
    if out.len() > start_len {
        let mut kept = start_len;
        for i in start_len..out.len() {
            let cell = out[i];
            if window.contains(cell.0, cell.1) {
                out[kept] = cell;
                kept += 1;
            }
        }
        out.truncate(kept);
    }
}

/// 折线 → **窗口内**覆盖格（保序；相邻段共享的边界格会重复出现，由调用方容忍）
///
/// 成本 O(段数 + 窗口内格数)。窗口外的线段只付一次裁剪判定。
/// 语义：与 [`cover_cells`] 相比**只是缺了视口外的格与去重**——
/// 预览用（同色重复覆盖视觉无差别），生成仍走 [`cover_cells`] 保证精确。
pub fn cover_cells_in_window(
    points: &[(f32, f32)],
    snap: f32,
    window: CellWindow,
    out: &mut Vec<CoveredCell>,
) {
    let snap = normalize_snap(snap);
    match points {
        [] => {}
        [only] => {
            let cell = point_cell(*only, snap as f32);
            if window.contains(cell.0, cell.1) {
                out.push(cell);
            }
        }
        _ => {
            for pair in points.windows(2) {
                segment_cells_in_window(pair[0], pair[1], snap as f32, window, out);
            }
        }
    }
}

/// Liang–Barsky 线段裁剪：把 `(x0,y0)-(x1,y1)` 裁到窗口矩形，返回裁剪后的端点
///
/// 完全在窗口外返回 `None`。全部用 f64（格坐标可到 i64 边界，浮点比较更安全）。
fn clip_to_window(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    window: CellWindow,
) -> Option<(f64, f64, f64, f64)> {
    let dx = x1 - x0;
    let dy = y1 - y0;
    // 窗口是**闭区间格索引**：格 `cell_hi` 在连续坐标上占 `[cell_hi, cell_hi + 1)`，
    // 故裁剪矩形上界要 +1（只按 cell_hi 裁会把最后一格的右半格切掉 → 丢格）
    let (x_min, x_max) = (window.cell_lo as f64, window.cell_hi as f64 + 1.0);
    let (y_min, y_max) = (window.key_lo as f64, window.key_hi as f64 + 1.0);
    let mut t0 = 0.0f64;
    let mut t1 = 1.0f64;
    for (p, q) in [
        (-dx, x0 - x_min),
        (dx, x_max - x0),
        (-dy, y0 - y_min),
        (dy, y_max - y0),
    ] {
        if p == 0.0 {
            // 平行于该边界：在外侧则整段不可见
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            if r > t0 {
                t0 = r;
            }
        } else {
            if r < t0 {
                return None;
            }
            if r < t1 {
                t1 = r;
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    // ⚠️ 两个端点都必须由**原始**端点 + t 求值：先改起点再算终点会把终点推远
    // （曾把窗口 [50,60] 的段裁成 50..150 → 越窗吐格 + 丢格，被等价性测试抓住）
    Some((x0 + t0 * dx, y0 + t0 * dy, x0 + t1 * dx, y0 + t1 * dy))
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
