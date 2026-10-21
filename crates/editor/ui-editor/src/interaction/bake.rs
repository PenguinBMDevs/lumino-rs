//! Bake 历史（DRAW-041）
//!
//! ✓ 后丢弃矢量对象，只保留音符。为了 undo 能回到待确认曲线/形状/笔刷，
//! 这里按历史 group 保存各工具的 pending 状态；undo 恢复，redo 再次丢弃。

use lumino_core::BrushConfig;
use lumino_editor_state::{BrushToolState, LineToolState, ShapeToolState};

use crate::Editor;

/// 一次 Bake 对应的待确认矢量快照
#[derive(Debug, Clone)]
pub(crate) enum BakePending {
    /// 曲线工具
    Line(LineToolState),
    /// 形状工具
    Shape(ShapeToolState),
    /// 画刷工具（笔画 + 粗细/分层配置）
    Brush {
        /// 待确认笔画与笔画历史
        state: BrushToolState,
        /// 全局粗细与分层映射
        config: BrushConfig,
    },
}

impl Editor {
    /// 记录 ✓ 前的 pending 快照；`group` 为音符创建历史分组
    pub(crate) fn remember_bake(&mut self, group: u64, pending: BakePending) {
        if group == 0 {
            return;
        }
        self.baked_pending.insert(group, pending);
        // 历史本身有上限；这里同步给一个宽松上限，避免会话内无限增长。
        while self.baked_pending.len() > 256 {
            let Some(oldest) = self.baked_pending.keys().next().copied() else {
                break;
            };
            self.baked_pending.remove(&oldest);
        }
    }

    /// undo 一个 Bake group：恢复待确认矢量
    pub(crate) fn restore_baked_group(&mut self, group: u64) -> bool {
        let Some(pending) = self.baked_pending.get(&group).cloned() else {
            return false;
        };
        match pending {
            BakePending::Line(mut state) => {
                // 当前状态是权威：恢复后不应再走“路径 redo”而跳过 Bake redo。
                state.path_history.truncate(state.path_history_index + 1);
                self.editor_state.line_tool = state;
            }
            BakePending::Shape(state) => self.editor_state.shape_tool = state,
            BakePending::Brush { mut state, config } => {
                state.path_history.truncate(state.path_history_index + 1);
                self.editor_state.brush_tool = state;
                self.brush = config;
            }
        }
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }

    /// redo 一个 Bake group：再次丢弃矢量
    pub(crate) fn discard_baked_group(&mut self, group: u64) -> bool {
        let Some(pending) = self.baked_pending.get(&group).cloned() else {
            return false;
        };
        match pending {
            BakePending::Line(_) => self.editor_state.line_tool.reset(),
            BakePending::Shape(_) => self.editor_state.shape_tool.clear_pending(),
            BakePending::Brush { .. } => self.editor_state.brush_tool.reset(),
        }
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }
}
