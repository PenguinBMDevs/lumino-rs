//! 全局粗细「加粗带」几何测试
//!
//! 量的是**方向无关的宽度**：水平段纵向加厚、竖直段横向加宽、斜段沿法线加厚。
//! 用户报的 BUG（"矩形竖边改了粗细还是细一条"）就钉在
//! [`test_vertical_segment_widens_horizontally`] 这条上——旧实现（按 key 向上平移
//! N 层）在这里必然失败：竖直段被平移只是变长/重复，横向宽度不动。
//!
//! 另一类 BUG（"矩形右上角有缺口、音符没盖到角上"）钉在文件末尾的拐角补齐三条上：
//! [`test_open_path_corner_gets_outer_square`] / [`test_collinear_vertex_adds_no_note`] /
//! [`test_closed_loop_corner_at_wrap_vertex`]——每段一条单侧矩形在拐角外侧各留一块
//! 方格，必须用相邻两段偏移向量张成的平行四边形补齐（预览的 Miter 拐角是实心的）。

use super::*;

/// 默认缩放口径（与 `ViewState::default()` 同源的量级）：1 tick = 0.1px、1 key = 20px
const ZX: f32 = 0.1;
const ZY: f32 = 20.0;
const KEY_HI: i32 = 127;

/// 每个 key 行 → 覆盖区间（按起点升序），便于断言"哪一行被覆盖了多宽"
fn by_key(notes: &[RawNote]) -> BTreeMap<i32, Vec<(i64, i64)>> {
    let mut map: BTreeMap<i32, Vec<(i64, i64)>> = BTreeMap::new();
    for n in notes {
        map.entry(n.key).or_default().push((n.start, n.end));
    }
    for v in map.values_mut() {
        v.sort_unstable();
    }
    map
}

/// 粗细 1 = 不加粗：带为空、`outline_notes` 与中心线逐位一致（零行为变化）。
#[test]
fn test_thickness_one_adds_no_band() {
    let poly = [(0.0, 60.0), (800.0, 60.0)];
    assert!(
        band_notes(&poly, 1, ZX, ZY, KEY_HI).is_empty(),
        "粗细 1 不应产生任何加粗带"
    );
    assert_eq!(
        outline_notes(&poly, 1, ZX, ZY, KEY_HI),
        paths::path_notes(&poly, false),
        "粗细 1 时出口应与中心线生成逐位一致"
    );
}

/// 退化输入不 panic、不产生垃圾：空折线 / 单点 / 零缩放 / 零长度段。
#[test]
fn test_degenerate_inputs_are_ignored() {
    assert!(band_notes(&[], 3, ZX, ZY, KEY_HI).is_empty());
    assert!(band_notes(&[(0.0, 60.0)], 3, ZX, ZY, KEY_HI).is_empty());
    assert!(
        band_notes(&[(0.0, 60.0), (800.0, 60.0)], 3, 0.0, ZY, KEY_HI).is_empty(),
        "zoom_x = 0 时应直接放弃（不产生 inf/NaN 坐标）"
    );
    assert!(
        band_notes(&[(0.0, 60.0), (0.0, 60.0)], 3, ZX, ZY, KEY_HI).is_empty(),
        "零长度段不应贡献区间"
    );
}

/// 水平段：加粗带沿**纵向**铺 `thickness - 1` 层（向上），不改 tick 区间。
///
/// 这是与旧实现口径一致的那一半（旧实现的向上平移对水平段恰好成立），必须保持：
/// 粗细 N ⇒ 覆盖 key `k ..= k + N - 1` 共 N 行，tick 区间 = 线段本身。
#[test]
fn test_horizontal_segment_thickens_upward() {
    let poly = [(0.0, 60.0), (800.0, 60.0)];
    let band = band_notes(&poly, 3, ZX, ZY, KEY_HI);
    let map = by_key(&band);

    assert_eq!(
        map.keys().copied().collect::<Vec<_>>(),
        vec![60, 61, 62],
        "粗细 3 的水平段应覆盖 key 60/61/62 三行（向上铺，不下探）"
    );
    for (key, spans) in &map {
        assert_eq!(
            spans,
            &vec![(0, 800)],
            "key {key} 的 tick 区间应与线段一致（加粗只改垂直方向）"
        );
    }
}

