//! 钢琴卷帘右键上下文菜单消息类型
//!
//! 定义钢琴卷帘区域内右键弹出的内嵌悬浮面板菜单的动作。
//! 保持与 UI 框架无关，使用 `Point2` 传递坐标。

use crate::Point2;

/// 钢琴卷帘右键上下文菜单动作
#[derive(Debug, Clone)]
pub enum PianoRollContextMenuAction {
    /// 在指定位置打开菜单（canvas 局部坐标）
    Open {
        /// 菜单弹出位置
        position: Point2,
        /// 菜单作用目标（决定条目集）
        target: ContextMenuTarget,
    },
    /// 关闭菜单
    Close,
    /// 点击菜单项
    ItemClicked(PianoRollContextMenuItem),
}

/// 右键菜单作用目标
///
/// 同一套菜单容器承载两类目标：音符（传统钢琴卷帘菜单）与
/// **已绘制图形对象**（音符画工具栏「鼠标工具」选中后右键），
/// 由目标决定渲染哪些条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextMenuTarget {
    /// 音符（钢琴卷帘编辑）
    #[default]
    Notes,
    /// 已确认的绘制图形对象（曲线 / 形状 / 画刷 √ 后登记的图形）
    DrawnShape,
    /// 未 √ 的待确认镜像（仅几何、无音符；019–028 没有可操作模型）
    PendingDrawnShape,
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
    /// 删除选中的绘制图形（图形目标专用）
    DeleteDrawnShape,
    /// 折叠为 Live shape（DRAW-019）
    LiveShape,
    /// 还原 Live shape（DRAW-019）
    SplitLiveShape,
    /// 连接选中的图形（DRAW-021）
    JoinShapes,
    /// 还原 Join（DRAW-021）
    SplitJoinedShape,
    /// 门控合并两个图形（DRAW-022）
    MergeShapes,
    /// 进入自定义编辑（DRAW-020）
    EditShape,
    /// 进入区域着色（DRAW-032B）
    PaintAreas,
    /// 切换遮罩标记（DRAW-010；pending 只影响预览，✓ 后固定）
    MaskToggle,
    /// 打开遮罩参数面板（DRAW-010）
    MaskSettings,
    /// 打开 ShapeStore 模型列表（DRAW-041B）
    ShapeList,
    /// 竖轴对称（DRAW-023）
    SymmetryVertical,
    /// 横轴对称（DRAW-023）
    SymmetryHorizontal,
    /// 手绘扶正 50%（DRAW-024）
    Straighten,
    /// 取消扶正（DRAW-024）
    StraightenOff,
    /// 竖切（DRAW-025）
    SliceVertical,
    /// 创建补间（DRAW-026）
    BetweenCreate,
    /// 沿路径复制（DRAW-027）
    CopyAlong,
    /// 力度渐强（DRAW-028）
    VelocityUp,
    /// 复制选中图形为一行分享文本（DRAW-034）
    CopyShapeText,
    /// 从剪贴板一行分享文本粘贴形状（DRAW-034）
    PasteShapeText,
}

impl PianoRollContextMenuItem {
    /// 该条目所属的目标（用于按目标过滤菜单条目）
    pub fn target(self) -> ContextMenuTarget {
        match self {
            Self::DeleteDrawnShape
            | Self::LiveShape
            | Self::SplitLiveShape
            | Self::JoinShapes
            | Self::SplitJoinedShape
            | Self::MergeShapes
            | Self::EditShape
            | Self::PaintAreas
            | Self::MaskToggle
            | Self::MaskSettings
            | Self::SymmetryVertical
            | Self::SymmetryHorizontal
            | Self::Straighten
            | Self::StraightenOff
            | Self::SliceVertical
            | Self::BetweenCreate
            | Self::CopyAlong
            | Self::VelocityUp
            | Self::CopyShapeText => ContextMenuTarget::DrawnShape,
            _ => ContextMenuTarget::Notes,
        }
    }
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
    /// 设置端口（内部值 0..=15，UI 显示 1..=16）
    SetPort,
}

/// 音轨列表面板空白区域右键上下文菜单项
///
/// 与 `TrackContextMenuItem` 区分：本枚举针对音轨列表空白区域
/// （非音轨选项卡本身）触发的右键菜单，用于工程级音轨管理动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelContextMenuItem {
    /// 找回删除音轨（弹出对话框，列出已缓存的 `.lmdeltrack` 文件）
    RecoverDeletedTrack,
}

/// 素材库右键上下文菜单项
///
/// 与 `TrackContextMenuItem` 对应：针对右侧栏素材库面板中的单个素材项
/// 触发的右键菜单，提供素材级管理动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialContextMenuItem {
    /// 重命名素材（行内编辑：同步文件与 metadata 名称）
    Rename,
    /// 删除素材（本地文件，需二次确认）
    Delete,
    /// 上传到云（打开云存储文件管理面板选择上传位置）
    UploadToCloud,
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
    fn test_menu_item_target_routing() {
        assert_eq!(
            PianoRollContextMenuItem::DeleteDrawnShape.target(),
            ContextMenuTarget::DrawnShape
        );
        assert_eq!(
            PianoRollContextMenuItem::Delete.target(),
            ContextMenuTarget::Notes
        );
        assert_eq!(
            PianoRollContextMenuItem::SelectAll.target(),
            ContextMenuTarget::Notes
        );
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
    fn test_panel_menu_item_variants() {
        let _items = [PanelContextMenuItem::RecoverDeletedTrack];
    }

    #[test]
    fn test_material_menu_item_variants() {
        let _items = [
            MaterialContextMenuItem::Rename,
            MaterialContextMenuItem::Delete,
            MaterialContextMenuItem::UploadToCloud,
        ];
    }

    #[test]
    fn test_action_open_position() {
        let action = PianoRollContextMenuAction::Open {
            position: Point2::new(120.0, 80.0),
            target: ContextMenuTarget::Notes,
        };
        match action {
            PianoRollContextMenuAction::Open { position, target } => {
                assert!((position.x - 120.0).abs() < f32::EPSILON);
                assert!((position.y - 80.0).abs() < f32::EPSILON);
                assert_eq!(target, ContextMenuTarget::Notes);
            }
            _ => panic!("应为 Open 动作"),
        }
    }
}
