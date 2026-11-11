//! DRAW-023..028：形状参数/组合操作（从 `shape_compose.rs` 拆出）
//!
//! 已 Bake 的 DrawnShape 走 `ShapeStore` 模型；041C 起 027/028 也支持
//! Bake 前 pending 入口（组合结果仍是 pending，✓ 才落文档）。

use lumino_core::shape::{
    BetweenParams, BetweenRole, BetweenSet, CopyMode, CopyPathParams, ShapeKind, ShapeModel,
    ShapeOp, StraightenParams, Sym, SymAxis, SymParams, VelocityEnv,
};
use lumino_editor_state::ComposedKind;
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{
    SymReject, copy_along_apply, symmetric_shape_with_reason, sync_groups,
};

use crate::Editor;

use super::{ShapeItem, source_from_model, sym_reject_message};

/// Mould 失败原因（用于给用户精确提示）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouldReject {
    /// 不是恰好一个/两个待确认形状（含混合选区 / 已 Bake 参与）
    Selection,
    /// 单形状入口没有「刀」：切到鼠标工具前没有选中手画音符
    NoWalls,
    /// 刀的同一 key 行没有空隙可切：需要至少两条墙音符
    NoGap,
}

/// 027 一次沿路径复制的输入规格（参数面板接入前的代码级唯一来源）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CopyAlongSpec {
    /// 相邻副本左下角的间距（tick）
    pub step_ticks: f32,
    /// **总组数**（含原图）；5 = 原图 + 4 份副本
    pub groups: u32,
}

impl CopyAlongSpec {
    /// 默认：当前 PPQ 的半拍步长、共 5 组（原图 + 4 份副本）
    pub(crate) fn half_beat(ppq: u16) -> Self {
        Self {
            step_ticks: (f32::from(ppq) * 0.5).max(1.0),
            groups: 5,
        }
    }

    /// `CopyMode::Count` 的副本数（总组数至少 2）
    fn copies(self) -> Option<u32> {
        (self.groups >= 2).then(|| self.groups - 1)
    }
}

