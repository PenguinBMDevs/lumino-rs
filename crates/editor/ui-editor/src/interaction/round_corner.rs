//! DRAW-044：角度柔化（逐角点拖半径）
//!
//! 只作用于 **✓ 前的待确认形状**（DEC-001）：参数化形状/曲线先提升为单支组合，
//! 再在模型上追加 `ShapeOp::Round`；拖拽只改 pending，不进文档历史。
//!
//! 半径以 **key** 为单位：`radius = hypot(dtick / (ppq·k), dkey)`，
//! 与 `shape_ops::round` 的圆弧求解同口径。
//!
//! DEBT-18：**Ctrl+拖**改的是弧形状（`ShapeOp::RoundShape`，纵向鼓度），
//! 普通拖仍是半径；两者共用同一份切点，几何由 `rounded_points` 统一消费。

use lumino_core::shape::{RoundParams, RoundShapeParams, ShapeModel, ShapeOp};
use lumino_ui_core::geom::Point;

use crate::Editor;

/// 形状拖拽的默认半径（key）：Ctrl+拖一个还没有圆角的角点时先给一个可见半径，
/// 否则形状参数没有几何载体（切点距离为 0）。
const DEFAULT_SHAPE_RADIUS_KEYS: f32 = 2.0;

/// 本次拖拽调整的维度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RoundDrag {
    /// 普通拖拽：调半径
    #[default]
    Radius,
    /// Ctrl+拖拽：调弧形状（鼓度）
    Shape,
}

/// 柔化模式状态
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoundCornerMode {
    /// `pending_composed` 下标
    pub composed: usize,
    /// 当前拖拽/选中的角点（锚点下标）
    pub corner: Option<usize>,
    /// 拖拽前的整份 pending 组合快照（一次撤销）
    pub before: Option<Vec<lumino_editor_state::PendingComposed>>,
    /// 拖拽起点（逻辑 tick/key），半径按到角点的距离算
    pub origin: Option<(f32, f32)>,
    /// 当前光标（逻辑 tick/key；拖拽中用于画半径提示线）
    pub cursor: Option<(f32, f32)>,
    /// 是否已经改过半径（离开模式时决定要不要还原提升前的曲线）
    pub changed: bool,
    /// 本次拖拽在调什么（半径 / 形状）
    pub drag: RoundDrag,
    /// 形状拖拽的基准值（按下时该角点的形状）
    pub shape_base: f32,
}

impl Editor {
    /// 进入柔化模式；已 Bake 形状拒绝（DEC-001）
    pub fn begin_round_corners(&mut self) -> bool {
        if self.editor_state.shape_select.selection_len() != 1 {
            return false;
        }
        let pending = self
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .and_then(|shape| shape.pending);
        let index = match pending {
            Some(lumino_editor_state::PendingShapeRef::Composed(index)) => index,
            Some(
                lumino_editor_state::PendingShapeRef::Shape(_)
                | lumino_editor_state::PendingShapeRef::CurvePath(_),
            ) => {
                // 参数化形状/曲线先提升成单支组合，再按模型编辑
                if !self.promote_pending_for_edit() {
                    return false;
                }
                let Some(shape) = self.editor_state.shape_select.selected_shapes().next() else {
                    return false;
                };
                let Some(lumino_editor_state::PendingShapeRef::Composed(index)) = shape.pending
                else {
                    return false;
                };
                index
            }
            // 画刷笔画 / 已 Bake：不提供柔化入口
            _ => return false,
        };
        let Some(part) = pending_part(self, index) else {
            return false;
        };
        if part.model.geometry.anchors.len() < 3 {
            return false;
        }
        self.shape_tools.round_corner = Some(RoundCornerMode {
            composed: index,
            corner: None,
            before: None,
            origin: None,
            cursor: None,
            changed: false,
            drag: RoundDrag::Radius,
            shape_base: 0.0,
        });
        true
    }

    /// 是否处于柔化模式
    pub fn round_corners_active(&self) -> bool {
        self.shape_tools.round_corner.is_some()
    }

    /// 退出柔化模式
    pub fn end_round_corners(&mut self) {
        self.shape_tools.round_corner.take();
    }

