//! 画刷「绘制行为」对话框动作
//!
//! 镜像 `custom_precision`：主窗侧 `OpenDialog` 触发 Runner 打开独立 OS 窗口，
//! 对话框内部编辑本地草稿（`RootState::brush_settings_draft`），`Save` 通过
//! `DialogResult::BrushSettings` 回传主窗应用。

use lumino_core::BrushConfig;

/// 画刷「绘制行为」对话框动作
#[derive(Debug, Clone)]
pub enum BrushSettingsAction {
    /// 打开对话框（主窗侧，携带当前画刷配置）
    OpenDialog(BrushConfig),
    /// 关闭对话框
    CloseDialog,
    /// 保存并应用配置
    Save,
    /// 取消
    Cancel,
    /// 粗细度变更（1-20）
    ThicknessChanged(u8),
    /// 设置某层音轨（level 0-based，None=默认自动分配）
    LevelTrackChanged(usize, Option<usize>),
    /// 在 level 之后插入新层
    AddLevel(usize),
    /// 删除 level 层（保留至少 1 层）
    RemoveLevel(usize),
}
