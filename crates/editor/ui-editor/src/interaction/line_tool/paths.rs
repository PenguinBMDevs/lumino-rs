//! 路径（(tick, key) 点列）→ 音符：蜘蛛网（Spiderweb）式逐音高行精确跨越
//!
//! 移植自 Spiderweb `scripts/notes/paths.py`，替换原先「按设定精度取格点、
//! 每格一个定长音符」的做法：
//!
//! - **不用设定精度**：曲线先按几何容差展平为折线（[`super::geom::flatten_path`]），
//!   再**解析**求出折线穿越每条音高行边界（key ± 0.5）的精确 tick；
//! - 每条音高行一条音符：起点 = 进入该行的 tick（四舍五入到整 tick），
//!   终点 = 下一条「起点更晚」的音符起点 → **无缝连奏（legato）**、长度自然变化；
//!   一条 key 60 → 64 的直线因此得到 5 条等时长的音符，而不是 5 个定长格子；
//! - 一段跨越多个音高行时**逐行展开**（在段内按行边界线性插值求交点），
//!   所以陡峭段不会漏行；
//! - 同一 tick 上跨多行：除最后一条外都取 1 tick，避免紧折角后接平坦段
//!   时两条长音符叠成实心块；
//! - 首尾各补成**完整行份额**（[`stretch_ends`]）：原做法首行/末行各只占半份时间；
//! - 闭合环（首点 = 尾点）从最左点重启、不做首尾拉伸，与自定义图形轮廓同规则。
//!
//! 全程 **f64**：`EDGE = 0.5 - 1e-6` 这种「行内微缩」在 f32 下会被舍入吃掉
//! （key ≈ 64 处 f32 间距约 3.8e-6，`64.499999` 舍成 `64.5`），音高行会凭空
//! 多出一行——Spiderweb 用 numpy 的 float64，同此。

/// 半行、略微内缩：把一个音高行的起止推到行的边界上
const EDGE: f64 = 0.5 - 1e-6;
/// 判定「时间未推进」的容差（tick），`ends_forward` 用
const TIME_EPS: f64 = 1e-12;
/// 判定「时间原地不动」（竖直段）的容差（tick），`line_notes` 用
const FLAT_EPS: f64 = 1e-9;

/// 折线上的一个点（逻辑坐标：tick, key）
type Pt = (f64, f64);

/// 音符生成中间结果（tick 单位、整数；key 为整数音高行）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RawNote {
    /// 起始 tick
    pub start: i64,
    /// 结束 tick（不含该 tick）
    pub end: i64,
    /// 音高行
    pub key: i32,
}

impl RawNote {
    /// 时长（tick，至少 1）
    pub(crate) fn length(&self) -> i64 {
        (self.end - self.start).max(1)
    }
}

/// 最接近 `y` 的音高行（行 `q` 覆盖 [q - 0.5, q + 0.5)）
fn pitch_of(y: f64) -> i32 {
    (y + 0.5).floor() as i32
}

/// 去掉与前一格完全重复的点
fn dedupe(points: &[Pt]) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    out
}

/// 方向（上 / 下）反转的段索引：段 `i` 从点 `i` 到 `i + 1`。
///
/// 只与「上一个真的动了的段」比较——原地不动的段被跳过（Spiderweb
/// `direction_changes`）。key 方向用它找转弯点，tick 方向用它把路径切成
/// 一段段从左到右的片。
fn direction_changes(v: &[f64]) -> Vec<usize> {
    let mut idx: Vec<usize> = Vec::new();
    let mut dir: Vec<f64> = Vec::new();
    for i in 0..v.len().saturating_sub(1) {
        let d = v[i + 1] - v[i];
        let s = if d > 0.0 {
            1.0
        } else if d < 0.0 {
            -1.0
        } else {
            0.0
        };
        if s != 0.0 {
            idx.push(i);
            dir.push(s);
        }
    }
    let mut out = Vec::new();
    for k in 1..idx.len() {
        if dir[k] != dir[k - 1] {
            out.push(idx[k]);
        }
    }
    out
}

/// 把 `pts[i..=j]` 的 key 拉伸，使首行 / 末行占满一整份而不是半份。
///
/// - `move_start`：起点推到首行边界（`ps - d * EDGE`）；
/// - `move_end`：终点推到末行边界外侧（`pe + d * EDGE`）；
///   若 `end_dot`，只推到末行边界内侧（末行「刚好够到」，其音符起于末点）。
fn remap(pts: &mut [Pt], i: usize, j: usize, move_start: bool, move_end: bool, end_dot: bool) {
    let (ys, ye) = (pts[i].1, pts[j].1);
    let (ps, pe) = (pitch_of(ys), pitch_of(ye));
    if ps == pe {
        return;
    }
    let d = if pe > ps { 1.0 } else { -1.0 };
    let ns = if move_start { ps as f64 - d * EDGE } else { ys };
    let ne = if move_end {
        if end_dot {
            pe as f64 - d * EDGE
        } else {
            pe as f64 + d * EDGE
        }
    } else {
        ye
    };
    let k = (ne - ns) / (ye - ys);
    for p in &mut pts[i..=j] {
        p.1 = ns + (p.1 - ys) * k;
    }
}

