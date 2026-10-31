//! 形状抽屉库/变换/插入动作（DRAW-033B2）
//!
//! 画布工具与拖拽仍由 `DrawerDoc`/Canvas 直接处理；库列表、保存、变换与
//! 插入工程走本动作枚举，避免把文件 I/O 塞进 canvas `update`。

/// 形状抽屉动作
#[derive(Debug, Clone, PartialEq)]
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
    /// 开始拖动库/画布分割线
    PanelDragStart,
    /// 结束拖动库/画布分割线
    PanelDragEnd,
    /// 设置库面板宽度（拖动分割线）
    SetPanelWidth(f32),
    /// 切换区域着色模式
    ToggleAreaMode,
    /// 设置区域色号（0..=15）
    SetAreaColour(u8),
    /// 清空当前图形的区域点
    ClearAreas,
    /// 插入宽度输入（拍）
    SetInsertBeats(String),
    /// 插入高度输入（key）
    SetInsertKeys(String),
    /// 切换插入比例（拉伸 / 保持比例）
    ToggleInsertFit,
    /// 抽屉内缩放百分比输入
    SetScalePercent(String),
    /// 应用抽屉内缩放
    ApplyScale,
    /// 切换选中锚点的 Smooth/Corner
    ToggleAnchorBend,
}
