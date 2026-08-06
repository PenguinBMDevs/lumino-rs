//! 右侧栏动作

/// 右侧栏动作
#[derive(Debug, Clone)]
pub enum RightSidebarAction {
    /// 点击图片转 MIDI 按钮（展开/收起面板并亮灯）
    ImageToMidiClicked,
    /// 面板内点击"选择图片文件"按钮（弹出文件对话框）
    SelectImageFile,
    /// 开始拖拽调整面板宽度
    ResizeDragStarted,
    /// 拖拽中调整面板宽度
    ResizeDragged,
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
}
