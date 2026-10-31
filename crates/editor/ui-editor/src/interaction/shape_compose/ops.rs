//! DRAW-023..028：形状参数/组合操作（从 `shape_compose.rs` 拆出）
//!
//! 已 Bake 的 DrawnShape 走 `ShapeStore` 模型；041C 起 027/028 也支持
//! Bake 前 pending 入口（组合结果仍是 pending，✓ 才落文档）。

use lumino_core::shape::{
    BetweenParams, BetweenRole, BetweenSet, CopyMode, CopyPathParams, ShapeKind, ShapeModel,
    ShapeOp, StraightenParams, Sym, SymAxis, SymParams, VelocityEnv,
};
use lumino_editor_state::shape_ops::{
    SymReject, between_blend, between_graph_at, copy_along_apply, cut_loops,
    symmetric_shape_with_reason,
};
use lumino_editor_state::{ComposedKind, DrawnShapeSource, ShapeNote};

use crate::Editor;

use super::{ShapeItem, source_from_model, sym_reject_message};

impl Editor {
    /// 027 默认路径：沿 x 复制 3 份（Bake 前/后同口径）
    pub(crate) fn default_copy_along_params(model: &ShapeModel) -> Option<CopyPathParams> {
        let bounds = model.geometry.bounds()?;
        let width = (bounds.2 - bounds.0).max(1.0);
        Some(CopyPathParams {
            pts: vec![[width * 2.0, 0.0]],
            mode: CopyMode::Count { n: 3 },
            grp: Some([bounds.0, bounds.1]),
            ..CopyPathParams::default()
        })
    }

    /// 028 默认渐强包络（Bake 前/后同口径）
    pub(crate) fn rising_velocity_env() -> VelocityEnv {
        VelocityEnv::new(vec![[0.0, 30.0], [1.0, 120.0]])
    }

    /// 023：切换对称（竖轴/横轴）。再次调用同一轴 = 关闭。
    ///
    /// 关闭不是失败，只有「拒绝」才返回 `Err`（附可展示原因，DRAW-043）。
    pub fn toggle_symmetry(&mut self, vertical: bool) -> Result<(), SymReject> {
        let result = self.toggle_symmetry_inner(vertical);
        if let Err(reason) = result {
            tracing::warn!(
                reason = sym_reject_message(reason),
                "半边对称被拒绝（DRAW-023）"
            );
        }
        result
    }

