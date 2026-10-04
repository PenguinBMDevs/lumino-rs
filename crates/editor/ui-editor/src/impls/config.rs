//! Editor 配置相关方法 — 简单的 getter/setter/delegate
//!
//! 包含：工具切换、光标/画布状态、总 ticks 和 PPQ、音符变更标志、缓存失效

use crate::{CacheInvalidation, Editor};
use iced_core::Point;
use lumino_editor_state::editor_state::viewport::Viewport;
use lumino_message::Tool;

impl Editor {
    /// 设置当前工具（委托到 editor_state）
    ///
    /// 切到非曲线工具时自动关闭曲线工具颜料桶填充模式；
    /// 切到非形状工具时自动关闭形状工具颜料桶填充模式（均为工具附属开关）。
    ///
    /// ⚠️ 待确认的绘制内容（曲线路径 / 待确认图形 / 画刷笔画 / 文本框）**不因切换工具丢失**
    /// ——见 `EditorState::set_tool` 只收敛交互手势、保留产物。切到「鼠标工具」
    /// （`Tool::ShapeSelect`）时同样**只保留、不固化**：用户没按 √ 就不该生成音符。
    pub fn set_tool(&mut self, tool: Tool) {
        self.editor_state.set_tool(tool);
        if tool != Tool::Curve {
            self.editor_state.line_tool.fill_enabled = false;
        }
        // 形状工具：离开时关闭其填充桶（避免遗留影响其他工具）
        if tool != Tool::Shape {
            self.editor_state.shape_tool.fill_enabled = false;
        }
    }

    /// 某绘制工具的**待确认预览**此刻是否应渲染/可交互
    ///
    /// 规则：拥有者工具激活时照旧；**鼠标工具（`Tool::ShapeSelect`，图形选中）激活时也渲染**。
    /// 依据：待确认内容不再因切换工具被丢弃（见 `EditorState::set_tool`），若选择工具下不渲染，
    /// 用户看到的就是「切过去图案就消失、无从框选」。其余工具不渲染，避免浮层干扰音符编辑。
    ///
    /// ⚠️ 仅用于**几何预览**（曲线/形状/画刷方块/文本框），不含 √× 按钮——
    /// 按钮仍是拥有者工具专属（确认/取消属于绘制工具的操作，选择工具不越权）。
    pub fn pending_preview_visible(&self, owner: Tool) -> bool {
        let current = self.current_tool();
        current == owner || current == Tool::ShapeSelect
    }

    /// 设置当前形状类型（由工具栏 `current_shape` 同步）
    pub fn set_shape(&mut self, kind: lumino_editor_state::ShapeKind) {
        self.editor_state.shape_tool.set_shape_kind(kind);
    }

    /// 设置颜料桶填充模式开关（曲线工具与形状工具共享同一开关）
    pub fn set_fill_enabled(&mut self, enabled: bool) {
        self.editor_state.line_tool.fill_enabled = enabled;
        self.editor_state.shape_tool.fill_enabled = enabled;
    }

    /// 颜料桶填充模式是否开启
    pub fn fill_enabled(&self) -> bool {
        self.editor_state.line_tool.fill_enabled
    }

    /// 设置颜料桶填充的**切分档位**（`Some(x)` = x 分音符切分；`None` = 整块填充）
    ///
    /// 由主窗口「分音符填充」对话框确认后调用。变更会同步进路径历史
    /// （有待确认内容时），使 Ctrl+Z 可撤销。
    pub fn set_fill_division(&mut self, division: Option<u32>) {
        self.editor_state.line_tool.set_fill_division(division);
        self.mark_notes_changed();
    }

    /// 颜料桶填充的切分档位（`None` = 整块填充）
    pub fn fill_division(&self) -> Option<u32> {
        self.editor_state.line_tool.fill_division
    }

    /// 取走「打开分音符填充对话框」请求（一次性标志，Root 每帧取用）
    pub fn take_fill_division_dialog_request(&mut self) -> bool {
        let requested = self.fill_division_dialog_requested;
        self.fill_division_dialog_requested = false;
        requested
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
    /// - `playback_scan_state`：播放键色增量扫描状态（避免旧文档残留）
    pub fn reset_internal_state(&mut self) {
        self.notes_changed = false;
        self.playback_position = 0.0;
        self.playback_scan_state = Default::default();
        self.velocity_panel = crate::velocity::VelocityPanel::new();
    }

    /// 设置播放时键盘颜色指示是否启用
    pub fn set_playback_key_colors_enabled(&mut self, enabled: bool) {
        self.playback_key_colors_enabled = enabled;
        if !enabled {
            self.playback_key_colors = [0u8; 1024];
            // 关闭时重置扫描状态，下次启用从干净状态开始
            self.playback_scan_state = Default::default();
        }
    }

    /// Domino（TAKABO SOFT）剪贴板互粘是否启用（默认关闭，仅 Windows 生效）
    ///
    /// 跨平台方法：非 Windows 下 Domino 路径整体被 `#[cfg(windows)]` 裁掉，该字段无处可读；
    /// 保留跨平台读写是为了让 `Root` 的设置同步链路三平台一致，避免 cfg 分叉。
    #[inline]
    pub fn domino_clipboard_enabled(&self) -> bool {
        self.domino_clipboard_enabled
    }

    /// 设置 Domino（TAKABO SOFT）剪贴板互粘是否启用（仅 Windows 生效）
    #[inline]
    pub fn set_domino_clipboard_enabled(&mut self, enabled: bool) {
        self.domino_clipboard_enabled = enabled;
    }
}