impl Editor {
    /// 027 默认路径：按 `spec` 的 tick 步长与总组数生成参数。
    ///
    /// `CopyMode::Count { n }` 的落点是 `i / (n + 1)`，所以路径总长取
    /// `step * groups`（`groups = n + 1`）才能让相邻副本正好相隔 `step`。
    pub(crate) fn default_copy_along_params(
        model: &ShapeModel,
        spec: CopyAlongSpec,
    ) -> Option<CopyPathParams> {
        let copies = spec.copies()?;
        let bounds = model.geometry.bounds()?;
        Some(CopyPathParams {
            pts: vec![[spec.step_ticks * spec.groups as f32, 0.0]],
            mode: CopyMode::Count { n: copies },
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

    /// 025：选中一个**待确认**图形，沿 bbox 中心竖切成两半
    ///
    /// DEC-001：已 Bake 形状不提供切割入口（音符是唯一权威）。
    pub fn slice_selected_vertical(&mut self) -> bool {
        if self.pending_single_selection() {
            return self.compose_pending(ComposedKind::Slice);
        }
        false
    }

    /// 025：饼干模——刀取「手画音符」，面团取当前待确认形状
    ///
    /// 两种入口：
    /// - **推荐**：先用鼠标工具选中若干手画音符（它们当刀），再切到鼠标工具、
    ///   选中**一个**待确认形状 → 空隙里就是内片；
    /// - 兼容旧口径：选中**两个**待确认形状，靠前者当刀（它的音符需要同一行
    ///   有 ≥2 段才能切出空隙）。
    ///
    /// 失败原因分开报，见 [`MouldReject`]。
    pub fn mould_selected(&mut self) -> Result<(), MouldReject> {
        let pending = self
            .editor_state
            .shape_select
            .selected_shapes()
            .filter(|shape| shape.pending.is_some())
            .count();
        if pending == 1 && self.pending_single_selection() {
            let Some(walls) = self.mould_walls.clone().filter(|w| !w.is_empty()) else {
                return Err(MouldReject::NoWalls);
            };
            return if self.compose_pending_mould(walls) {
                // 刀已消费：清掉虚线框，避免下一刀沿用旧快照
                self.mould_walls = None;
                self.grid_cache.clear();
                Ok(())
            } else {
                Err(MouldReject::NoGap)
            };
        }
        if pending == 2 && self.pending_selection_only() {
            return if self.compose_pending(ComposedKind::Mould) {
                Ok(())
            } else {
                Err(MouldReject::NoGap)
            };
        }
        Err(MouldReject::Selection)
    }

    /// 当前选中的手画音符（当前轨）→ 饼干模的刀
    pub(crate) fn selected_document_notes(&self) -> Vec<ShapeNote> {
        let track = self.editor_state.data.current_track;
        let notes = self.editor_state.data.track_notes(track);
        self.get_selected_indices()
            .into_iter()
            .filter_map(|index| notes.get(index))
            .map(|note| ShapeNote {
                track,
                tick: note.start_tick as f32,
                key: u16::from(note.key),
                length: note.end_tick.saturating_sub(note.start_tick) as f32,
                velocity: note.velocity,
                channel: note.channel,
            })
            .collect()
    }

    /// 026：两个选中图形生成 5 个 Between 步
    pub fn create_between_selected(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Between);
        }
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
        let group_id = self.next_between_group_id();
        first.ops.push(ShapeOp::Between(BetweenParams {
            id: group_id,
            role: BetweenRole::First,
            at: Some(0.0),
            sig: None,
            push: None,
            vel: None,
            set: Some(set),
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
        let mut shapes = vec![first, last];
        for warning in sync_groups(&mut shapes) {
            tracing::warn!(message = warning.message, "Between 组同步提示");
        }
        let last_index = shapes.len().saturating_sub(1);
        let mut items = Vec::with_capacity(shapes.len());
        for (index, model) in shapes.into_iter().enumerate() {
            let notes = if index == 0 {
                selected[0].notes.clone()
            } else if index == last_index {
                selected[1].notes.clone()
            } else {
                self.shape_notes_of(&model, track)
            };
            items.push(ShapeItem {
                source: source_from_model(&model),
                notes,
                model: Some(model),
            });
        }
        self.replace_selected_with_many(items)
    }

    /// 027：选中一个图形，沿默认路径复制 4 份（半拍步长，原图 + 4 = 5 个）
    ///
    /// 副本是 `ShapeOp::CopyAlong` 这个 fx 页的产物：op 必须写回模型，
    /// 否则后续 `edit_shape` 按模型重生成时会把副本抹掉（DRAW-027 验收）。
    pub fn copy_along_selected(&mut self) -> bool {
        let ppq = self.editor_state.view.ppq;
        self.copy_along_selected_with(f32::from(ppq) * 0.5, 5)
    }

    /// 027：按显式 **tick 步长** 与 **总组数** 复制（参数面板/脚本入口）。
    ///
    /// `groups` 是含原图的总组数：`groups = 5` → 4 份副本。
    pub fn copy_along_selected_with(&mut self, step_ticks: f32, groups: u32) -> bool {
        let spec = CopyAlongSpec {
            step_ticks: step_ticks.max(1.0),
            groups,
        };
        tracing::info!(
            step_ticks = spec.step_ticks,
            groups = spec.groups,
            "DRAW-027 沿路径复制"
        );
        if self.pending_single_selection() {
            return self.compose_pending_copy_along(spec);
        }
        let Some((drawn_id, _, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        let ppq = f64::from(self.editor_state.view.ppq);
        let Some(params) = Self::default_copy_along_params(&model, spec) else {
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

    /// 最近一次 027 复制的可展示摘要（总组数 · 步长 tick），供 Toast 校验。
    ///
    /// pending 组合优先，其次当前选中的已 Bake 模型；`Every` 模式无法从
    /// 路径长度反推步长，返回 `None`。
    pub fn copy_along_summary(&self) -> Option<String> {
        let (groups, step) = self
            .editor_state
            .pending_composed
            .last()
            .and_then(|item| {
                if item.kind != ComposedKind::CopyAlong {
                    return None;
                }
                item.results.iter().find_map(|part| {
                    part.model.ops.iter().rev().find_map(|op| match op {
                        ShapeOp::CopyAlong(params) => summarize_copy_params(params),
                        _ => None,
                    })
                })
            })
            .or_else(|| {
                let model_id = self
                    .editor_state
                    .shape_select
                    .selected_shapes()
                    .find_map(|shape| shape.model_id)?;
                let model = self.editor_state.shape_store.model(model_id)?;
                model.ops.iter().rev().find_map(|op| match op {
                    ShapeOp::CopyAlong(params) => summarize_copy_params(params),
                    _ => None,
                })
            })?;
        Some(format!("{groups} 组 · 步长 {step:.0} tick"))
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

fn summarize_copy_params(params: &CopyPathParams) -> Option<(u32, f32)> {
    let CopyMode::Count { n } = params.mode else {
        return None;
    };
    let groups = n + 1;
    let step = params.pts.first().map(|pt| pt[0].abs() / groups as f32)?;
    Some((groups, step))
}