    /// 当前模式下的**可柔化角点**（锚点下标 + 屏幕位置）
    ///
    /// 只给能真正生效的角点画手柄：
    /// - 开放路径：跳过首尾端点（只有一个相邻段，无法圆角）；
    /// - 闭合路径：跳过与起点重合的收尾锚点（它和下标 0 是同一个角）。
    pub(crate) fn round_corner_points(&self) -> Vec<(usize, Point)> {
        let Some(mode) = &self.shape_tools.round_corner else {
            return Vec::new();
        };
        let Some(part) = pending_part(self, mode.composed) else {
            return Vec::new();
        };
        let anchors = &part.model.geometry.anchors;
        let Some(first) = anchors.first() else {
            return Vec::new();
        };
        let last = anchors.last().copied();
        let closed = anchors.len() >= 3
            && last.is_some_and(|last| {
                (last.pos.0 - first.pos.0).abs() < 1e-3 && (last.pos.1 - first.pos.1).abs() < 1e-3
            });
        let count = if closed {
            anchors.len() - 1
        } else {
            anchors.len()
        };
        // 与几何求解同一个 beats-per-key：共线/折返的角点求不出圆，不能画手柄
        let ppq = f64::from(self.editor_state.view.ppq).max(1.0);
        let scale = 1.0 / (ppq * f64::from(round_k(part, self)).max(1e-6));
        let to_pt = |p: (f32, f32)| (f64::from(p.0) * scale, f64::from(p.1));
        anchors
            .iter()
            .take(count)
            .enumerate()
            .filter(|(index, _)| closed || (*index > 0 && *index + 1 < count))
            .filter(|(index, _)| {
                let prev = anchors[(index + count - 1) % count].pos;
                let cur = anchors[*index].pos;
                let next = anchors[(*index + 1) % count].pos;
                lumino_editor_state::shape_ops::is_roundable(to_pt(prev), to_pt(cur), to_pt(next))
            })
            .map(|(index, anchor)| (index, self.line_pos_screen_pos(anchor.pos)))
            .collect()
    }

    /// 按下：命中角点即开始拖半径（容差 14px）；按住 Ctrl 则调弧形状
    pub(crate) fn round_corner_pressed(&mut self, pos: Point) -> bool {
        let Some(composed) = self
            .shape_tools
            .round_corner
            .as_ref()
            .map(|mode| mode.composed)
        else {
            return false;
        };
        let Some(part) = pending_part(self, composed) else {
            return false;
        };
        let Some(index) = self
            .round_corner_points()
            .into_iter()
            .map(|(index, point)| {
                let d = ((point.x - pos.x).powi(2) + (point.y - pos.y).powi(2)).sqrt();
                (d, index)
            })
            .filter(|(d, _)| *d <= 14.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(d, index)| (index, d))
            .map(|(index, _)| index)
        else {
            return false;
        };
        let anchor = part.model.geometry.anchors[index].pos;
        let shape_drag = self.ctrl_pressed();
        let shape_base = round_shape_of(&part.model, index).unwrap_or(0.0);
        let radius_missing = round_radius_of(&part.model, index).unwrap_or(0.0) <= 0.0;
        // 拖拽前的整份 pending 组合快照（一次撤销）——必须早于下面的默认半径写入
        let snapshot = self.editor_state.pending_composed.clone();
        // Ctrl+拖一个还没有圆角的角点：先给一个默认半径，形状才有可见载体
        if shape_drag && radius_missing {
            let k = view_beats_per_key(self);
            let mut model = part.model.clone();
            set_radius(
                &mut model,
                index,
                DEFAULT_SHAPE_RADIUS_KEYS,
                k,
                self.editor_state.view.ppq,
            );
            update_pending_part(self, composed, model);
        }
        let Some(mode) = &mut self.shape_tools.round_corner else {
            return false;
        };
        mode.corner = Some(index);
        mode.before = Some(snapshot);
        mode.origin = Some(anchor);
        mode.drag = if shape_drag {
            RoundDrag::Shape
        } else {
            RoundDrag::Radius
        };
        mode.shape_base = shape_base;
        true
    }

    /// 拖动：普通拖拽 = 半径（光标到角点的距离）；Ctrl+拖拽 = 弧形状（纵向鼓度）
    pub(crate) fn round_corner_moved(&mut self, pos: Point) -> bool {
        let Some(mode) = &self.shape_tools.round_corner else {
            return false;
        };
        let (Some(index), Some(origin)) = (mode.corner, mode.origin) else {
            return false;
        };
        let composed = mode.composed;
        let drag = mode.drag;
        let shape_base = mode.shape_base;
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        if let Some(mode) = &mut self.shape_tools.round_corner {
            mode.cursor = Some((tick, key));
            mode.changed = true;
        }
        let Some(part) = pending_part(self, composed) else {
            return false;
        };
        let mut model = part.model.clone();
        // 与几何求解同一个 beats-per-key：半径（key）与屏幕距离一一对应
        let k = view_beats_per_key(self);
        let ppq = self.editor_state.view.ppq;
        match drag {
            RoundDrag::Radius => {
                let ppq_f = f32::from(ppq).max(1.0);
                let dx = (tick - origin.0) / (ppq_f * k);
                let dy = key - origin.1;
                let radius = (dx * dx + dy * dy).sqrt().clamp(0.0, 64.0);
                set_radius(&mut model, index, radius, k, ppq);
            }
            RoundDrag::Shape => {
                // 纵向 1 个 key = 1 个单位；按半径归一化，半径越大越不敏感
                let radius = round_radius_of(&model, index)
                    .unwrap_or(DEFAULT_SHAPE_RADIUS_KEYS)
                    .max(0.5);
                let shape = RoundShapeParams::clamp_shape(shape_base + (key - origin.1) / radius);
                set_shape(&mut model, index, shape, k, ppq);
            }
        }
        update_pending_part(self, composed, model);
        true
    }

