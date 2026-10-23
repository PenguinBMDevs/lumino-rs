//! Mask group 组合：按 Shapes 列表顺序叠加 Add / Subtract / Intersect
//!
//! 首个成员恒为 Add（其 `mode` 被忽略）；组的 `reach` 是所有成员 base 的并集，
//! 用于判断哪些形状被这组遮罩触及。

use lumino_core::shape::{MaskMode, MaskParams};

use super::area::Area;

/// 一个遮罩成员：参数 + base 区域（未做 own 最终加减）+ 自身音符跨度
#[derive(Debug, Clone, PartialEq)]
pub struct MaskMember {
    /// 遮罩参数
    pub params: MaskParams,
    /// 区域（尚未应用 invert/own）
    pub base: Area,
    /// 自身音符跨度（`own=false` 或 solid 时为空）
    pub own: Area,
}

/// 一组遮罩的最终区域
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnitArea {
    /// 最终可见区域（已处理 invert 与 own）
    pub area: Area,
    /// 覆盖判定用的 reach（base 并集，未处理 own）
    pub reach: Area,
    /// 最终是否为「洞」（invert）
    pub invert: bool,
}

/// 计算一组遮罩的最终区域；空组返回空
pub fn unit_area(members: &[MaskMember]) -> UnitArea {
    let Some(first) = members.first() else {
        return UnitArea::default();
    };
    if members.len() == 1 {
        return UnitArea {
            area: apply_own(&first.base, &first.own, first.params.invert),
            reach: first.base.clone(),
            invert: first.params.invert,
        };
    }

    let mut current = first.base.clone();
    let mut negated = first.params.invert;
    let mut reach = first.base.clone();
    let mut own = first.own.clone();
    for member in &members[1..] {
        reach = reach.joined(&member.base);
        own = own.joined(&member.own);
        let (mode, nx) = mode_of(&member.params);
        let (next, next_negated) = combine(&current, negated, &member.base, nx, mode);
        current = next;
        negated = next_negated;
    }
    UnitArea {
        area: apply_own(&current, &own, negated),
        reach,
        invert: negated,
    }
}

/// `Subtract` 等价于「与取反后的区域求交」
fn mode_of(params: &MaskParams) -> (MaskMode, bool) {
    match params.mode {
        MaskMode::Subtract => (MaskMode::Intersect, !params.invert),
        mode => (mode, params.invert),
    }
}

/// 上游 `_with`：把 `x`（是否取反 `nx`）按 `mode` 并入 `cur`（是否取反 `neg`）
fn combine(cur: &Area, neg: bool, x: &Area, nx: bool, mode: MaskMode) -> (Area, bool) {
    match mode {
        MaskMode::Add => {
            if !neg && !nx {
                (cur.joined(x), false)
            } else if neg && nx {
                (cur.common(x), true)
            } else if neg {
                (cur.minus(x), true)
            } else {
                (x.minus(cur), true)
            }
        }
        MaskMode::Intersect => {
            if !neg && !nx {
                (cur.common(x), false)
            } else if neg && nx {
                (cur.joined(x), true)
            } else if neg {
                (x.minus(cur), false)
            } else {
                (cur.minus(x), false)
            }
        }
        // mode_of 已把 Subtract 转为 Intersect；显式兜底避免未来变体遗漏
        MaskMode::Subtract => combine(cur, neg, x, !nx, MaskMode::Intersect),
    }
}

fn apply_own(area: &Area, own: &Area, invert: bool) -> Area {
    if invert {
        area.joined(own)
    } else {
        area.minus(own)
    }
}

#[cfg(test)]
mod tests;
