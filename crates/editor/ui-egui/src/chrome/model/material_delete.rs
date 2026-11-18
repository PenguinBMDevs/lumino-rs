//! 素材删除确认对话框模型（DRAW-045，独立 OS 窗口）。
//!
//! 文案全部由生产侧预格式化；egui 只负责排版与命中。

/// 素材删除确认对话框的只读模型。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterialDeleteModel {
    /// 顶部是否有 30px 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题（「删除素材」）。
    pub title: String,
    /// 素材显示名（主色）。
    pub name: String,
    /// 危险提示（「删除后无法恢复…」）。
    pub warning: String,
    /// 取消按钮文案。
    pub cancel: String,
    /// 删除按钮文案。
    pub delete: String,
    /// 待删除素材下标（动作回传用）。
    pub index: usize,
}
