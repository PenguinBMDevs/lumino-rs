//! 形状抽屉库/变换/插入动作（DRAW-033B2）
//!
//! 画布工具与拖拽仍由 `DrawerDoc`/Canvas 直接处理；库列表、保存、变换与
//! 插入工程走本动作枚举，避免把文件 I/O 塞进 canvas `update`。

/// 形状抽屉动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeDrawerAction {
    /// 修改名称输入框
    SetName(String),
    /// 选中库内图形
    Select(String),
    /// 新建空画板
    New,
    /// 保存当前图形
    Save,
    /// 用当前 id 重命名（保存）
    Rename,
    /// 删除当前图形
    Delete,
    /// 复制选中 strokes 到抽屉剪贴板
    Copy,
    /// 粘贴抽屉剪贴板
    Paste,
    /// 水平翻转
    FlipHorizontal,
    /// 垂直翻转
    FlipVertical,
    /// 顺时针旋转 90°
    Rotate90,
    /// 插入当前图形到工程
    Insert,
}
