//! 右侧栏动作

/// 右侧栏动作
#[derive(Debug, Clone)]
pub enum RightSidebarAction {
    /// 点击图片转 MIDI 按钮（展开/收起面板并亮灯）
    ImageToMidiClicked,
    /// 面板内点击"选择图片文件"按钮（弹出文件对话框）
    SelectImageFile,
    /// 面板内点击"转换为 MIDI"按钮（调用 i2m-rs 转换并进入放置模式）
    ConvertClicked,
    /// 放置模式悬浮按钮：√ 确认（写入 document）
    PlacementConfirm,
    /// 放置模式悬浮按钮：× 取消（还原显示区域）
    PlacementCancel,
    /// 更新 i2m 数字配置项（文本仅接受数字；空串仅保留输入缓冲）
    I2mConfigTextChanged { field: I2mConfigField, text: String },
    /// 切换调色板算法（索引指向 `PALETTE_ALGORITHMS`）
    I2mPaletteChanged(usize),
    /// 开始拖拽调整面板宽度
    ResizeDragStarted,
    /// 拖拽中调整面板宽度
    ResizeDragged,
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
}

/// i2m 转换配置中的数字编辑字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2mConfigField {
    /// 最低 MIDI key（0-127）
    StartKey,
    /// 最高 MIDI key（0-127）
    EndKey,
    /// 目标高度（像素，0=保持宽高比）
    TargetHeight,
    /// 每像素行对应的 MIDI tick（>0）
    TicksPerPixel,
    /// 调色板颜色数（=生成音轨数）
    ColorCount,
}