/// 一条线从 key 60 到 64 会经过 5 条音高行：给每条**等长的一份时间**。
///
/// 不做这一步，首行与末行只会各拿到半份（行的中心在端点上）。只有路径的
/// 起点与终点需要拉伸——转弯点本身已经是完整份额。
fn stretch_ends(pts: &mut [Pt], end_dot: bool) {
    if pts.len() < 2 {
        return;
    }
    let keys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let turns: Vec<usize> = std::iter::once(0)
        .chain(direction_changes(&keys))
        .chain(std::iter::once(pts.len() - 1))
        .collect();
    if turns.len() == 2 {
        remap(pts, 0, pts.len() - 1, true, true, end_dot);
    } else {
        remap(pts, 0, turns[1], true, false, end_dot);
        remap(
            pts,
            turns[turns.len() - 2],
            pts.len() - 1,
            false,
            true,
            end_dot,
        );
    }
}

/// 闭合环（首点 = 尾点）从**最左点**重新起头，从而切成一段段从左到右的折线
/// （从中间斜坡起头会在接缝处留下一道接痕）。
fn loop_from_left(pts: &[Pt]) -> Vec<Pt> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let body = &pts[..pts.len() - 1];
    // 字典序最小（先 tick 后 key）的点 = 最左点（同 tick 取最低 key）
    let mut i = 0;
    for (k, p) in body.iter().enumerate() {
        if (p.0, p.1) < (body[i].0, body[i].1) {
            i = k;
        }
    }
    let mut out: Vec<Pt> = body[i..].to_vec();
    out.extend_from_slice(&body[..=i]);
    out
}

/// 路径是不是「在时间上向前」走到它的末点？
///
/// 绕过末点又折回来的曲线（弧线超过半圈）是从时间反向到达末点的：那里的音符
/// 本来就已经起在末点上了，不需要 `end_dot` 的尾巴处理。
fn ends_forward(pts: &[Pt]) -> bool {
    let mut moved: Option<usize> = None;
    for i in (0..pts.len().saturating_sub(1)).rev() {
        if (pts[i + 1].0 - pts[i].0).abs() > TIME_EPS {
            moved = Some(i);
            break;
        }
    }
    match moved {
        None => true,
        Some(i) => pts[i + 1].0 > pts[i].0,
    }
}

/// 同一个 key 在同一个 tick 上起两次（片与片接缝处）：**保留最长的那条**，
/// 输出顺序按各组首次出现的顺序（Spiderweb `keep_longest`）。结束 tick < 0 的丢弃。
pub(crate) fn keep_longest(notes: &[RawNote]) -> Vec<RawNote> {
    let mut order: Vec<usize> = (0..notes.len()).filter(|&i| notes[i].end >= 0).collect();
    if order.len() < 2 {
        return order.into_iter().map(|i| notes[i]).collect();
    }
    order.sort_by(|&a, &b| {
        (notes[a].start, notes[a].key, a).cmp(&(notes[b].start, notes[b].key, b))
    });
    let mut groups: Vec<(usize, RawNote)> = Vec::new();
    for &i in &order {
        let n = notes[i];
        match groups.last_mut() {
            Some((_, prev)) if prev.start == n.start && prev.key == n.key => {
                if n.end > prev.end {
                    prev.end = n.end;
                }
            }
            _ => groups.push((i, n)),
        }
    }
    groups.sort_by_key(|&(i, _)| i);
    groups.into_iter().map(|(_, n)| n).collect()
}

