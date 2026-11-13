//! 形状抽屉 → 工程：待确认插入与「退回抽屉」（DEBT-14 / DEC-001）
//!
//! ## 语义
//!
//! **Insert 只是提案，✓ 才是定稿。**
//!
//! - Insert：生成 pending 形状 + 预览音符，`document` 在 ✓ 前**一个音符都不变**，
//!   不产生任何文档历史（所以「未 ✓ 保存并重开工程」天然看不到它）；
//! - ✓：仍走 `confirm_shape_tool` 的唯一提交点（`pending_composed` → 文档 + Bake 快照）；
//! - 退回抽屉：删掉 pending、把原图与插入参数回填抽屉，**不写文档历史**。
//!
//! ## 为什么用 `PendingComposed` 承载
//!
//! 预览（`grid/shape_tool_box.rs`）、✓（`confirm_shape_tool`）、undo
//! （`BakePending::Shape`）三条既有通路都已按 `pending_composed` 工作；
//! 新增一个容器等于把这三条各复制一遍。`ComposedKind::Insert` 只用于区分来源。

use lumino_core::Tool;
use lumino_core::shape::{DrawerInsert, ShapeModel, ShapeRect};
use lumino_editor_state::{PendingComposed, ShapeNote};

use crate::Editor;

impl Editor {
    /// 抽屉 Insert：生成待确认形状 + 预览音符（**不写文档、不推历史**）
    ///
    /// 返回 `false` 表示未插入（指挥轨 / 空几何），调用方据此保留抽屉状态。
    pub fn insert_shape_pending(&mut self, model: ShapeModel, drawer: DrawerInsert) -> bool {
        let track = model.track as usize;
        if track == 0 {
            tracing::debug!("形状: Conductor 轨道禁止插入待确认形状");
            return false;
        }
        let notes = self.shape_notes_of(&model, track);
        if notes.is_empty() {
            tracing::debug!("形状: 抽屉图形没有生成任何预览音符，Insert 取消");
            return false;
        }
        // 先切到形状选择：`rebuild_pending_mirrors` 只在 ShapeSelect 下建镜像
        // （否则 pending 既画不出选择框，也右键不到「退回抽屉」）。
        self.set_tool(Tool::ShapeSelect);
        self.push_pending_insert(model, notes, drawer);
        true
    }

    /// 把 pending 插入的结果落到 `pending_composed` 并刷新选择镜像
    fn push_pending_insert(
        &mut self,
        model: ShapeModel,
        notes: Vec<ShapeNote>,
        drawer: DrawerInsert,
    ) {
        let index = self.editor_state.pending_composed.len();
        self.editor_state
            .pending_composed
            .push(PendingComposed::from_drawer(model, notes, drawer));
        self.pending_composed_redo.clear();
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        // 插入后立刻选中，右键菜单才有目标（与既有组合入口一致）。
        if let Some(id) = self
            .editor_state
            .shape_select
            .pending_mirror(lumino_editor_state::PendingShapeRef::Composed(index))
        {
            self.editor_state.shape_select.select_only(id);
        }
        self.grid_cache.clear();
        self.mark_notes_changed();
    }

    /// 退回抽屉：删除该 pending 并把来源原样交回（不写文档历史）
    ///
    /// 返回 `None` 表示该下标不是抽屉插入或已失效——调用方必须保留 pending，
    /// 不能出现「预览没了、抽屉也没回」的中间态。
    pub fn return_pending_to_drawer(&mut self, index: usize) -> Option<DrawerInsert> {
        let entry = self.editor_state.pending_composed.get(index)?;
        let drawer = entry.drawer.as_ref()?;
        // 先取走再删除：只有确认来源可用才动 pending。
        let drawer = drawer.clone();
        self.editor_state.pending_composed.remove(index);
        self.pending_composed_redo.clear();
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        self.mark_notes_changed();
        Some(drawer)
    }

    /// 该 pending 是否是抽屉插入（供右键菜单决定是否显示「退回抽屉」）
    pub fn pending_drawer_source(&self, index: usize) -> Option<&DrawerInsert> {
        self.editor_state
            .pending_composed
            .get(index)
            .and_then(|entry| entry.drawer.as_ref())
    }

    /// 该 pending 的落位矩形（抽屉插入才有；供测试/UI 复核）
    pub fn pending_insert_rect(&self, index: usize) -> Option<ShapeRect> {
        self.pending_drawer_source(index).map(|d| d.spec.rect)
    }
}

#[cfg(test)]
mod tests;