/// **竖直段：加粗带沿横向铺开**（本模块要修的 BUG）。
///
/// 宽度 = `(thickness - 1) × zoom_y` 像素，换算成 tick = `(N-1)·zoom_y/zoom_x`。
/// 旧实现（按 key 向上平移）在这里横向宽度恒为 1 tick → 用户看到"竖边很细一条"。
#[test]
fn test_vertical_segment_widens_horizontally() {
    let poly = [(0.0, 60.0), (0.0, 64.0)];
    let expected_ticks = ((3 - 1) as f32 * ZY / ZX).round() as i64; // (N-1) 个 key 高
    let band = band_notes(&poly, 3, ZX, ZY, KEY_HI);
    let map = by_key(&band);

    assert_eq!(
        map.keys().copied().collect::<Vec<_>>(),
        vec![60, 61, 62, 63, 64],
        "竖直段应覆盖它穿过的全部音高行"
    );
    for (key, spans) in &map {
        assert_eq!(
            spans,
            &vec![(0, expected_ticks)],
            "key {key} 应被横向加宽到 {expected_ticks} tick（= 2 个 key 高的像素宽度）"
        );
    }
    // 反向也成立：加粗方向与线段走向无关（自下而上 / 自上而下同宽）
    let reversed = band_notes(&[(0.0, 64.0), (0.0, 60.0)], 3, ZX, ZY, KEY_HI);
    assert_eq!(
        by_key(&reversed),
        map,
        "线段的走向不应改变加粗结果（法线定号是绝对的）"
    );
}

/// 斜段：加粗沿**法线**（既不是纯纵向、也不是纯横向），宽度与水平/竖直段同源。
///
/// 45° 段（本用例构造的正是 45°：tick 方向 80px、key 方向 80px）：法线 = (−1,1)/√2，
/// 于是加粗带既向上推 key、又向左推 tick；每一行的横向弦长 = 带宽 / sin45° ≈ 1.41·W。
/// 断言给的是**区间**而不是精确值——行带模型（行 = key ± 0.5）在端行会被拐角切片，
/// 端行的弦长本就应该更短，钉死端行数值等于把测试绑死在离散化细节上。
#[test]
fn test_diagonal_segment_widens_along_normal() {
    // 45°：tick 方向 800 tick × 0.1px = 80px，key 方向 4 key × 20px = 80px
    let poly = [(0.0, 60.0), (800.0, 64.0)];
    let thickness = 2_u8;
    let width_px = (thickness - 1) as f32 * ZY;
    let band = band_notes(&poly, thickness, ZX, ZY, KEY_HI);
    assert!(!band.is_empty(), "斜段也必须被加粗");

    let map = by_key(&band);
    // ① 中段行的弦长应 ≥ 1.2 × 带宽（45° 的弦长放大系数 1.41，留出离散化余量）
    let mid = map.get(&62).expect("45° 段的中段行 62 应被覆盖");
    let mid_px = (mid[0].1 - mid[0].0) as f32 * ZX;
    assert!(
        mid_px >= width_px * 1.2,
        "45° 段中段行的横向弦长 {mid_px:.1}px 应 ≥ 1.2 × 带宽 {width_px:.1}px（法线加粗）"
    );

    // ② 整条带的最宽处不小于带宽（任何一种方向都成立的不变式）
    let max_px = map
        .values()
        .flatten()
        .map(|(a, b)| (*b - *a) as f32 * ZX)
        .fold(f32::MIN, f32::max);
    assert!(
        max_px >= width_px,
        "加粗带最宽处 {max_px:.1}px 应 ≥ 带宽 {width_px:.1}px"
    );

    // ③ 法线的 +key 分量：带应超出线段的 key 上界
    let max_key = band.iter().map(|n| n.key).max().unwrap_or(i32::MIN);
    assert!(
        max_key > 64,
        "45° 段的加粗带应超出线段的 key 上界（法线的 +key 分量），实际最大 key {max_key}"
    );
}

