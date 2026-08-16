//! 编辑器相关消息类型

use crate::Point2;

/// 编辑器动作
#[derive(Debug, Clone)]
pub enum EditorAction {
    Pressed {
        pos: Point2,
        shift: bool,
    },
    Moved(Point2),
    Released,
    Scrolled {
        delta_x: f32,
        delta_y: f32,
    },
    /// 双击事件
    DoubleClicked(Point2),
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
    /// 曲线工具直线：确认并按直线经过的区域生成音符（√ 按钮）
    LineToolConfirm,
    /// 曲线工具直线：取消并清空（× 按钮）
    LineToolCancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_action_clone() {
        let action = EditorAction::DeletePressed;
        let cloned = action.clone();
        assert!(matches!(cloned, EditorAction::DeletePressed));
    }

    #[test]
    fn test_editor_action_debug() {
        let action = EditorAction::Undo;
        let debug = format!("{:?}", action);
        assert!(debug.contains("Undo"));
    }
}