    fn toggle_symmetry_inner(&mut self, vertical: bool) -> Result<(), SymReject> {
        let Some((drawn_id, model_id, mut model)) = self.selected_model_for_edit() else {
            return Err(SymReject::NoSelection);
        };
        let params = SymParams {
            mode: Sym::Mirror,
            axis: if vertical {
                SymAxis::Vertical {
                    x: model
                        .geometry
                        .bounds()
                        .map(|b| (b.0 + b.2) / 2.0)
                        .unwrap_or(0.0),
                }
            } else {
                SymAxis::Horizontal {
                    y: model
                        .geometry
                        .bounds()
                        .map(|b| (b.1 + b.3) / 2.0)
                        .unwrap_or(0.0),
                }
            },
            source: 0,
        };
        let already = model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_)));
        model.ops.retain(|op| !matches!(op, ShapeOp::Symmetry(_)));
        if !already {
            symmetric_shape_with_reason(&model, &params)?;
            model.ops.push(ShapeOp::Symmetry(params));
        }
        let updated = self.edit_shape(model_id, model);
        self.refresh_drawn_source(drawn_id, model_id);
        if updated {
            Ok(())
        } else {
            Err(SymReject::EditFailed)
        }
    }

    /// 024：显式开启/关闭扶正
    pub fn set_straighten(&mut self, on: bool) -> bool {
        let Some((drawn_id, model_id, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        model.ops.retain(|op| !matches!(op, ShapeOp::Straighten(_)));
        if on {
            if !matches!(
                model.kind,
                ShapeKind::Freehand | ShapeKind::Polyline | ShapeKind::Line
            ) {
                return false;
            }
            model
                .ops
                .push(ShapeOp::Straighten(StraightenParams { level: 50 }));
        }
        let ok = self.edit_shape(model_id, model);
        self.refresh_drawn_source(drawn_id, model_id);
        ok
    }

    /// 025：选中一个图形，沿 bbox 中心竖切成两半
    pub fn slice_selected_vertical(&mut self) -> bool {
        let Some((_, _, model)) = self.selected_model_for_edit() else {
            return false;
        };
        let Some((x0, y0, x1, y1)) = model.geometry.bounds() else {
            return false;
        };
        let cut = (x0 + x1) / 2.0;
        let ring = geometry_ring(&model);
        let Some((left, right)) = cut_loops(&[ring], (cut, y0 - 1.0), (cut, y1 + 1.0)) else {
            return false;
        };
        let selected = self.selected_drawn_snapshot();
        let notes = selected[0].notes.clone();
        let (left_notes, right_notes) = split_notes_vertical(&notes, cut);
        let items = vec![
            ShapeItem {
                source: DrawnShapeSource::Polyline {
                    points: left.into_iter().flatten().collect(),
                },
                notes: left_notes,
                model: None,
            },
            ShapeItem {
                source: DrawnShapeSource::Polyline {
                    points: right.into_iter().flatten().collect(),
                },
                notes: right_notes,
                model: None,
            },
        ];
        self.replace_selected_with_many(items)
    }

    /// 026：两个选中图形生成 5 个 Between 步
    pub fn create_between_selected(&mut self) -> bool {
        let selected = self.selected_drawn_snapshot();
        if selected.len() != 2 {
            return false;
        }
        let Some(first_id) = self.ensure_model_for_drawn(selected[0].id) else {
            return false;
        };
        let Some(last_id) = self.ensure_model_for_drawn(selected[1].id) else {
            return false;
        };
        let Some(mut first) = self.editor_state.shape_store.model(first_id).cloned() else {
            return false;
        };
        let Some(mut last) = self.editor_state.shape_store.model(last_id).cloned() else {
            return false;
        };
        let set = BetweenSet {
            steps: 5,
            ..BetweenSet::default()
        };
        let group_id = 1;
        first.ops.push(ShapeOp::Between(BetweenParams {
            id: group_id,
            role: BetweenRole::First,
            at: Some(0.0),
            sig: None,
            push: None,
            vel: None,
            set: Some(set.clone()),
        }));
        last.ops.push(ShapeOp::Between(BetweenParams {
            id: group_id,
            role: BetweenRole::Last,
            at: Some(1.0),
            sig: None,
            push: None,
            vel: None,
            set: None,
        }));
        let track = selected[0].track;
        let mut items = vec![
            ShapeItem {
                source: source_from_model(&first),
                notes: selected[0].notes.clone(),
                model: Some(first.clone()),
            },
            ShapeItem {
                source: source_from_model(&last),
                notes: selected[1].notes.clone(),
                model: Some(last.clone()),
            },
        ];
        for index in 1..=set.steps {
            let at = f32::from(index) / f32::from(set.steps + 1);
            let model = between_blend(&first, &last, between_graph_at(&set, at), set.rev);
            let notes = self.shape_notes_of(&model, track);
            items.insert(
                index as usize,
                ShapeItem {
                    source: source_from_model(&model),
                    notes,
                    model: Some(model),
                },
            );
        }
        self.replace_selected_with_many(items)
    }

    /// 027：选中一个图形，沿默认路径复制 3 份
    ///
    /// 副本是 `ShapeOp::CopyAlong` 这个 fx 页的产物：op 必须写回模型，
    /// 否则后续 `edit_shape` 按模型重生成时会把副本抹掉（DRAW-027 验收）。
    pub fn copy_along_selected(&mut self) -> bool {
        if self.pending_single_selection() {
            return self.compose_pending(ComposedKind::CopyAlong);
        }
        let Some((drawn_id, _, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        let Some(params) = Self::default_copy_along_params(&model) else {
            return false;
        };
        let Some(selected) = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .find(|shape| shape.id == drawn_id)
            .cloned()
        else {
            return false;
        };
        let ppq = f64::from(self.editor_state.view.ppq);
        let notes = copy_along_apply(&selected.notes, &params, ppq);
        model.ops.push(ShapeOp::CopyAlong(params));
        // 替换旧模型：清 id 让 ShapeStore 分配新 id，避免与刚标记删除的旧条目同 id
        model.id = 0;
        self.replace_selected_with_many(vec![ShapeItem {
            source: selected.source,
            notes,
            model: Some(model),
        }])
    }

    /// 028：对选中图形应用渐强力度包络
    pub fn velocity_up_selected(&mut self) -> bool {
        if self.pending_single_selection() {
            return self.compose_pending(ComposedKind::Velocity);
        }
        let Some((drawn_id, model_id, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        model
            .ops
            .retain(|op| !matches!(op, ShapeOp::VelocityEnv(_)));
        model
            .ops
            .push(ShapeOp::VelocityEnv(Self::rising_velocity_env()));
        let ok = self.edit_shape(model_id, model);
        self.refresh_drawn_source(drawn_id, model_id);
        ok
    }
}

fn geometry_ring(model: &ShapeModel) -> Vec<(f32, f32)> {
    let mut ring = model
        .geometry
        .anchors
        .iter()
        .map(|anchor| anchor.pos)
        .collect::<Vec<_>>();
    if ring.is_empty()
        && let Some(rect) = model.geometry.rect
    {
        ring = vec![
            (rect.tick_lo, rect.key_lo),
            (rect.tick_hi, rect.key_lo),
            (rect.tick_hi, rect.key_hi),
            (rect.tick_lo, rect.key_hi),
        ];
    }
    if let Some(first) = ring.first().copied()
        && ring.last().copied() != Some(first)
    {
        ring.push(first);
    }
    ring
}

fn split_notes_vertical(notes: &[ShapeNote], cut: f32) -> (Vec<ShapeNote>, Vec<ShapeNote>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for note in notes {
        let start = note.tick;
        let end = note.tick + note.length.max(0.0);
        if end <= cut {
            left.push(*note);
        } else if start >= cut {
            right.push(*note);
        } else {
            if cut > start {
                left.push(ShapeNote {
                    length: cut - start,
                    ..*note
                });
            }
            if end > cut {
                right.push(ShapeNote {
                    tick: cut,
                    length: end - cut,
                    ..*note
                });
            }
        }
    }
    (left, right)
}
