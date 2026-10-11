//! 全局绘制粗细的**加粗几何**：把描边中心线按线段法线膨胀成有宽度的「带」
//!
//! ## 口径（与画布上画出来的线严格同源）
//!
//! 线宽 = 粗细层数 × 单 key 高（`zoom_y`），沿**线段法线**单侧铺开，偏向
//! 「向上优先、其次向右」（与画刷笔画"从落笔 key 向上铺层"同偏向）。于是：
//! - **水平段** → 纵向加厚 N 个 key；
//! - **竖直段** → 横向加宽 `(N-1)·zoom_y/zoom_x` 个 tick（**本模块存在的理由**）；
//! - **斜段** → 沿其法线加厚（不是按"水平段"硬套）；
//! - **拐角** → 相邻两段各铺一条矩形，外角方格由「两段偏移向量张成的平行四边形」补上
//!   （见下文「拐角必须单独补」，不补就是用户报的"矩形右上角缺口"）。
//!
//! ## 为什么必须沿法线（BUG 根因）
//!
//! 早先的实现是「把 `path_notes` 出的中心线音符按 `key + level` 向上平移 N 层」。
//! 这对水平段恰好成立（向上 = 垂直于线），但**对竖直段等于把线拉长**：横向宽度
//! 原地不动，用户看到的就是"改了粗细度，矩形竖边还是很细一条，宽度不匹配"。
//! 平移不是加粗——加粗必须垂直于线段方向。
//!
//! ## 实现
//!
//! 每段折线构造一条「单侧矩形」（段两端各沿法线偏移 `(N-1)·zoom_y` 像素，矩形的
//! 长边 = 原线段、短边 = 加宽量），再用填充引擎同一套逐音高行区间解析
//! （[`super::fill::spans::row_spans`]；凸四边形下 even-odd 即内部）求出该段覆盖的
//! 每一行区间；全部线段的区间按行求并（[`spans::merge_spans`]）后与中心线音符合并。
//!
//! ## 拐角必须单独补：外角方格（BUG 根因之二）
//!
//! 只用「每段一条矩形」在拐角**外侧**会缺一块：矩形 A 止于角点的 tick、矩形 B 止于
//! 角点的 key，谁都不越界到「`tick > 角点 ∧ key > 角点`」那一格——用户看到的就是
//! 「矩形右上角有个缺口、音符没盖到角上」。画布预览走 `Stroke` 的**默认 Miter** 拼接
//! （`grid/shape_tool_box.rs` 的 `frame.stroke`），拐角是实心的，所以生成侧必须补上
//! 同一块外角方格才叫「所见即所得」：以角点为起点、两条相邻偏移向量张成的
//! **平行四边形**（垂直拐角时正好是缺掉的那个正方格；斜拐角时是其外切方格，比真 miter
//! 略大、比留缺口安全）。共线（同向）拐角的平行四边形退化为零面积，跳过——不跳会在
//! 直线上留下 1 tick 的碎音符。
//!
//! 用「每段一条矩形 + 拐角平行四边形 + 逐行求并」而不是「整条路径平移 N 次」的另一个
//! 好处：重叠部分由区间求并自然融合，不需要为拐角写第二套几何。
//!
//! ## 与卷帘方向无关
//!
//! `zoom_x` / `zoom_y` 的含义是「每 tick / 每 key 多少像素」，纵向卷帘只是让这两个
//! 方向占据不同的屏幕轴（见 `coords.rs` 的转置），像素度量本身不变——因此本模块
//! **不需要**为纵向卷帘写任何分支。

use std::collections::BTreeMap;

use super::fill::spans::{merge_spans, row_spans};
use super::paths::{self, RawNote};

/// 折线上的一个点（逻辑坐标：tick, key）——与 `paths` / `geom` 同形（f64）
pub(crate) type Pt = (f64, f64);

/// 一条描边折线 → 音符：**中心线 + 全局粗细的加粗带**（同一份几何，唯一入口）。
///
/// - `thickness <= 1`：只返回中心线音符（与加粗功能引入前逐位一致）；
/// - `zoom_x` / `zoom_y`：每 tick / 每 key 的像素数（纵向卷帘下含义不变）；
/// - `key_hi`：渲染音高行上限（越界行裁掉，与调用方既有过滤同口径）。
///
/// 返回值已按「同 tick 同 key 只留最长」收敛（[`paths::keep_longest`]）：中心线音符
/// 与加粗带在同一行/同一 tick 上相遇时保留更长的那条——加粗带因此自然吸收掉
/// 中心线的短音符，不会留下"带里嵌着细线"的双份音符。
pub(crate) fn outline_notes(
    poly: &[Pt],
    thickness: u8,
    zoom_x: f32,
    zoom_y: f32,
    key_hi: i32,
) -> Vec<RawNote> {
    let mut raw = paths::path_notes(poly, false);
    raw.extend(band_notes(poly, thickness, zoom_x, zoom_y, key_hi));
    paths::keep_longest(&raw)
}

