//! DRAW-020：多选组变换（仅 ✓ 前 pending 组合）
//!
//! 组的手柄 box = 所有成员模型包围盒的并集；拖动时每个成员按同一个
//! `apply_group_drag` 算子映射，参数各自写回，不烘焙点列。

use lumino_core::shape::ShapeModel;
use lumino_editor_state::PendingComposed;
use lumino_editor_state::editor_state::union_model;

use crate::Editor;

/// 组编辑成员：`pending_composed` 下标 + 拖动前模型
#[derive(Debug, Clone)]
pub(crate) struct GroupPart {
    /// `EditorState::pending_composed` 下标
    pub index: usize,
    /// 拖动前模型
    pub model: ShapeModel,
}

impl Editor {
    /// 当前组编辑的成员（按 `shape_group_edit` 顺序）；不足 2 支返回 `None`
    pub(crate) fn group_parts(&self) -> Option<Vec<GroupPart>> {
        let indexes = self.shape_group_edit.as_ref()?;
        let mut parts = Vec::with_capacity(indexes.len());
        for index in indexes {
            let model = self
                .editor_state
                .pending_composed
                .get(*index)
                .and_then(|composed| composed.results.first())
                .map(|part| part.model.clone())?;
            parts.push(GroupPart {
                index: *index,
                model,
            });
        }
        (parts.len() >= 2).then_some(parts)
    }

    /// 组手柄模型 = 成员联合 box
    pub(crate) fn group_union_model(&self) -> Option<ShapeModel> {
        let models: Vec<_> = self
            .group_parts()?
            .into_iter()
            .map(|part| part.model)
            .collect();
        union_model(&models)
    }

    /// 组编辑写回：每个成员模型 + 按模型重新生成的音符
    pub(crate) fn write_group_parts(&mut self, parts: &[GroupPart], models: &[ShapeModel]) {
        for (part, model) in parts.iter().zip(models) {
            let notes = self.part_notes_for_edit(model, model.track as usize);
            if let Some(target) = self
                .editor_state
                .pending_composed
                .get_mut(part.index)
                .and_then(|composed| composed.results.first_mut())
            {
                target.model = model.clone();
                target.notes = notes;
            }
        }
        self.refresh_pending_mirror_geometry();
    }

    /// 恢复整份 `pending_composed` 快照
    pub(crate) fn restore_group_snapshot(&mut self, snapshot: Vec<PendingComposed>) {
        self.editor_state.pending_composed = snapshot;
        self.refresh_pending_mirror_geometry();
    }
}
