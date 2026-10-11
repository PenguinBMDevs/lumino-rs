//! 全局绘制粗细的**加粗几何**：把描边中心线按线段法线膨胀成有宽度的「带」
//!
//! ## 口径（与画布上画出来的线严格同源）
//!
//! 线宽 = 粗细层数 × 单 key 高（`zoom_y`），沿**线段法线**单侧铺开，偏向
//! 「向上优先、其次向右」（与画刷笔画"从落笔 key 向上铺层"同偏向）。于是：
//! - **水平段** → 纵向加厚 N 个 key；
//! - **竖直段** → 横向加宽 `(N-1)·zoom_y/zoom_x` 个 tick（**本模块存在的理由**）；
//! - **斜段** → 沿其法线加厚（不是按"水平段"硬套）。
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
//! 用「每段一条矩形 + 逐行求并」而不是「整条路径平移 N 次」的另一个好处：
//! 段与段在拐角处的重叠由区间求并自然融合，矩形在拐角外侧也**不会留缝**。
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

    // 逐行收集所有线段贡献的区间（BTreeMap 顺带保证输出按 key 有序）
    let mut spans_by_key: BTreeMap<i32, Vec<(f32, f32)>> = BTreeMap::new();
    for pair in poly.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (ax, ay) = (a.0 as f32, a.1 as f32);
        let (bx, by) = (b.0 as f32, b.1 as f32);
        if !ax.is_finite() || !ay.is_finite() || !bx.is_finite() || !by.is_finite() {
            continue;
        }
        // 像素度量下的方向：tick 方向 × zoom_x、key 方向 × zoom_y
        let (dx, dy) = ((bx - ax) * zoom_x, (by - ay) * zoom_y);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= f32::EPSILON {
            continue;
        }
        // 左法线，按「向上优先、其次向右」定号：
        // 方向确定（翻转与线段走向无关），且与画刷笔画的向上铺层同偏向。
        let (mut nx, mut ny) = (-dy / len, dx / len);
        if ny < 0.0 || (ny == 0.0 && nx < 0.0) {
            nx = -nx;
            ny = -ny;
        }
        // 法线偏移换算回逻辑单位
        let off_tick = nx * width_px / zoom_x;
        let off_key = ny * width_px / zoom_y;

        // 单侧矩形：中心线段 → 加宽到偏移后的平行线段（首尾重复 = 闭环形态）
        let quad = [
            (ax, ay),
            (ax + off_tick, ay + off_key),
            (bx + off_tick, by + off_key),
            (bx, by),
            (ax, ay),
        ];

        // 该段可能触及的音高行：四角 key 的外包范围（行带是 key ± 0.5，外扩一行兜底）
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
            let spans = row_spans(&quad, q as f32);
            if !spans.is_empty() {
                spans_by_key.entry(q).or_default().extend(spans);
            }
        }
    }

    spans_by_key
        .into_iter()
        .flat_map(|(key, spans)| {
            merge_spans(&spans).into_iter().map(move |(a, b)| {
                // tick 取整：区间端点来自几何交点，与中心线音符同一量化口径
                let start = a.round() as i64;
                let end = (b.round() as i64).max(start + 1);
                RawNote { start, end, key }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
