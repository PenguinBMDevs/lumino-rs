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
    /// 开始拖拽调整面板宽度
    ResizeDragStarted,
    /// 拖拽中调整面板宽度
    ResizeDragged,
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
}