/// 从左到右的「片」→ 音符，每个音高行一条。
///
/// `r` 是各片点列首尾重叠一个点的拼接；`first[k]` 是第 k 片的起始下标
/// （片 k 范围 = `first[k] .. first[k + 1] - 1`，最后一片到 `r` 末尾）。
/// `tails[k]`：第 k 片的末音符起于片的末点，取与前一音符相同的门长。
fn parts_notes(r: &[Pt], first: &[usize], tails: &[bool]) -> Vec<RawNote> {
    let k_parts = first.len();
    if k_parts == 0 || r.is_empty() {
        return Vec::new();
    }
    let n_pts = r.len();
    let pitch: Vec<i32> = r.iter().map(|p| pitch_of(p.1)).collect();

    // 每片的末点下标
    let mut part_last: Vec<usize> = Vec::with_capacity(k_parts);
    for k in 0..k_parts {
        part_last.push(if k + 1 < k_parts {
            first[k + 1] - 1
        } else {
            n_pts - 1
        });
    }
    // 片内段（跨片跳变的那个段不算）
    let mut in_part = vec![true; n_pts.saturating_sub(1)];
    for k in 0..k_parts.saturating_sub(1) {
        if part_last[k] < in_part.len() {
            in_part[part_last[k]] = false;
        }
    }
    // 跨越音高行的段
    let mut seg: Vec<usize> = Vec::new();
    for j in 0..n_pts.saturating_sub(1) {
        if in_part[j] && pitch[j] != pitch[j + 1] {
            seg.push(j);
        }
    }

    // 每个跨界段展开成「逐行穿越」：目标行 qq、行边界 key `yy`、边界 tick `tt`
    let mut cross_t: Vec<f64> = Vec::new();
    let mut cross_q: Vec<i32> = Vec::new();
    let mut cross_part: Vec<usize> = Vec::new();
    for &j in &seg {
        let (pa, pb) = (pitch[j], pitch[j + 1]);
        let st = if pb > pa { 1 } else { -1 };
        let count = (pb - pa).unsigned_abs() as usize;
        let pid = first.partition_point(|&f| f <= j) - 1;
        let (ta, ya) = r[j];
        let (tb, yb) = r[j + 1];
        for k in 0..count {
            let qq = pa + st * (k as i32 + 1);
            // 进入行 qq 要跨过的行边界
            let yy = qq as f64 - 0.5 * st as f64;
            let tt = ta + (tb - ta) * (yy - ya) / (yb - ya);
            cross_t.push(tt);
            cross_q.push(qq);
            cross_part.push(pid);
        }
    }

    // 每片的「进入点」= 片首点，其后按顺序接上穿越点
    let mut per = vec![1usize; k_parts];
    for &pid in &cross_part {
        per[pid] += 1;
    }
    let mut off = Vec::with_capacity(k_parts);
    let mut acc = 0usize;
    for &c in &per {
        off.push(acc);
        acc += c;
    }
    let m = acc;
    let mut et = vec![0f64; m];
    let mut ep = vec![0i32; m];
    for k in 0..k_parts {
        et[off[k]] = r[first[k]].0;
        ep[off[k]] = pitch[first[k]];
    }
    for i in 0..cross_t.len() {
        let pos = i + cross_part[i] + 1;
        et[pos] = cross_t[i];
        ep[pos] = cross_q[i];
    }

    let starts: Vec<i64> = et.iter().map(|&v| (v + 0.5).floor() as i64).collect();
    let part_end: Vec<i64> = part_last
        .iter()
        .map(|&l| (r[l].0 + 0.5).floor() as i64)
        .collect();
    // 每条 entry 属于哪一片（各片 entry 连续：先是片首点，再接穿越点）
    let part: Vec<usize> = (0..k_parts)
        .flat_map(|k| std::iter::repeat_n(k, per[k]))
        .collect();
    let e_last: Vec<usize> = (0..k_parts).map(|k| off[k] + per[k] - 1).collect();

    // 一条音符一直响到下一条**起点更晚**的音符开始
    // （很陡的线上好几个音高落在同一个 tick：除最后一条外都只给 1 tick，
    //  这样紧折角后面接平坦段时不会把两条长音符叠成一个实心块）
    let mut nxt_at = vec![m; m];
    {
        let mut nxt = m;
        for i in (0..m).rev() {
            if i + 1 < m && part[i + 1] == part[i] && starts[i + 1] > starts[i] {
                nxt = i;
            }
            nxt_at[i] = nxt;
        }
    }
    let mut ends = vec![0i64; m];
    for i in 0..m {
        let v = if nxt_at[i] < e_last[part[i]] {
            starts[(nxt_at[i] + 1).min(m - 1)]
        } else {
            part_end[part[i]]
        };
        ends[i] = v.max(starts[i] + 1);
    }
    for i in 0..m.saturating_sub(1) {
        if part[i + 1] == part[i] && starts[i + 1] == starts[i] {
            ends[i] = starts[i] + 1;
        }
    }
    // 末点尾巴：末音符起于片的末点，取与前一音符相同的门长
    for k in 0..k_parts {
        if tails.get(k).copied().unwrap_or(false) && per[k] >= 2 {
            let l = e_last[k];
            if l > 0 && starts[l] == part_end[k] {
                let gate = (ends[l - 1] - starts[l - 1]).max(1);
                ends[l] = starts[l] + gate;
            }
        }
    }

    (0..m)
        .map(|i| RawNote {
            start: starts[i],
            end: ends[i],
            key: ep[i],
        })
        .collect()
}

