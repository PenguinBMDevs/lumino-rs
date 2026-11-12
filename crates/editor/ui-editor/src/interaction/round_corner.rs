//! DRAW-044：角度柔化（逐角点拖半径）
//!
//! 只作用于 **✓ 前的待确认形状**（DEC-001）：参数化形状/曲线先提升为单支组合，
//! 再在模型上追加 `ShapeOp::Round`；拖拽只改 pending，不进文档历史。
//!
//! 半径以 **key** 为单位：`radius = hypot(dtick / (ppq·k), dkey)`，
//! 与 `shape_ops::round` 的圆弧求解同口径。

use iced_core::Point;
use lumino_core::shape::{RoundParams, ShapeModel, ShapeOp};

use crate::Editor;

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
        let Some(part) = self.pending_part(index) else {
            return false;
        };
        if part.model.geometry.anchors.len() < 3 {
            return false;
        }
        self.round_corner = Some(RoundCornerMode {
            composed: index,
            corner: None,
            before: None,
            origin: None,
            cursor: None,
            changed: false,
        });
        self.grid_cache.clear();
        true
    }

    /// 是否处于柔化模式
    pub fn round_corners_active(&self) -> bool {
        self.round_corner.is_some()
    }

    /// 退出柔化模式
    pub fn end_round_corners(&mut self) {
        if self.round_corner.take().is_some() {
            self.grid_cache.clear();
        }
    }

    /// 当前模式下的**可柔化角点**（锚点下标 + 屏幕位置）
    ///
    /// 只给能真正生效的角点画手柄：
    /// - 开放路径：跳过首尾端点（只有一个相邻段，无法圆角）；
    /// - 闭合路径：跳过与起点重合的收尾锚点（它和下标 0 是同一个角）。
    pub(crate) fn round_corner_points(&self) -> Vec<(usize, Point)> {
        let Some(mode) = &self.round_corner else {
            return Vec::new();
        };
        let Some(part) = self.pending_part(mode.composed) else {
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

    /// 按下：命中角点即开始拖半径（容差 14px）
    pub(crate) fn round_corner_pressed(&mut self, pos: Point) -> bool {
        let Some(mode) = &self.round_corner else {
            return false;
        };
        let Some(part) = self.pending_part(mode.composed) else {
            return false;
        };
        let Some((index, _)) = self
            .round_corner_points()
            .into_iter()
            .map(|(index, point)| {
                let d = ((point.x - pos.x).powi(2) + (point.y - pos.y).powi(2)).sqrt();
                (d, index)
            })
            .filter(|(d, _)| *d <= 14.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(d, index)| (index, d))
        else {
            return false;
        };
        let anchor = part.model.geometry.anchors[index].pos;
        let snapshot = self.editor_state.pending_composed.clone();
        let Some(mode) = &mut self.round_corner else {
            return false;
        };
        mode.corner = Some(index);
        mode.before = Some(snapshot);
        mode.origin = Some(anchor);
        self.grid_cache.clear();
        true
    }

    /// 拖动：半径 = 光标到角点的距离（key 单位）
    pub(crate) fn round_corner_moved(&mut self, pos: Point) -> bool {
        let Some(mode) = &self.round_corner else {
            return false;
        };
        let (Some(index), Some(origin)) = (mode.corner, mode.origin) else {
            return false;
        };
        let composed = mode.composed;
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        let ppq = f32::from(self.editor_state.view.ppq).max(1.0);
        // 与几何求解同一个 beats-per-key：1 个 key 与 k 拍在屏幕上等长，
        // 拖动的屏幕距离才能和半径（key）一一对应
        let k = view_beats_per_key(self);
        let dx = (tick - origin.0) / (ppq * k);
        let dy = key - origin.1;
        let radius = (dx * dx + dy * dy).sqrt().clamp(0.0, 64.0);
        if let Some(mode) = &mut self.round_corner {
            mode.cursor = Some((tick, key));
            mode.changed = true;
        }
        let Some(part) = self.pending_part(composed) else {
            return false;
        };
        let mut model = part.model.clone();
        set_radius(&mut model, index, radius, k, self.editor_state.view.ppq);
        self.update_pending_part(composed, model);
        self.grid_cache.clear();
        true
    }

    /// 松手：一次拖拽 = 一次 pending 撤销
    pub(crate) fn round_corner_released(&mut self) -> bool {
        let Some(mode) = &mut self.round_corner else {
            return false;
        };
        let Some(before) = mode.before.take() else {
            return false;
        };
        mode.origin = None;
        mode.cursor = None;
        self.pending_edit_undo.push(before);
        while self.pending_edit_undo.len() > 64 {
            self.pending_edit_undo.remove(0);
        }
        self.pending_edit_redo.clear();
        self.pending_composed_redo.clear();
        self.grid_cache.clear();
        true
    }

    /// 取待确认组合的第一支（柔化只作用于单支模型）
    fn pending_part(&self, index: usize) -> Option<&lumino_editor_state::ComposedPart> {
        self.editor_state
            .pending_composed
            .get(index)?
            .results
            .first()
    }

    /// 用新模型替换第一支并重生成预览音符
    fn update_pending_part(&mut self, index: usize, mut model: ShapeModel) {
        let track = model.track as usize;
        let notes = self.part_notes_for_edit(&model, track);
        if let Some(part) = self
            .editor_state
            .pending_composed
            .get_mut(index)
            .and_then(|composed| composed.results.first_mut())
        {
            model.id = 0;
            part.model = model;
            part.notes = notes;
        }
        self.refresh_pending_mirror_geometry();
    }
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
