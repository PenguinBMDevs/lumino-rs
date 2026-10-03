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

    /// 曲线工具按钮在「按下」时应发出的事件。
    ///
    /// - 普通点击：选择曲线工具（基础态）。
    /// - 仅当当前已处于画刷工具时，Ctrl+点击才打开画刷工具下拉（设置面板）；
    ///   非画刷工具下 Ctrl+点击退化为普通点击，避免误弹画刷设置面板。
    /// - 仅当当前已处于形状工具时，Ctrl+点击才打开形状工具下拉（矩形/圆形/三角形
    ///   选择菜单）；非形状工具下 Ctrl+点击退化为普通点击，避免误弹形状菜单。
    /// - 油漆桶开启时（Curve+fill），Ctrl+点击打开「分音符填充」对话框 ——
    ///   与画刷/形状同语义：Ctrl+点当前工具按钮 = 打开该工具的设置。
    ///   **不得**退化为 `ToolSelected(Curve)`：那会把已开启的油漆桶打回曲线
    ///   （fill_enabled=false），正是「图标变回曲线、弹窗不出现」的根因。
    ///
    /// 该决策从视图层抽出，便于单元测试回归（见 `tests` 模块）。
    pub fn curve_button_press_event(&self) -> Event {
        if self.ctrl_pressed && self.current_tool == Tool::Brush {
            Event::ToggleBrushDropdown
        } else if self.ctrl_pressed && self.current_tool == Tool::Shape {
            Event::ToggleShapeDropdown
        } else if self.ctrl_pressed && self.current_tool == Tool::Curve && self.fill_enabled {
            Event::OpenFillDivisionDialog
        } else {
            Event::ToolSelected(Tool::Curve)
        }
    }
}