/// (tick, key) 点列 → 音符（未去重）。路径在**时间反向**处切开（每一片都从左到右），
/// 且在时间上不动的竖直段单独成片——它们的音符保持 1 tick，不与旁边的平坦段共享长度。
fn line_notes(pts: &[Pt], tail: bool) -> Vec<RawNote> {
    let n = pts.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        let tick = (pts[0].0 + 0.5).floor() as i64;
        return vec![RawNote {
            start: tick,
            end: tick + 1,
            key: pitch_of(pts[0].1),
        }];
    }
    // 时间方向反转处切开，每片从左到右
    let ticks: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let cuts = direction_changes(&ticks);
    let lo: Vec<usize> = std::iter::once(0).chain(cuts.iter().copied()).collect();
    let hi: Vec<usize> = cuts.iter().copied().chain(std::iter::once(n - 1)).collect();
    let mut q: Vec<Pt> = Vec::with_capacity(n + cuts.len());
    for k in 0..lo.len() {
        let (a, b) = (lo[k], hi[k]);
        if pts[b].0 < pts[a].0 {
            for i in (a..=b).rev() {
                q.push(pts[i]);
            }
        } else {
            q.extend_from_slice(&pts[a..=b]);
        }
    }
    let size: Vec<usize> = (0..lo.len()).map(|k| hi[k] - lo[k] + 1).collect();
    let mut piece_end = Vec::with_capacity(size.len());
    let mut acc = 0usize;
    for &s in &size {
        acc += s;
        piece_end.push(acc - 1);
    }
    // q 内相邻两点是不是同一段（不是片与片之间的跳变）
    let mut same = vec![true; q.len().saturating_sub(1)];
    for k in 0..piece_end.len().saturating_sub(1) {
        if piece_end[k] < same.len() {
            same[piece_end[k]] = false;
        }
    }
    // 时间上原地不动（竖直）的分段单独成片
    let up: Vec<bool> = (0..q.len().saturating_sub(1))
        .map(|i| (q[i + 1].0 - q[i].0).abs() < FLAT_EPS)
        .collect();
    let mut split: Vec<usize> = Vec::new();
    for i in 1..up.len() {
        if same[i] && same[i - 1] && up[i] != up[i - 1] {
            split.push(i);
        }
    }
    // 片的首下标 = 每片起点 + 竖直/平坦切换点
    let mut first: Vec<usize> = (0..size.len())
        .map(|k| piece_end[k] + 1 - size[k])
        .collect();
    first.extend_from_slice(&split);
    first.sort_unstable();
    let mut last: Vec<usize> = split;
    last.extend_from_slice(&piece_end);
    last.sort_unstable();
    // r = 各片首尾重叠一个点地拼接（`parts_notes` 按 first 切片）
    let mut r: Vec<Pt> = Vec::with_capacity(q.len() + first.len());
    let mut offsets: Vec<usize> = Vec::with_capacity(first.len());
    for k in 0..first.len() {
        offsets.push(r.len());
        r.extend_from_slice(&q[first[k]..=last[k]]);
    }
    let tails: Vec<bool> = if tail {
        (0..first.len())
            .map(|k| piece_end.contains(&last[k]) && q[last[k]] == pts[n - 1])
            .collect()
    } else {
        vec![false; first.len()]
    };
    parts_notes(&r, &offsets, &tails)
}

/// 一条路径（(tick, key) 点列）→ 音符。
///
/// 闭合环（首点 = 尾点）没有首尾：不做拉伸，从最左点重启。开放路径按
/// [`stretch_ends`] 拉伸首尾，终点在时间上折回时不做 `end_dot` 尾巴。
pub(crate) fn path_notes(path: &[Pt], end_dot: bool) -> Vec<RawNote> {
    let pts = dedupe(path);
    if pts.is_empty() {
        return Vec::new();
    }
    if pts.len() == 1 {
        return keep_longest(&line_notes(&pts, false));
    }
    if pts.len() > 3 && pts[0] == pts[pts.len() - 1] {
        return keep_longest(&line_notes(&loop_from_left(&pts), false));
    }
    // 从右往左画的路径反过来：末点仍是时间上更靠后的那一端
    let mut pts = pts;
    if pts[pts.len() - 1].0 < pts[0].0 {
        pts.reverse();
    }
    let end_dot = end_dot && ends_forward(&pts);
    stretch_ends(&mut pts, end_dot);
    keep_longest(&line_notes(&pts, end_dot))
}

#[cfg(test)]
mod tests;
