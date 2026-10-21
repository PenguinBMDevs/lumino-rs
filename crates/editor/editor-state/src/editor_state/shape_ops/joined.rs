//! Join / Split（DRAW-021）
//!
//! 把线/折线/手绘/曲线/弧串成一条 Curve；段类型与锚点保留，空隙记录在
//! `JoinedParams.gaps`，`sources` 用于 Split separate 还原。

use lumino_core::shape::{
    JoinedParams, ShapeAnchor, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, StrokeKind,
};

use super::bezier::{reverse_anchors, to_bezier};

/// 路径点（tick, key）
pub type Pt = (f32, f32);

/// 一段来源
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// 段类型
    pub kind: StrokeKind,
    /// 锚点
    pub anchors: Vec<ShapeAnchor>,
    /// 来源 id
    pub src: u32,
}

/// 连接结果
#[derive(Debug, Clone, PartialEq)]
pub struct JoinedShape {
    /// 新形状
    pub model: ShapeModel,
    /// 连接参数
    pub params: JoinedParams,
    /// 各段（保真用）
    pub segments: Vec<Segment>,
    /// 原始形状
    pub sources: Vec<ShapeModel>,
    /// 刚连接时的锚点快照（判断绘图是否被改）
    pub original_anchors: Vec<ShapeAnchor>,
}

/// 贪心串联；`touch` 是端点相接容差
pub fn join(shapes: &[ShapeModel], touch: f32, k: f32) -> Option<JoinedShape> {
    let first = shapes.first()?;
    let paths = shapes
        .iter()
        .map(|shape| to_bezier(shape, k).0)
        .collect::<Vec<_>>();
    if paths.iter().any(|path| path.len() < 2) {
        return None;
    }
    let mut remaining = (1..shapes.len()).collect::<Vec<_>>();
    let mut chain = vec![(0usize, false)];
    let mut gaps = Vec::new();
    while !remaining.is_empty() {
        let front = chain.first()?.0;
        let back = chain.last()?.0;
        let front_point = paths[front].first()?.pos;
        let back_point = paths[back].last()?.pos;
        let mut best: Option<(usize, bool, bool, f32)> = None;
        for &index in &remaining {
            let anchors = &paths[index];
            let start = anchors.first()?.pos;
            let end = anchors.last()?.pos;
            for (at_front, reverse, point) in [
                (true, false, start),
                (true, true, end),
                (false, false, start),
                (false, true, end),
            ] {
                let base = if at_front { front_point } else { back_point };
                let distance = dist(base, point);
                if best.is_none_or(|(_, _, _, d)| distance < d) {
                    best = Some((index, at_front, reverse, distance));
                }
            }
        }
        let (index, at_front, reverse, distance) = best?;
        remaining.retain(|value| *value != index);
        if distance > touch.max(0.0) {
            let segment_index = if at_front { 0 } else { chain.len() } as u16;
            gaps.push(segment_index);
        }
        if at_front {
            chain.insert(0, (index, reverse));
        } else {
            chain.push((index, reverse));
        }
    }
    let mut segments = Vec::with_capacity(chain.len());
    let mut anchors = Vec::new();
    let mut src = Vec::with_capacity(chain.len());
    let mut tumours = Vec::with_capacity(chain.len());
    for (index, reverse) in chain {
        let shape = &shapes[index];
        let mut segment_anchors = paths[index].clone();
        if reverse {
            reverse_anchors(&mut segment_anchors);
        }
        let segment = Segment {
            kind: StrokeKind::from_kind(shape.kind),
            anchors: segment_anchors,
            src: u32::try_from(shape.id).ok()?,
        };
        anchors.extend(segment.anchors.iter().copied());
        src.push(segment.src);
        tumours.push(shape.ops.iter().find_map(|op| match op {
            ShapeOp::Tumours(params) => Some(params.clone()),
            _ => None,
        }));
        segments.push(segment);
    }
    let params = JoinedParams {
        gaps,
        tumours,
        splits: Vec::new(),
        src,
    };
    let id = shapes.iter().map(|shape| shape.id).max().unwrap_or(0) + 1;
    let mut model = ShapeModel::new(id, first.track, ShapeKind::Curve, first.k);
    model.geometry = ShapeGeometry {
        anchors: anchors.clone(),
        rect: None,
        closed: false,
    };
    model.style = first.style.clone();
    model.ops = vec![ShapeOp::Joined(params.clone())];
    Some(JoinedShape {
        model,
        params,
        segments,
        sources: shapes.to_vec(),
        original_anchors: anchors,
    })
}

/// 在锚点处切开；两半共享切点
pub fn split_here(joined: &JoinedShape, anchor: u16) -> (JoinedShape, JoinedShape) {
    let index = usize::from(anchor).min(joined.model.geometry.anchors.len().saturating_sub(1));
    let left_anchors = joined.model.geometry.anchors[..=index].to_vec();
    let right_anchors = joined.model.geometry.anchors[index..].to_vec();
    let left = clone_with_anchors(joined, left_anchors, joined.model.id);
    let right = clone_with_anchors(joined, right_anchors, joined.model.id + 1);
    (left, right)
}

/// 还原原始形状；连接后改过锚点则返回 `None`
pub fn split_separate(joined: &JoinedShape) -> Option<Vec<ShapeModel>> {
    (joined.model.geometry.anchors == joined.original_anchors).then(|| joined.sources.clone())
}

fn clone_with_anchors(joined: &JoinedShape, anchors: Vec<ShapeAnchor>, id: u64) -> JoinedShape {
    let mut model = joined.model.clone();
    model.id = id;
    model.geometry.anchors = anchors.clone();
    let mut params = joined.params.clone();
    params.splits.push(0);
    model.ops = vec![ShapeOp::Joined(params.clone())];
    JoinedShape {
        model,
        params,
        segments: joined.segments.clone(),
        sources: joined.sources.clone(),
        original_anchors: anchors,
    }
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests;
