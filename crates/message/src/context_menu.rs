//! 钢琴卷帘右键上下文菜单消息类型
//!
//! 定义钢琴卷帘区域内右键弹出的内嵌悬浮面板菜单的动作。
//! 保持与 UI 框架无关，使用 `Point2` 传递坐标。

use crate::Point2;

/// 钢琴卷帘右键上下文菜单动作
#[derive(Debug, Clone)]
pub enum PianoRollContextMenuAction {
    /// 在指定位置打开菜单（canvas 局部坐标）
    Open { position: Point2 },
    /// 关闭菜单
    Close,
    /// 点击菜单项
    ItemClicked(PianoRollContextMenuItem),
}

/// 上下文菜单项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PianoRollContextMenuItem {
    /// 剪切
    Cut,
    /// 复制
    Copy,
    /// 粘贴
    Paste,
    /// 删除
    Delete,
    /// 全选
    SelectAll,
    /// 批量编辑
    BatchEdit,
}

/// 音轨选项卡右键上下文菜单项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackContextMenuItem {
    /// 删除音轨
    Delete,
    /// 重命名音轨
    Rename,
    /// 设置选项卡颜色
    SetColor,
    /// 设置通道
    SetChannel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_item_variants() {
        let _items = [
            PianoRollContextMenuItem::Cut,
            PianoRollContextMenuItem::Copy,
            PianoRollContextMenuItem::Paste,
            PianoRollContextMenuItem::Delete,
            PianoRollContextMenuItem::SelectAll,
        ];
    }

    #[test]
    fn test_track_menu_item_variants() {
        let _items = [
            TrackContextMenuItem::Delete,
            TrackContextMenuItem::Rename,
            TrackContextMenuItem::SetColor,
            TrackContextMenuItem::SetChannel,
        ];
    }

    #[test]
    fn test_action_open_position() {
        let action = PianoRollContextMenuAction::Open {
            position: Point2::new(120.0, 80.0),
        };
        match action {
            PianoRollContextMenuAction::Open { position } => {
                assert!((position.x - 120.0).abs() < f32::EPSILON);
                assert!((position.y - 80.0).abs() < f32::EPSILON);
            }
            _ => panic!("应为 Open 动作"),
        }
    }
}
