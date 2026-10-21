//! 020 Custom edit 的 UI 拖拽状态（CP-4.5a）
//!
//! 纯变换在 `editor_state::shape_edit`；本模块负责：
//! 命中手柄 → 拖动预览 → 松手调用 `edit_shape` 一次提交。

use iced_core::Point;
use lumino_core::shape::ShapeModel;
use lumino_editor_state::editor_state::{
    HandleKind, Mods, Zoom, apply_drag, edit_handles, hit_test,
};

use crate::Editor;

use super::shape_bridge::source_from_model;

/// 一次形状手柄拖拽
#[derive(Debug, Clone)]
pub(crate) struct ShapeEditState {
    /// UI 图形 id
    pub drawn_id: u64,
    /// ShapeStore 模型 id
    pub model_id: u64,
    /// 当前手柄
    pub handle: HandleKind,
    /// 按下时的逻辑坐标
    pub start: (f32, f32),
    /// 拖动前模型（取消/判断空操作）
    pub original: ShapeModel,
    /// 当前预览模型
    pub current: ShapeModel,
    /// Ctrl / Shift
    pub mods: Mods,
}

impl Editor {
    /// 为当前唯一选中的图形绑定 ShapeModel，随后渲染编辑手柄
    pub fn enable_shape_edit(&mut self) -> bool {
        if self.editor_state.shape_select.selection_len() != 1 {
            return false;
        }
        let Some(id) = self
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .map(|shape| shape.id)
        else {
            return false;
        };
        let ok = self.ensure_model_for_drawn(id).is_some();
        if ok {
            self.grid_cache.clear();
        }
        ok
    }

    /// 命中选中形状的手柄则进入编辑；返回是否接管本次按下
    pub(crate) fn begin_shape_edit(&mut self, tick: f32, key: f32) -> bool {
        if self.current_tool() != lumino_message::Tool::ShapeSelect
            || self.editor_state.shape_select.selection_len() != 1
        {
            return false;
        }
        let Some(drawn) = self
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .cloned()
        else {
            return false;
        };
        // 只有已绑定 ShapeModel 的图形才进入手柄编辑；普通图形保持原有框选/拖动语义。
        let Some(model_id) = drawn.model_id else {
            return false;
        };
        let Some(model) = self.editor_state.shape_store.model(model_id).cloned() else {
            return false;
        };
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let Some(handle) = hit_test(&model, (tick, key), zoom, 8.0) else {
            return false;
        };
        self.shape_edit = Some(ShapeEditState {
            drawn_id: drawn.id,
            model_id,
            handle,
            start: (tick, key),
            original: model.clone(),
            current: model,
            mods: Mods {
                ctrl: self.ctrl_pressed(),
                shift: self.shift_pressed(),
            },
        });
        true
    }

    /// 拖动手柄；返回是否更新
    pub(crate) fn update_shape_edit(&mut self, tick: f32, key: f32) -> bool {
        let Some(state) = &mut self.shape_edit else {
            return false;
        };
        let k = state.current.k;
        if !apply_drag(
            &mut state.current,
            state.handle,
            state.start,
            (tick, key),
            state.mods,
            k,
        ) {
            return false;
        }
        let source = source_from_model(&state.current);
        self.editor_state
            .shape_select
            .set_source(state.drawn_id, source);
        self.grid_cache.clear();
        true
    }

    /// 松手提交；无实际变化或失败时恢复原几何
    pub(crate) fn finish_shape_edit(&mut self) -> bool {
        let Some(state) = self.shape_edit.take() else {
            return false;
        };
        if state.current == state.original {
            let source = source_from_model(&state.original);
            self.editor_state
                .shape_select
                .set_source(state.drawn_id, source);
            return false;
        }
        let ok = self.edit_shape(state.model_id, state.current);
        if !ok {
            let source = source_from_model(&state.original);
            self.editor_state
                .shape_select
                .set_source(state.drawn_id, source);
        }
        self.grid_cache.clear();
        ok
    }

    /// 取消拖拽并恢复原几何（预留给 Esc）
    #[allow(dead_code)]
    pub(crate) fn cancel_shape_edit(&mut self) {
        let Some(state) = self.shape_edit.take() else {
            return;
        };
        let source = source_from_model(&state.original);
        self.editor_state
            .shape_select
            .set_source(state.drawn_id, source);
        self.grid_cache.clear();
    }

    /// 当前编辑/选中形状的手柄屏幕位置（用于渲染）
    pub(crate) fn shape_edit_handle_points(&self) -> Vec<Point> {
        let Some(drawn) = self.editor_state.shape_select.selected_shapes().next() else {
            return Vec::new();
        };
        let Some(model_id) = drawn.model_id else {
            return Vec::new();
        };
        let Some(model) = self.editor_state.shape_store.model(model_id) else {
            return Vec::new();
        };
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let Some(layout) = edit_handles(model, zoom) else {
            return Vec::new();
        };
        layout
            .handles
            .iter()
            .map(|(_, pos)| self.line_pos_screen_pos(*pos))
            .collect()
    }

    /// 是否正在拖拽形状手柄
    pub(crate) fn is_shape_editing(&self) -> bool {
        self.shape_edit.is_some()
    }
}

#[cfg(test)]
mod tests;
