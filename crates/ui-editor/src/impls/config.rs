//! Editor 配置相关方法 — 简单的 getter/setter/delegate
//!
//! 包含：工具切换、光标/画布状态、总 ticks 和 PPQ、音符变更标志、缓存失效

use crate::{CacheInvalidation, Editor};
use lumino_message::Tool;
use iced_core::Point;
use lumino_core::editor_state::viewport::Viewport;

impl Editor {
    /// 设置当前工具（委托到 editor_state）
    pub fn set_tool(&mut self, tool: Tool) {
        self.editor_state.set_tool(tool);
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.editor_state.tool
    }

    /// 更新鼠标位置（由外部调用）
    pub fn update_cursor_position(&mut self, position: Option<Point>) {
        let pos = position.map(|p| (p.x, p.y));
        if self.editor_state.canvas.cursor_position == pos {
            return;
        }
        self.editor_state.canvas.cursor_position = pos;
    }

    /// 更新 Canvas 偏移量（用于坐标转换）
    pub fn set_canvas_offset(&mut self, offset: Point) {
        self.editor_state.canvas.offset_x = offset.x;
        self.editor_state.canvas.offset_y = offset.y;
    }

    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: Point) {
        self.editor_state.canvas.size_x = size.x;
        self.editor_state.canvas.size_y = size.y;
    }

    /// 设置总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: u32) {
        self.editor_state.view.total_ticks = total_ticks;
        Viewport::new(
            &mut self.editor_state.view,
            &mut self.editor_state.max_scroll,
        )
        .update_max_scroll(total_ticks);
    }

    /// 设置 PPQ
    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor_state.view.ppq = ppq;
    }

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 统一缓存失效（替代散落的 grid_cache.clear() 等调用）
    #[inline]
    pub fn invalidate_caches(&mut self, which: CacheInvalidation) {
        if which.0 & CacheInvalidation::GRID.0 != 0 {
            self.grid_cache.clear();
        }
        if which.0 & CacheInvalidation::KEYBOARD.0 != 0 {
            self.keyboard_cache.clear();
        }
        if which.0 & CacheInvalidation::RULER.0 != 0 {
            self.ruler_cache.clear();
        }
    }

    /// 重置编辑器内部状态到默认值（释放私有字段内存）
    ///
    /// 供 `clear_editor()` 调用，重置本模块私有的字段：
    /// - `notes_changed`：音符变更标志
    /// - `playback_position`：播放指示线位置
    pub fn reset_internal_state(&mut self) {
        self.notes_changed = false;
        self.playback_position = 0.0;
        self.velocity_panel = crate::velocity::VelocityPanel::new();
    }

    /// 设置播放时键盘颜色指示是否启用
    pub fn set_playback_key_colors_enabled(&mut self, enabled: bool) {
        self.playback_key_colors_enabled = enabled;
        if !enabled {
            self.playback_key_colors = [0u8; 1024];
        }
    }
}
