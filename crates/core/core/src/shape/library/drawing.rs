//! 形状库中的单个可复用图形（DRAW-033A）
//!
//! 库内坐标统一为 0..=1 的单位框；落到工程时由 [`ShapeDrawing::to_model`]
//! 映射到目标 `ShapeRect`。这与 `.lmshapes` 的 tick/key 坐标解耦，
//! 因此同一图形可以在不同 PPQ / 不同大小的目标框里复用。
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::{
    AreaSpot, DrawingParams, ShapeAnchor, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeRect,
    ShapeStyle, Stroke, StrokeRole,
};

/// 库内名称/ID 的最大字符数
pub const MAX_NAME_CHARS: usize = 128;
/// 库内单图形最大区域点数
pub const MAX_AREAS: usize = 1024;

/// 一个可复用图形（抽屉里的一幅画）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeDrawing {
    /// 稳定 ID（用于文件名；只允许安全字符，加载时可由文件名补全）
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 多条 stroke；坐标单位框 0..=1
    pub strokes: Vec<Stroke>,
    /// 归一化区域着色点
    pub areas: Vec<AreaSpot>,
    /// 与 strokes 等长的角色；空 = 全部默认
    pub roles: Vec<StrokeRole>,
}

impl ShapeDrawing {
    /// 构造空图形；调用方负责填 strokes
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            strokes: Vec::new(),
            areas: Vec::new(),
            roles: Vec::new(),
        }
    }

    /// 校验库数据；错误可读、不 panic
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(CoreError::InvalidArgument("形状 id 不能为空".into()));
        }
        if self.id.chars().count() > MAX_NAME_CHARS {
            return Err(CoreError::InvalidArgument(format!(
                "形状 id 过长（最多 {MAX_NAME_CHARS} 字符）"
            )));
        }
        if self.name.trim().is_empty() {
            return Err(CoreError::InvalidArgument("形状名称不能为空".into()));
        }
        if self.name.chars().count() > MAX_NAME_CHARS {
            return Err(CoreError::InvalidArgument(format!(
                "形状名称过长（最多 {MAX_NAME_CHARS} 字符）"
            )));
        }
        if self.areas.len() > MAX_AREAS {
            return Err(CoreError::InvalidArgument(format!(
                "区域点数超过上限（最多 {MAX_AREAS}，实际 {}）",
                self.areas.len()
            )));
        }
        for area in &self.areas {
            area.validate()?;
        }
        DrawingParams {
            strokes: self.strokes.clone(),
            roles: self.roles.clone(),
        }
        .validate()?;
        Ok(())
    }

    /// 所有 stroke 的联合外接框（库内单位框）
    pub fn bounds(&self) -> Option<ShapeRect> {
        DrawingParams {
            strokes: self.strokes.clone(),
            roles: self.roles.clone(),
        }
        .bbox()
    }

    /// 转换为工程形状：`Custom + ShapeOp::Drawing`，不新增 ShapeModel 字段。
    ///
    /// `rect` 是目标工程框；库内 0..=1 坐标按源包围盒线性映射到该框。
    pub fn to_model(&self, track: u16, k: f32, rect: ShapeRect) -> Result<ShapeModel> {
        self.validate()?;
        if !rect.is_finite() || !rect.is_normalized() {
            return Err(CoreError::InvalidArgument(
                "目标 ShapeRect 必须是有限且规范化的框".into(),
            ));
        }
        let source = self
            .bounds()
            .ok_or_else(|| CoreError::InvalidArgument("形状没有可用的几何包围盒".into()))?;
        let params = DrawingParams {
            strokes: self
                .strokes
                .iter()
                .map(|stroke| scale_stroke(stroke, source, rect))
                .collect(),
            roles: if self.roles.is_empty() {
                vec![StrokeRole::default(); self.strokes.len()]
            } else {
                self.roles.clone()
            },
        };
        let mut model = ShapeModel::new(0, track, ShapeKind::Custom, k);
        model.name = self.name.clone();
        // 单条闭合 stroke（内置三形）整体可视为闭合，供 033C 区域着色识别；
        // 多笔画整体不是一个闭合区域。
        let closed = params.strokes.len() == 1 && params.stroke_closed(0);
        model.geometry = ShapeGeometry {
            anchors: Vec::new(),
            rect: Some(rect),
            closed,
        };
        model.style = ShapeStyle {
            areas: self.areas.clone(),
            ..ShapeStyle::default()
        };
        model.ops.push(ShapeOp::Drawing(params));
        model.validate()?;
        Ok(model)
    }
}

fn scale_stroke(stroke: &Stroke, source: ShapeRect, target: ShapeRect) -> Stroke {
    let (source_w, source_h) = source.size();
    let (target_w, target_h) = target.size();
    let anchors = stroke
        .anchors
        .iter()
        .map(|anchor| {
            scale_anchor(
                anchor, source, source_w, source_h, target, target_w, target_h,
            )
        })
        .collect();
    let rect = stroke
        .rect
        .map(|rect| scale_rect(rect, source, source_w, source_h, target));
    Stroke {
        kind: stroke.kind,
        anchors,
        rect,
    }
}

fn scale_anchor(
    anchor: &ShapeAnchor,
    source: ShapeRect,
    source_w: f32,
    source_h: f32,
    target: ShapeRect,
    target_w: f32,
    target_h: f32,
) -> ShapeAnchor {
    let mut out = *anchor;
    out.pos.0 = map_axis(
        anchor.pos.0,
        source.tick_lo,
        source_w,
        target.tick_lo,
        target_w,
    );
    out.pos.1 = map_axis(
        anchor.pos.1,
        source.key_lo,
        source_h,
        target.key_lo,
        target_h,
    );
    out.in_handle.0 = map_handle(anchor.in_handle.0, source_w, target_w);
    out.in_handle.1 = map_handle(anchor.in_handle.1, source_h, target_h);
    out.out_handle.0 = map_handle(anchor.out_handle.0, source_w, target_w);
    out.out_handle.1 = map_handle(anchor.out_handle.1, source_h, target_h);
    out
}

fn scale_rect(
    rect: ShapeRect,
    source: ShapeRect,
    source_w: f32,
    source_h: f32,
    target: ShapeRect,
) -> ShapeRect {
    let (target_w, target_h) = target.size();
    ShapeRect {
        tick_lo: map_axis(
            rect.tick_lo,
            source.tick_lo,
            source_w,
            target.tick_lo,
            target_w,
        ),
        key_lo: map_axis(
            rect.key_lo,
            source.key_lo,
            source_h,
            target.key_lo,
            target_h,
        ),
        tick_hi: map_axis(
            rect.tick_hi,
            source.tick_lo,
            source_w,
            target.tick_lo,
            target_w,
        ),
        key_hi: map_axis(
            rect.key_hi,
            source.key_lo,
            source_h,
            target.key_lo,
            target_h,
        ),
    }
}

fn map_axis(value: f32, source_lo: f32, source_span: f32, target_lo: f32, target_span: f32) -> f32 {
    if source_span.abs() <= 1e-6 {
        target_lo + target_span * 0.5
    } else {
        target_lo + (value - source_lo) / source_span * target_span
    }
}

fn map_handle(value: f32, source_span: f32, target_span: f32) -> f32 {
    if source_span.abs() <= 1e-6 {
        0.0
    } else {
        value / source_span * target_span
    }
}