/// 一条描边折线 → **将生成的音符块**（逻辑坐标 `(起 tick, 止 tick, key)`）。
///
/// **单一入口**：`√` 写入与画布预览都从这里取几何，「所见即生成」因此是**结构保证**
/// 而不是两套代码的巧合——对照画刷的同一条纪律（`Editor::brush_pending_notes_by_stroke`：
/// 预览与写入共用一个权威源）。块的 key 语义 = 音符所在音高行，渲染时占
/// `[key - 0.5, key + 0.5)` 的格子（与音符渲染 / 填充预览的 block 画法同一口径）。
///
/// `min_ticks` 必须参与长度补齐（[`paths::to_note_triple`]，默认 128 分音符）：预览要画
/// **补过长度**的结果——否则竖直段在预览里是 1 tick（实机 < 1px）、产物却是补齐过的，
/// 又变成"预览一套、产物一套"。
pub(crate) fn outline_blocks(
    poly: &[Pt],
    thickness: u8,
    zoom_x: f32,
    zoom_y: f32,
    key_hi: i32,
    min_ticks: f32,
) -> Vec<(f32, f32, u16)> {
    outline_notes(poly, thickness, zoom_x, zoom_y, key_hi)
        .into_iter()
        .filter(|n| n.key >= 0 && n.key < key_hi)
        .map(|n| {
            let (start, key, len) = paths::to_note_triple(&n, min_ticks);
            (start, start + len, key)
        })
        .collect()
}