/// `outline_notes` = 中心线 + 加粗带，且短的中心线音符被更长的带吸收（不留细线夹层）。
#[test]
fn test_outline_notes_merges_centerline_with_band() {
    let poly = [(0.0, 60.0), (0.0, 64.0)];
    let notes = outline_notes(&poly, 3, ZX, ZY, KEY_HI);
    let map = by_key(&notes);
    let expected_ticks = ((3 - 1) as f32 * ZY / ZX).round() as i64;

    assert_eq!(
        map.keys().copied().collect::<Vec<_>>(),
        vec![60, 61, 62, 63, 64],
        "竖直段整体覆盖 60..=64"
    );
    for (key, spans) in &map {
        assert_eq!(
            spans,
            &vec![(0, expected_ticks)],
            "key {key} 只应剩一条加宽后的音符（中心线的 1 tick 短线已被同 tick 同 key 合并）"
        );
    }
}

/// 越界音高行裁到 `key_hi`：加粗不得把音符顶出编辑器渲染范围。
#[test]
fn test_band_clamps_to_key_hi() {
    // key 126 的水平段 + 粗细 5 → 会往上铺到 130，必须裁到 key_hi
    let poly = [(0.0, 126.0), (400.0, 126.0)];
    let band = band_notes(&poly, 5, ZX, ZY, 127);
    let keys: Vec<i32> = band.iter().map(|n| n.key).collect();
    assert!(!keys.is_empty(), "至少应保留 key_hi 以内的行");
    assert!(
        keys.iter().all(|k| *k <= 127),
        "加粗带不得越过 key_hi=127，实际 {keys:?}"
    );
}

/// 时间轴左侧的溢出：带在 tick 0 附近外溢到负 tick 时，起点必须在**本模块内**就钳到 0。
///
/// 留到调用方（`paths::to_note_triple`）才钳会漏掉合并：`keep_longest` 按
/// `(start, key)` 分组，`(-21, 60)` 与中心线的 `(0, 60)` 是两组，钳完就变成两条
/// 同 start 同 key 的重叠音符（渲染叠层 + 历史里两条）。
#[test]
fn test_band_clamps_negative_start_before_dedupe() {
    // 向右上的斜段（法线带 +tick 分量）：带在 tick 0 附近向左外溢到负 tick
    let poly = [(0.0, 60.0), (800.0, 64.0)];
    let notes = outline_notes(&poly, 3, ZX, ZY, KEY_HI);
    assert!(
        notes.iter().all(|n| n.start >= 0),
        "不得出现负起点音符：{:?}",
        notes.iter().filter(|n| n.start < 0).collect::<Vec<_>>()
    );
    let mut seen = std::collections::HashSet::new();
    for n in &notes {
        assert!(
            seen.insert((n.start, n.key)),
            "出现重复音符（同 start 同 key）: {n:?}"
        );
    }
}

// ─── 拐角补齐（用户报「矩形右上角缺口」的根因）─────────────────────────//
// 单侧矩形在拐角**外侧**各留一块方格：矩形 A 止于角点的 tick、矩形 B 止于角点的
// key，谁都不越界到「tick > 角点 ∧ key > 角点」那一格。预览是 `Stroke` 默认 Miter
// 拼接（拐角实心），所以生成侧必须用「两条偏移向量张成的平行四边形」把外角补上。

