use super::*;

impl Toolbar {
    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    /// 开始调整大小，记录起始鼠标 Y 坐标
    pub fn start_resize(&mut self, cursor_y: f32) {
        self.is_resizing = true;
        self.resize_start_y = cursor_y;
        self.resize_start_height = self.height;
    }

    /// 更新拖拽位置（从外部传入当前鼠标 Y 坐标）
    pub fn update_resize_position(&mut self, cursor_y: f32) {
        if self.is_resizing {
            let delta_y = cursor_y - self.resize_start_y;
            let new_height = self.resize_start_height + delta_y;
            self.height = new_height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
    }

    /// 结束调整大小
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }

    /// 获取当前高度
    pub fn height(&self) -> f32 {
        self.height
    }
}
