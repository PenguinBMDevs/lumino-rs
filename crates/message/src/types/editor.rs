//! 编辑器相关消息类型

/// 编辑器动作
#[derive(Debug, Clone)]
pub enum EditorAction {
    Pressed {
        pos: iced_core::Point,
        shift: bool,
    },
    Moved(iced_core::Point),
    Released,
    Scrolled {
        delta_x: f32,
        delta_y: f32,
    },
    /// 双击事件
    DoubleClicked(iced_core::Point),
    /// 删除键按下（Delete 或 Backspace）
    DeletePressed,
    /// 剪切
    Cut,
    /// 复制
    Copy,
    /// 粘贴
    Paste,
    /// 全选
    SelectAll,
    /// 撤销
    Undo,
    /// 重做
    Redo,
    /// 标尺 scrubbing：设置播放位置（tick 值）
    Scrubbed {
        tick: f32,
    },
    /// 演奏指示线拖拽开始（固定指示线模式下）
    IndicatorDragStart {
        x: f32,
    },
    /// 演奏指示线拖拽移动
    IndicatorDragMove {
        x: f32,
    },
}
