//! 右侧栏动作

/// 右侧栏动作
#[derive(Debug, Clone)]
pub enum RightSidebarAction {
    /// 切换面板显示/隐藏
    TogglePanel,
    /// 点击图片转 MIDI 按钮
    ImageToMidiClicked,
    /// 开始拖拽调整面板宽度
    ResizeDragStarted,
    /// 拖拽中调整面板宽度
    ResizeDragged,
    /// 结束拖拽调整面板宽度
    ResizeDragEnded,
}
