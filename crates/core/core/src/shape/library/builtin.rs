//! 内置三形：Circle / Square / Triangle（DRAW-033A）
//!
//! 全部使用单位框 0..=1 的单条闭合 Polyline；抽屉与工程映射共用同一份数据，
//! 避免 UI 预览与落到工程的几何各写一套。
use crate::shape::library::ShapeDrawing;
use crate::shape::{ShapeAnchor, ShapeRect, Stroke, StrokeKind, StrokeRole};

/// 内置图形类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinShape {
    /// 圆
    Circle,
    /// 正方形
    Square,
    /// 三角形
    Triangle,
}

impl BuiltinShape {
    /// 全部内置图形（固定顺序：Circle / Square / Triangle）
    pub const ALL: [Self; 3] = [Self::Circle, Self::Square, Self::Triangle];

    /// 稳定 ID（也是文件名/查找键）
    pub const fn id(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Square => "square",
            Self::Triangle => "triangle",
        }
    }

    /// 默认显示名（UI 可自行 i18n；core 不依赖文案目录）
    pub const fn name(self) -> &'static str {
        match self {
            Self::Circle => "Circle",
            Self::Square => "Square",
            Self::Triangle => "Triangle",
        }
    }

    /// 生成对应的库图形
    pub fn drawing(self) -> ShapeDrawing {
        match self {
            Self::Circle => circle_drawing(),
            Self::Square => square_drawing(),
            Self::Triangle => triangle_drawing(),
        }
    }
}

/// 内置三形列表
pub fn builtin_drawings() -> Vec<ShapeDrawing> {
    BuiltinShape::ALL
        .into_iter()
        .map(BuiltinShape::drawing)
        .collect()
}

fn drawing(id: &'static str, name: &'static str, points: Vec<(f32, f32)>) -> ShapeDrawing {
    let anchors: Vec<ShapeAnchor> = points.into_iter().map(ShapeAnchor::new).collect();
    ShapeDrawing {
        id: id.to_string(),
        name: name.to_string(),
        strokes: vec![Stroke {
            kind: StrokeKind::Polyline,
            anchors,
            rect: Some(ShapeRect::normalized(0.0, 0.0, 1.0, 1.0)),
        }],
        areas: Vec::new(),
        roles: vec![StrokeRole::default()],
    }
}

fn circle_drawing() -> ShapeDrawing {
    // 64 段折线近似；首尾同点，闭合判断不依赖 rect。
    const SEGMENTS: usize = 64;
    let mut points = Vec::with_capacity(SEGMENTS + 1);
    for index in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * index as f32 / SEGMENTS as f32;
        points.push((0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()));
    }
    drawing("circle", "Circle", points)
}

fn square_drawing() -> ShapeDrawing {
    drawing(
        "square",
        "Square",
        vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)],
    )
}

fn triangle_drawing() -> ShapeDrawing {
    drawing(
        "triangle",
        "Triangle",
        vec![(0.5, 0.0), (1.0, 1.0), (0.0, 1.0), (0.5, 0.0)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_ids_are_stable_and_unique() {
        let ids: Vec<&str> = BuiltinShape::ALL
            .into_iter()
            .map(BuiltinShape::id)
            .collect();
        assert_eq!(ids, vec!["circle", "square", "triangle"]);
    }

    #[test]
    fn test_builtin_three_are_valid_closed_shapes() {
        for drawing in builtin_drawings() {
            drawing.validate().expect("内置形状必须合法");
            assert_eq!(drawing.strokes.len(), 1);
            let stroke = &drawing.strokes[0];
            assert!(stroke.rect.is_some(), "内置形状应有单位框");
            assert!(stroke.anchors.len() >= 3, "内置形状至少三个点");
            let first = stroke.anchors.first().expect("首点");
            let last = stroke.anchors.last().expect("尾点");
            assert!(
                (first.pos.0 - last.pos.0).abs() <= 1e-4
                    && (first.pos.1 - last.pos.1).abs() <= 1e-4,
                "内置形状必须首尾闭合：{}",
                drawing.id
            );
        }
    }

    #[test]
    fn test_builtin_names_and_roles_match_strokes() {
        for drawing in builtin_drawings() {
            assert!(!drawing.name.is_empty());
            assert_eq!(drawing.roles.len(), drawing.strokes.len());
            assert!(drawing.areas.is_empty());
        }
    }
}
