//! DRAW-023..028：形状参数/组合操作（从 `shape_compose.rs` 拆出）
//!
//! DEC-001：Bake 后形状对象只读。已 Bake 分支全部关闭；027/028/026 只保留
//! ✓ 前的 pending 入口（组合结果仍是 pending，✓ 才落文档）。

use lumino_core::shape::{
    CopyMode, CopyPathParams, ShapeModel, ShapeOp, Sym, SymAxis, SymParams, VelocityEnv,
};
use lumino_editor_state::ComposedKind;
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{SymReject, ring_of, symmetric_shape_with_reason};

use super::mould_knife::{MouldKnife, MouldKnifeInfo, MouldNotice, selected_pending_rings};

use crate::Editor;

/// Mould 失败原因（用于给用户精确提示）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouldReject {
    /// 面团不是恰好一个待确认形状（含混合选区 / 已 Bake 参与）
    Selection,
    /// 尚未设刀
    NoKnife,
    /// 当前选区里没有可以当作刀的内容
    NoKnifeSource,
    /// 刀已失效：音符或形状已改变
    KnifeStale,
    /// 刀与面团是同一个形状
    SameShape,
    /// 刀的同一 key 行没有空隙可切：需要至少两段墙
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
    /// DEC-001：只作用于 ✓ 前的 pending。普通 pending 曲线会先提升为单支组合；
    /// 已 Bake 形状没有 pending 目标，返回 `NoSelection`。
    pub fn toggle_symmetry(&mut self, vertical: bool) -> Result<(), SymReject> {
        let Some((index, promoted)) = self.pending_single_composed_for_ops() else {
            return Err(SymReject::NoSelection);
        };
        let Some(mut model) = self
            .editor_state
            .pending_composed
            .get(index)
            .and_then(|composed| composed.results.first())
            .map(|part| part.model.clone())
        else {
            return Err(SymReject::NoSelection);
        };
        // DRAW-023 的输入是**半边曲线**：最后一个锚点就是对称轴上的接缝。
        // 轴必须过该点；取 bbox 中心会把接缝投影到中心，整条曲线会错位。
        let seam = model.geometry.anchors.last().map(|anchor| anchor.pos);
        let axis = if vertical {
            SymAxis::Vertical {
                x: seam
                    .map(|(tick, _)| tick)
                    .or_else(|| model.geometry.bounds().map(|b| (b.0 + b.2) / 2.0))
                    .unwrap_or(0.0),
            }
        } else {
            SymAxis::Horizontal {
                y: seam
                    .map(|(_, key)| key)
                    .or_else(|| model.geometry.bounds().map(|b| (b.1 + b.3) / 2.0))
                    .unwrap_or(0.0),
            }
        };
        let params = SymParams {
            mode: Sym::Mirror,
            axis,
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
        // 已组合：快照只回退本次参数；刚提升：撤销组合会连提升一起回到 pending 矢量。
        if !promoted {
            let snapshot = self.editor_state.pending_composed.clone();
            self.pending_edit_undo.push(snapshot);
            while self.pending_edit_undo.len() > 64 {
                self.pending_edit_undo.remove(0);
            }
            self.pending_edit_redo.clear();
            self.pending_composed_redo.clear();
        }
        self.update_pending_composed_model(index, model);
        Ok(())
    }

    /// 024：显式开启/关闭扶正
    ///
    /// DEC-001：已 Bake 形状只读；pending 扶正入口尚未实现，因此当前一律拒绝。
    pub fn set_straighten(&mut self, _on: bool) -> bool {
        false
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

    /// 025：饼干模——刀由「以选中内容为刀」显式设置，面团取当前待确认形状
    ///
    /// 刀在切完即清除；切口由 `PendingComposed::mould_cuts` 保留到 ✓ / ×。
    /// 失败原因分开报，见 [`MouldReject`]。
    pub fn mould_selected(&mut self) -> Result<(), MouldReject> {
        let pending = self
            .editor_state
            .shape_select
            .selected_shapes()
            .filter(|shape| shape.pending.is_some())
            .count();
        if pending != 1 || !self.pending_single_selection() {
            return Err(MouldReject::Selection);
        }
        let Some(knife) = self.mould_knife.clone() else {
            return Err(MouldReject::NoKnife);
        };
        if !knife.is_current(self) {
            self.mould_knife = None;
            return Err(MouldReject::KnifeStale);
        }
        if self.dough_matches_knife(&knife) {
            return Err(MouldReject::SameShape);
        }
        if self.compose_pending_mould(knife) {
            // 刀已消费：清掉虚线框，避免下一刀沿用旧快照
            self.mould_knife = None;
            self.grid_cache.clear();
            Ok(())
        } else {
            Err(MouldReject::NoGap)
        }
    }

    /// 以选中内容为刀（DEBT-17）：优先音符，其次待确认形状的几何
    ///
    /// 只设置会话，不写文档；切完 / 失效 / 换轨 / Esc 时结束。
    pub fn mould_set_knife(&mut self) -> Result<MouldKnifeInfo, MouldReject> {
        let notes = self.selected_document_notes();
        let knife = if !notes.is_empty() {
            MouldKnife::Notes {
                track: self.editor_state.data.current_track,
                notes,
            }
        } else {
            let rings = selected_pending_rings(self);
            if rings.is_empty() {
                return Err(MouldReject::NoKnifeSource);
            }
            MouldKnife::Geometry { rings }
        };
        let info = knife.info();
        self.mould_knife = Some(knife);
        self.grid_cache.clear();
        Ok(info)
    }

    /// 显式清除刀（Esc）；返回是否确实清除了
    pub fn mould_clear_knife(&mut self) -> bool {
        let had = self.mould_knife.take().is_some();
        if had {
            self.grid_cache.clear();
        }
        had
    }

    /// 刀源被删/改后立即失效（事件路径：换工具、删除）
    ///
    /// 返回是否发生了失效；失效时刀被清除并登记一次性通知，由 UI 层转成 toast。
    pub(crate) fn sync_mould_knife(&mut self) -> bool {
        let stale = self
            .mould_knife
            .as_ref()
            .is_some_and(|knife| !knife.is_current(self));
        if stale {
            self.mould_knife = None;
            self.mould_notice = Some(MouldNotice::Expired);
            self.grid_cache.clear();
        }
        stale
    }

    /// 取走刀相关的一次性通知（UI 层转 toast）
    pub fn take_mould_notice(&mut self) -> Option<MouldNotice> {
        self.mould_notice.take()
    }

    /// 当前面团是否就是刀里的某个形状
    fn dough_matches_knife(&self, knife: &MouldKnife) -> bool {
        if knife.rings().is_empty() {
            return false;
        }
        let track = self.editor_state.data.current_track;
        let Some(reference) = self
            .editor_state
            .shape_select
            .selected_shapes()
            .find(|shape| shape.pending.is_some())
            .and_then(|shape| shape.pending)
        else {
            return false;
        };
        let Some(source) = self.pending_source(reference) else {
            return false;
        };
        let model = self.model_from_source_at_view(&source, track, 1.0);
        let ring = ring_of(&model);
        knife.contains_ring(&ring)
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
    ///
    /// DEC-001：只有 pending 选区可创建；已 Bake 图形只读。
    pub fn create_between_selected(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Between);
        }
        false
    }

    /// 027：选中一个图形，沿默认路径复制 4 份（半拍步长，原图 + 4 = 5 个）
    pub fn copy_along_selected(&mut self) -> bool {
        let ppq = self.editor_state.view.ppq;
        self.copy_along_selected_with(f32::from(ppq) * 0.5, 5)
    }

    /// 027：按显式 **tick 步长** 与 **总组数** 复制（参数面板/脚本入口）。
    ///
    /// `groups` 是含原图的总组数：`groups = 5` → 4 份副本。
    /// DEC-001：只有 pending 选区可复制；已 Bake 图形只读。
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
        false
    }

    /// 最近一次 027 复制的可展示摘要（总组数 · 步长 tick），供 Toast 校验。
    ///
    /// pending 组合优先，其次当前选中的已 Bake 模型（只读展示）；`Every` 模式
    /// 无法从路径长度反推步长，返回 `None`。
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
                        lumino_core::shape::ShapeOp::CopyAlong(params) => {
                            summarize_copy_params(params)
                        }
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
                    lumino_core::shape::ShapeOp::CopyAlong(params) => summarize_copy_params(params),
                    _ => None,
                })
            })?;
        Some(format!("{groups} 组 · 步长 {step:.0} tick"))
    }

    /// 028：对选中图形应用渐强力度包络
    ///
    /// DEC-001：只有 pending 选区可应用；已 Bake 图形只读。
    pub fn velocity_up_selected(&mut self) -> bool {
        if self.pending_single_selection() {
            return self.compose_pending(ComposedKind::Velocity);
        }
        false
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