    /// 松手：一次拖拽 = 一次 pending 撤销
    pub(crate) fn round_corner_released(&mut self) -> bool {
        let Some(mode) = &mut self.shape_tools.round_corner else {
            return false;
        };
        let Some(before) = mode.before.take() else {
            return false;
        };
        mode.origin = None;
        mode.cursor = None;
        self.shape_tools.push_pending_edit_snapshot(before);
        true
    }
}

/// 取待确认组合的第一支（自由函数：T-30 slice 6）。
fn pending_part(editor: &Editor, index: usize) -> Option<&lumino_editor_state::ComposedPart> {
    editor
        .editor_state
        .pending_composed
        .get(index)?
        .results
        .first()
}

/// 用新模型替换第一支并重生成预览音符（自由函数：T-30 slice 6）。
fn update_pending_part(editor: &mut Editor, index: usize, mut model: ShapeModel) {
    let track = model.track as usize;
    let notes = editor.part_notes_for_edit(&model, track);
    if let Some(part) = editor
        .editor_state
        .pending_composed
        .get_mut(index)
        .and_then(|composed| composed.results.first_mut())
    {
        model.id = 0;
        part.model = model;
        part.notes = notes;
    }
    editor.refresh_pending_mirror_geometry();
}

/// 在模型上设置某角点的圆角半径（无 Round 算子时追加）
///
/// 首次创建时冻结当前视图的 beats-per-key：x（tick）与 key 必须同尺度，
/// 圆弧在屏幕上才是正圆；不冻结会让短 tick 段的圆角被夹到几乎看不见。
fn set_radius(model: &mut ShapeModel, index: usize, radius: f32, view_k: f32, view_ppq: u16) {
    if !model.ops.iter().any(|op| matches!(op, ShapeOp::Round(_))) {
        model.ops.push(ShapeOp::Round(RoundParams {
            corners: Vec::new(),
            k: view_k,
            ppq: view_ppq,
        }));
    }
    for op in &mut model.ops {
        if let ShapeOp::Round(params) = op {
            if !(params.k.is_finite() && params.k > 0.0) {
                params.k = view_k;
            }
            if params.ppq == 0 {
                params.ppq = view_ppq;
            }
            params.set_radius(index, radius);
        }
    }
}

/// 在模型上设置某角点的弧形状（无 RoundShape 算子时追加）
///
/// 与 `set_radius` 同口径：确保 `Round` 存在并冻结 beats-per-key / PPQ；
/// 形状回到 0 时移除条目，全部为零则连算子一并移除（算子链保持干净）。
fn set_shape(model: &mut ShapeModel, index: usize, shape: f32, view_k: f32, view_ppq: u16) {
    if !model.ops.iter().any(|op| matches!(op, ShapeOp::Round(_))) {
        model.ops.push(ShapeOp::Round(RoundParams {
            corners: Vec::new(),
            k: view_k,
            ppq: view_ppq,
        }));
    }
    for op in &mut model.ops {
        if let ShapeOp::Round(params) = op {
            if !(params.k.is_finite() && params.k > 0.0) {
                params.k = view_k;
            }
            if params.ppq == 0 {
                params.ppq = view_ppq;
            }
        }
    }
    let mut params = model
        .ops
        .iter()
        .find_map(|op| match op {
            ShapeOp::RoundShape(params) => Some(params.clone()),
            _ => None,
        })
        .unwrap_or_default();
    params.set_shape(index, shape);
    model.ops.retain(|op| !matches!(op, ShapeOp::RoundShape(_)));
    if !params.is_empty() {
        model.ops.push(ShapeOp::RoundShape(params));
    }
}

/// 某角点当前登记的圆角半径（未登记 = `None`）
fn round_radius_of(model: &ShapeModel, index: usize) -> Option<f32> {
    model.ops.iter().find_map(|op| match op {
        ShapeOp::Round(params) => params.radius_of(index),
        _ => None,
    })
}

/// 某角点当前登记的弧形状（未登记 = `None`，即正圆弧）
fn round_shape_of(model: &ShapeModel, index: usize) -> Option<f32> {
    model.ops.iter().find_map(|op| match op {
        ShapeOp::RoundShape(params) => params.shape_of(index),
        _ => None,
    })
}

/// 当前视图的 beats-per-key：1 个 key 与多少拍在屏幕上等长
fn view_beats_per_key(editor: &Editor) -> f32 {
    let view = &editor.editor_state.view;
    let ppq = f32::from(view.ppq).max(1.0);
    let zoom_x = view.zoom_x.max(1e-6);
    let k = view.zoom_y / (ppq * zoom_x);
    if k.is_finite() && k > 0.0 { k } else { 1.0 }
}

/// 柔化几何实际使用的 beats-per-key：已冻结则用之，否则回退到当前视图
fn round_k(part: &lumino_editor_state::ComposedPart, editor: &Editor) -> f32 {
    part.model
        .ops
        .iter()
        .find_map(|op| match op {
            ShapeOp::Round(params) => Some(params.effective_k(part.model.k)),
            _ => None,
        })
        .unwrap_or_else(|| view_beats_per_key(editor))
}

#[cfg(test)]
mod tests;
