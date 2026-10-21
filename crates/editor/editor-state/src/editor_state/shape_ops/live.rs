//! Turn into live shape（DRAW-019）
//!
//! 把 N 个形状折叠成 1 个 `ShapeKind::Custom` + `ShapeOp::Live`：
//! 每段保留原类型/锚点，音符生成链不变；`from` 供未改绘图时 Split separate。

use lumino_core::shape::{
    LiveShapeParams, LiveWarning, ShapeAnchor, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp,
    Stroke, StrokeKind,
};
/// Live 合并错误
#[derive(Debug, Clone, PartialEq)]
pub enum LiveError {
    /// 没有输入
    Empty,
    /// 嵌套超过 2 层
    TooDeep,
    /// 形状 id 超出 u32
    IdOverflow(u64),
    /// 几何为空
    NoGeometry(u64),
}

impl std::fmt::Display for LiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Live 至少需要一个形状"),
            Self::TooDeep => write!(f, "Live 嵌套超过 2 层"),
            Self::IdOverflow(id) => write!(f, "形状 id {id} 超出 u32"),
            Self::NoGeometry(id) => write!(f, "形状 {id} 没有可转换的几何"),
        }
    }
}

impl std::error::Error for LiveError {}

/// 合并多个形状；返回新形状与丢弃设置提示
pub fn to_live(shapes: &[ShapeModel]) -> Result<(ShapeModel, Vec<LiveWarning>), LiveError> {
    let Some(first) = shapes.first() else {
        return Err(LiveError::Empty);
    };
    if shapes
        .iter()
        .any(|shape| shape.ops.iter().any(|op| matches!(op, ShapeOp::Live(_))))
    {
        return Err(LiveError::TooDeep);
    }
    let mut strokes = Vec::with_capacity(shapes.len());
    let mut src = Vec::with_capacity(shapes.len());
    let mut warnings = Vec::new();
    for shape in shapes {
        let mut anchors = shape.geometry.anchors.clone();
        if anchors.is_empty()
            && let Some(rect) = shape.geometry.rect
        {
            anchors = vec![
                ShapeAnchor::new((rect.tick_lo, rect.key_lo)),
                ShapeAnchor::new((rect.tick_hi, rect.key_lo)),
                ShapeAnchor::new((rect.tick_hi, rect.key_hi)),
                ShapeAnchor::new((rect.tick_lo, rect.key_hi)),
            ];
        }
        if anchors.is_empty() {
            return Err(LiveError::NoGeometry(shape.id));
        }
        strokes.push(Stroke {
            kind: StrokeKind::from_kind(shape.kind),
            anchors,
            rect: shape.geometry.rect,
        });
        src.push(u32::try_from(shape.id).map_err(|_| LiveError::IdOverflow(shape.id))?);
    }
    let id = shapes.iter().map(|shape| shape.id).max().unwrap_or(0) + 1;
    let params = LiveShapeParams {
        strokes,
        src,
        from: shapes.to_vec(),
    };
    let mut ops = vec![ShapeOp::Live(params)];
    for variant in [
        Variant::Glue,
        Variant::Chop,
        Variant::Claw,
        Variant::Strum,
        Variant::Cycle,
    ] {
        match common_op(shapes, variant) {
            Some(op) => ops.push(op),
            None if shapes
                .iter()
                .any(|shape| variant.find(&shape.ops).is_some()) =>
            {
                warnings.push(LiveWarning {
                    message: format!("{} 设置不一致，已丢弃", variant.name()),
                });
            }
            None => {}
        }
    }
    let anchors = shapes
        .iter()
        .flat_map(|shape| shape.geometry.anchors.iter().copied())
        .collect::<Vec<_>>();
    let mut model = ShapeModel::new(id, first.track, ShapeKind::Custom, first.k);
    model.geometry = ShapeGeometry {
        anchors,
        rect: None,
        closed: false,
    };
    model.style = first.style.clone();
    model.ops = ops;
    Ok((model, warnings))
}

/// 未改绘图时还原原始形状；`drawing_changed = true` 返回 `None`
pub fn split_back(live: &ShapeModel, drawing_changed: bool) -> Option<Vec<ShapeModel>> {
    if drawing_changed {
        return None;
    }
    live.ops.iter().find_map(|op| match op {
        ShapeOp::Live(params) if !params.from.is_empty() => Some(params.from.clone()),
        _ => None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    Glue,
    Chop,
    Claw,
    Strum,
    Cycle,
}

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Glue => "Glue",
            Self::Chop => "Chop",
            Self::Claw => "Claw",
            Self::Strum => "Strum",
            Self::Cycle => "Colours",
        }
    }

    fn find(self, ops: &[ShapeOp]) -> Option<&ShapeOp> {
        ops.iter().find(|op| {
            matches!(
                (self, op),
                (Self::Glue, ShapeOp::Glue(_))
                    | (Self::Chop, ShapeOp::Chop(_))
                    | (Self::Claw, ShapeOp::Claw(_))
                    | (Self::Strum, ShapeOp::Strum(_))
                    | (Self::Cycle, ShapeOp::Cycle(_))
            )
        })
    }
}

fn common_op(shapes: &[ShapeModel], variant: Variant) -> Option<ShapeOp> {
    let first = variant.find(&shapes.first()?.ops)?.clone();
    shapes
        .iter()
        .all(|shape| shape.ops.iter().any(|op| op == &first))
        .then_some(first)
}

#[cfg(test)]
mod tests;