/// 开放折线的内部拐角：外角方格必须被补上，且端帽不越界（预览默认 Butt）。
///
/// 折线 = 顶边向右 + 右边向下，拐角正是用户报的**右上角**：粗细 3（偏移 2 个 key 高
/// = 400 tick / 2 行）时，角点外角方格 = `tick 800..1200 × key 64..66`。
/// 断言逐行精确区间：拐角补齐前，64/65/66 行只有 `[0,800]`（顶边自身），
/// 缺的正是 `[800,1200]` 这一段。
#[test]
fn test_open_path_corner_gets_outer_square() {
    let poly = [(0.0, 64.0), (800.0, 64.0), (800.0, 60.0)];
    let map = by_key(&band_notes(&poly, 3, ZX, ZY, KEY_HI));

    let expected: BTreeMap<i32, Vec<(i64, i64)>> = [
        (60, vec![(800, 1200)]),
        (61, vec![(800, 1200)]),
        (62, vec![(800, 1200)]),
        (63, vec![(800, 1200)]),
        // 拐角补齐：顶边带 [0,800] 与右边带 [800,1200] 在 64 行相接，
        // 64..66 行合并成一条贯通区间（外角方格被填上）
        (64, vec![(0, 1200)]),
        (65, vec![(0, 1200)]),
        (66, vec![(0, 1200)]),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        map, expected,
        "开放折线的外角方格必须补齐；端帽（tick 1200 之外）不得外溢"
    );
}

/// 共线（同向）拐角：平行四边形退化为零面积，**不得**留下碎音符。
///
/// 不加这个守卫时，退化四边形的两条重合边会被 even-odd 配成 `(x, x)` 的零宽区间，
/// 再被「至少 1 tick」兜底成每行一条 1 tick 的垃圾音符——直线中间凭空多出一列音符。
#[test]
fn test_collinear_vertex_adds_no_note() {
    let straight = band_notes(&[(0.0, 60.0), (1600.0, 60.0)], 3, ZX, ZY, KEY_HI);
    let with_mid_vertex = band_notes(
        &[(0.0, 60.0), (800.0, 60.0), (1600.0, 60.0)],
        3,
        ZX,
        ZY,
        KEY_HI,
    );
    assert_eq!(
        by_key(&with_mid_vertex),
        by_key(&straight),
        "共线拐角不应改变加粗带（不能在直线上留下 1 tick 碎音符）"
    );
}

/// **闭合环**首顶点（回绕拐角）同样要补：顶点 0 的「前一段」是最后一段。
///
/// 对抗性构造：把矩形的顶点序旋转到**缺失的那个外角**落在下标 0——如果回绕取的是
/// `按 j-1 直接减`（越界/取错段），或者干脆漏掉顶点 0，右上角立刻又缺回来。
#[test]
fn test_closed_loop_corner_at_wrap_vertex() {
    // 顶点序：(右上) → 右下 → 左下 → 左上 → 回到右上（首尾逐位相等 = 闭合环）
    let poly = [
        (800.0, 64.0),
        (800.0, 60.0),
        (0.0, 60.0),
        (0.0, 64.0),
        (800.0, 64.0),
    ];
    let map = by_key(&band_notes(&poly, 3, ZX, ZY, KEY_HI));

    let expected: BTreeMap<i32, Vec<(i64, i64)>> = [
        // 下边带 + 左边带 + 右边带在 60..62 行首尾相接成一条
        (60, vec![(0, 1200)]),
        (61, vec![(0, 1200)]),
        (62, vec![(0, 1200)]),
        // 63 行只有左右竖边覆盖 → 露出「偏移后的洞」
        (63, vec![(0, 400), (800, 1200)]),
        (64, vec![(0, 1200)]),
        // 65/66 行 = 上边带 [0,800] + 顶点 0 的外角方格 [800,1200]
        (65, vec![(0, 1200)]),
        (66, vec![(0, 1200)]),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        map, expected,
        "闭合环的首顶点拐角必须补齐（前一段 = 最后一段）"
    );
}
