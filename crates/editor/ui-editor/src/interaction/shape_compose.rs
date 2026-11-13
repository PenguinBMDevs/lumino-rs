//! 019–022 的 UI 组合入口（CP-4.5a）
//!
//! DEC-001：Bake 后形状对象只读。本模块只保留 **✓ 前 pending** 的组合分支；
//! 已 Bake 图形（`pending == None`）一律拒绝，不再写回 ShapeStore / 文档。

use lumino_core::tr;
use lumino_editor_state::ComposedKind;
use lumino_editor_state::shape_ops::SymReject;

use crate::Editor;

impl Editor {
    /// 保证选中图形有对应的 `ShapeStore` 模型；返回模型 id
    ///
    /// 只用于**只读展示/分享**（例如复制为文本）；不生成或重写音符。
    pub(crate) fn ensure_model_for_drawn(&mut self, drawn_id: u64) -> Option<u64> {
        if let Some(model_id) = self.editor_state.shape_select.model_id_of(drawn_id)
            && self.editor_state.shape_store.model(model_id).is_some()
        {
            return Some(model_id);
        }
        let drawn = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .find(|shape| shape.id == drawn_id)?
            .clone();
        if drawn.track == 0 || drawn.pending.is_some() {
            return None;
        }
        let model = self.model_from_drawn_at_view(&drawn, 1.0);
        let model_id = self.editor_state.shape_store.insert(model, 0);
        self.editor_state
            .shape_store
            .assign_notes(model_id, drawn.notes.clone());
        self.editor_state
            .shape_select
            .set_model_id(drawn_id, Some(model_id));
        Some(model_id)
    }

    /// 019：选中的待确认图形折叠成一个 Live shape
    ///
    /// DEC-001：已 Bake 图形只读，不提供再组合。
    pub fn live_selected_shapes(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Live);
        }
        false
    }

    /// 021：选中的待确认图形连接成一条曲线
    ///
    /// DEC-001：已 Bake 图形只读，不提供再连接。
    pub fn join_selected_shapes(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Join);
        }
        false
    }

    /// 022：两个待确认图形按门控逐 key 行合并
    ///
    /// DEC-001：已 Bake 图形只读，不提供再合并。
    pub fn merge_selected_shapes(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Merge);
        }
        false
    }

    /// 019/021：还原最近一次组合前的图形
    ///
    /// DEC-001：组合只存在于 ✓ 前；已 Bake 图形不提供拆分——恢复旧快照会用
    /// 旧音符覆盖用户已编辑的结果，与「音符是唯一权威」冲突。
    pub fn split_composed_shape(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;

/// 半边对称拒绝原因的用户可见文案（DRAW-043：拒绝必须说得出为什么）
pub fn sym_reject_message(reason: SymReject) -> &'static str {
    match reason {
        SymReject::NoSelection => tr!("请先选中一个待确认形状"),
        SymReject::NotCurve => tr!("只有曲线形状支持半边对称"),
        SymReject::JoinedCurve => tr!("已连接的曲线不支持半边对称"),
        SymReject::TooFewAnchors => tr!("锚点太少，无法做半边对称"),
        SymReject::EvenAnchors => tr!("锚点数为偶数，中间锚点需落在对称轴上"),
        SymReject::InvalidParams => tr!("对称参数不合法"),
        SymReject::DegenerateAxis => tr!("对称轴退化，无法求解"),
        SymReject::NonFinite => tr!("对称结果包含非有限值"),
        SymReject::EditFailed => tr!("形状更新失败"),
    }
}

mod ops;

pub(crate) use ops::CopyAlongSpec;
pub use ops::MouldReject;
