//! Copy along path 的路径/落点

use lumino_core::shape::{CopyPathParams, ShapeRect, Speed};

/// 路径点
pub type Pt = (f32, f32);

/// 路径方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// 上行
    Up,
    /// 下行
    Down,
}

/// 一段路径
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Run {
    /// 起点
    pub a: Pt,
    /// 终点
    pub b: Pt,
}

/// 一次副本落点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// 该副本在 run 上的归一化位置
    pub t: f32,
    /// 副本角点
    pub corner: Pt,
    /// 尺寸比例
    pub scale: [f32; 2],
    /// 方向（用于翻转）
    pub dir: Dir,
    /// 副本序号
    pub index: usize,
}

/// 形状角点：上行取左下，下行取左上
pub fn corner_a(rect: &ShapeRect, dir: Dir) -> Pt {
    match dir {
        Dir::Up => (rect.tick_lo, rect.key_lo),
        Dir::Down => (rect.tick_lo, rect.key_hi),
    }
}

/// 一段路径上均分/按速度落点
pub fn place_on_run(run: &Run, count: u32, speed: Speed, size: [f32; 2]) -> Vec<Placement> {
    if count == 0 {
        return Vec::new();
    }
    let dir = if run.b.1 >= run.a.1 {
        Dir::Up
    } else {
        Dir::Down
    };
    let sx = size[0] / 100.0;
    let sy = size[1] / 100.0;
    (1..=count)
        .map(|index| {
            let raw = index as f32 / (count + 1) as f32;
            let scale_t = index as f32 / count as f32;
            let t = speed_at(speed, raw);
            Placement {
                t,
                corner: (
                    run.a.0 + (run.b.0 - run.a.0) * t,
                    run.a.1 + (run.b.1 - run.a.1) * t,
                ),
                scale: [1.0 + (sx - 1.0) * scale_t, 1.0 + (sy - 1.0) * scale_t],
                dir,
                index: index as usize,
            }
        })
        .collect()
}

fn speed_at(speed: Speed, x: f32) -> f32 {
    match speed {
        Speed::Even | Speed::Custom => x,
        Speed::Up => x * x,
        Speed::Down => 1.0 - (1.0 - x) * (1.0 - x),
    }
}

/// 由参数构造路径；`grp` 为组 box 首起点，`base` 为形状包围盒原点
///
/// **起点只有一个权威**：有 `grp` 就用 `grp`，绝不用调用方传进来的 `base`。
///
/// 历史 bug：起点曾用 `base`（当前包围盒原点）。音符包围盒的 key 是**整数**
/// （`ShapeNote.key: u16`），而 `grp` 是连续浮点；组合结果向下平移时两者相差
/// 会在 ±1 键之间来回跨越，首段的 `b.y >= a.y` 判定随之反复翻转，
/// `Dir::Down` 把副本上下镜像 —— 表现为「每经过一个音符中心/分界就翻一次」。
pub fn runs(params: &CopyPathParams, base: Pt) -> Vec<Run> {
    if params.pts.is_empty() {
        return Vec::new();
    }
    let origin = params.grp.map(|grp| (grp[0], grp[1])).unwrap_or(base);
    let mut points = vec![origin];
    points.extend(
        params
            .pts
            .iter()
            .map(|p| (origin.0 + p[0], origin.1 + p[1])),
    );
    points
        .windows(2)
        .map(|pair| Run {
            a: pair[0],
            b: pair[1],
        })
        .collect()
}

#[cfg(test)]
mod tests;
