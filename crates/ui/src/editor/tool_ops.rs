use crate::editor::Editor;
use crate::toolbar::Tool;

impl Editor {
    /// 设置当前工具
    pub fn set_tool(&mut self, tool: Tool) {
        self.current_tool = tool;
        // 切换工具时清除选中状态
        if tool != Tool::Pointer {
            self.selected_notes.clear();
        }
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.current_tool
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        self.remote_cursors.insert(
            user_id.to_string(),
            (
                iced_core::Point::new(x, y),
                color.to_string(),
                username.to_string(),
            ),
        );
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
    }

    /// 更新鼠标位置（由外部调用）
    pub fn update_cursor_position(&mut self, position: Option<iced_core::Point>) {
        self.cursor_position = position;
    }

    /// 更新 Canvas 偏移量（用于坐标转换）
    pub fn set_canvas_offset(&mut self, offset: iced_core::Point) {
        self.canvas_offset = offset;
    }

    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: iced_core::Point) {
        self.canvas_size = size;
    }
}
