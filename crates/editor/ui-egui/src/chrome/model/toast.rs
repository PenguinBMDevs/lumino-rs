//! 右下角浮层：Toast 通知 + 首次使用帮助提示。

use egui::Color32;

/// 单条 Toast（颜色由生产侧按主题算好）。
#[derive(Debug, Clone, PartialEq)]
pub struct ToastCardModel {
    /// 级别图标（`ℹ ⚠ ✗ ✓`）。
    pub icon: String,
    /// 文案。
    pub message: String,
    /// 卡片底色。
    pub background: Color32,
    /// 文字色。
    pub text: Color32,
}

/// Toast 叠加层；`cards` 为空表示不显示。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToastModel {
    /// 按添加顺序（自下而上渲染）。
    pub cards: Vec<ToastCardModel>,
}

/// 首次使用帮助提示（DRAW-038）。
#[derive(Debug, Clone, PartialEq)]
pub struct HelpTipModel {
    /// 标题。
    pub title: String,
    /// 首段正文。
    pub body: String,
    /// 「查看帮助」按钮文案。
    pub help_label: String,
    /// 「知道了」按钮文案。
    pub dismiss_label: String,
}