/// 加粗带（**不含**中心线本身）的逐音高行覆盖。
///
/// 单侧加宽量 = `(thickness - 1) × zoom_y` 像素：粗细 N 层的含义是「中心线那一层
/// 之外再加 N-1 层」，所以 `thickness == 1` 时加宽量为 0 → 返回空（零行为变化）。
pub(crate) fn band_notes(
    poly: &[Pt],
    thickness: u8,
    zoom_x: f32,
    zoom_y: f32,
    key_hi: i32,
) -> Vec<RawNote> {
    if thickness <= 1 || poly.len() < 2 || zoom_x <= 0.0 || zoom_y <= 0.0 {
        return Vec::new();
    }
    let width_px = (thickness - 1) as f32 * zoom_y;
    let n = poly.len();
    // 每段的偏移向量（逻辑单位）；退化段为 None —— 既不贡献带宽，也不参与拐角补齐
    let offsets: Vec<Option<(f32, f32)>> = (0..n - 1)
        .map(|i| segment_offset(poly[i], poly[i + 1], zoom_x, zoom_y, width_px))
        .collect();

    // 逐行收集所有几何贡献的区间（BTreeMap 顺带保证输出按 key 有序）
    let mut spans_by_key: BTreeMap<i32, Vec<(f32, f32)>> = BTreeMap::new();

    // ① 每段一条单侧矩形：中心线段 → 加宽到偏移后的平行线段
    for i in 0..n - 1 {
        let Some((off_tick, off_key)) = offsets[i] else {
            continue;
        };
        let (ax, ay) = (poly[i].0 as f32, poly[i].1 as f32);
        let (bx, by) = (poly[i + 1].0 as f32, poly[i + 1].1 as f32);
        let quad = [
            (ax, ay),
            (ax + off_tick, ay + off_key),
            (bx + off_tick, by + off_key),
            (bx, by),
            (ax, ay),
        ];
        accumulate_quad(&quad, key_hi, &mut spans_by_key);
    }

    // ② 拐角补齐（外角方格）：详见模块文档「拐角必须单独补」
    //
    // 顶点范围：闭合环（首点 = 尾点，判定与 `paths::path_notes` 同一条）的每个顶点都是
    // 内部拐角 → `0..=n-2`（末点 = 首点副本，不重复处理）；开放折线的两端是端帽
    // （预览的 `Stroke` 默认 Butt）→ 只补内部顶点 `1..=n-2`。
    let closed = n > 3 && poly[0] == poly[n - 1];
    let v_lo = if closed { 0 } else { 1 };
    for j in v_lo..n - 1 {
        let prev = if j == 0 { n - 2 } else { j - 1 };
        let (Some(off_a), Some(off_b)) = (offsets[prev], offsets[j]) else {
            continue;
        };
        // 共线（同向）拐角：两向量张成的平行四边形零面积，补了只会留下 1 tick 碎音符。
        // 用**像素度量**下的叉积判定，与缩放无关（两条偏移向量的像素长度都等于
        // `width_px`，故叉积量级 = width_px²·sin(夹角)）。
        let cross =
            (off_a.0 * zoom_x) * (off_b.1 * zoom_y) - (off_a.1 * zoom_y) * (off_b.0 * zoom_x);
        if cross.abs() <= 1e-6 * width_px * width_px {
            continue;
        }
        let (vx, vy) = (poly[j].0 as f32, poly[j].1 as f32);
        let quad = [
            (vx, vy),
            (vx + off_a.0, vy + off_a.1),
            (vx + off_a.0 + off_b.0, vy + off_a.1 + off_b.1),
            (vx + off_b.0, vy + off_b.1),
            (vx, vy),
        ];
        accumulate_quad(&quad, key_hi, &mut spans_by_key);
    }

    spans_by_key
        .into_iter()
        .flat_map(|(key, spans)| {
            merge_spans(&spans).into_iter().filter_map(move |(a, b)| {
                // 完全落在时间轴左侧（tick < 0）的区间丢弃：时间轴没有负 tick，
                // 钳成 tick 0 会凭空多出一条 1 tick 假音符
                if b <= 0.0 {
                    return None;
                }
                // tick 取整：区间端点来自几何交点，与中心线音符同一量化口径。
                // **起点必须在这里就钳到 0**：偏左法线（如向右上的斜段）会让带在
                // tick 0 附近外溢到负 tick，若留到调用方的 `to_note_triple` 才钳，
                // 负起点与中心线的 0 起点在 `keep_longest` 里算成两组（`(-21,k)` ≠
                // `(0,k)`），钳完就留下两条同 start 同 key 的重叠音符。
                let start = (a.round() as i64).max(0);
                let end = (b.round() as i64).max(start + 1);
                Some(RawNote { start, end, key })
            })
        })
        .collect()
}

/// 一段折线的加宽偏移向量（逻辑单位：tick / key）——法线按「向上优先、其次向右」定号：
/// 方向确定（翻转线段走向不改变结果），且与画刷笔画"从落笔 key 向上铺层"同偏向。
///
/// 退化段（零长度 / 坐标非有限）返回 `None`。
fn segment_offset(a: Pt, b: Pt, zoom_x: f32, zoom_y: f32, width_px: f32) -> Option<(f32, f32)> {
    let (ax, ay) = (a.0 as f32, a.1 as f32);
    let (bx, by) = (b.0 as f32, b.1 as f32);
    if !ax.is_finite() || !ay.is_finite() || !bx.is_finite() || !by.is_finite() {
        return None;
    }
    // 像素度量下的方向：tick 方向 × zoom_x、key 方向 × zoom_y
    let (dx, dy) = ((bx - ax) * zoom_x, (by - ay) * zoom_y);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return None;
    }
    let (mut nx, mut ny) = (-dy / len, dx / len);
    if ny < 0.0 || (ny == 0.0 && nx < 0.0) {
        nx = -nx;
        ny = -ny;
    }
    // 法线偏移换算回逻辑单位
    Some((nx * width_px / zoom_x, ny * width_px / zoom_y))
}

/// 把一个闭环多边形（首尾重复）在它触及的每个音高行上解析成区间，累加进 `out`。
///
/// 行范围 = 四角 key 的外包（行带是 key ± 0.5，外扩一行兜底），越界行不产出。
fn accumulate_quad(quad: &[(f32, f32)], key_hi: i32, out: &mut BTreeMap<i32, Vec<(f32, f32)>>) {
    let k_min = quad
        .iter()
        .map(|p| p.1)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let k_max = quad
        .iter()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i32;
    for q in k_min..=k_max.min(key_hi) {
        let spans = row_spans(quad, q as f32);
        if !spans.is_empty() {
            out.entry(q).or_default().extend(spans);
        }
    }
}

#[cfg(test)]
mod tests;
