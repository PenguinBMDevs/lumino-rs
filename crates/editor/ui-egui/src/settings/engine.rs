//! 设置页行渲染引擎：布局常量、内容高度与行游标。
//!
//! 「音频」与「界面」两页的 PAD / GAP / TITLE_BLOCK 与逐行推进完全相同，
//! 差异只在行模型与控件绘制；布局单份留在本模块。

use egui::Rect;

/// 内容内边距。
pub(super) const PAD: f32 = 20.0;
/// 行间距。
pub(super) const GAP: f32 = 10.0;
/// 标题区高度。
pub(super) const TITLE_BLOCK: f32 = 44.0;

/// 页面内容总高度（供 `ScrollArea` 预留空间）。
pub(super) fn content_height(row_heights: impl Iterator<Item = f32>) -> f32 {
    PAD * 2.0 + TITLE_BLOCK + row_heights.map(|h| h + GAP).sum::<f32>()
}

/// 行游标：统一 x / y 起点、行推进与控件区宽度。
pub(super) struct RowCursor {
    x: f32,
    y: f32,
    right: f32,
    width: f32,
}

impl RowCursor {
    pub(super) fn new(rect: Rect) -> Self {
        Self {
            x: rect.left() + PAD,
            y: rect.top() + PAD + TITLE_BLOCK,
            right: rect.right() - PAD,
            width: rect.width(),
        }
    }

    pub(super) fn x(&self) -> f32 {
        self.x
    }

    pub(super) fn y(&self) -> f32 {
        self.y
    }

    /// 内容区右边界（已扣右内边距）。
    pub(super) fn right(&self) -> f32 {
        self.right
    }

    /// 内容区可用宽度（扣左右内边距）。
    pub(super) fn content_width(&self) -> f32 {
        (self.width - PAD * 2.0).max(1.0)
    }

    /// 行垂直中心。
    pub(super) fn center(&self, h: f32) -> f32 {
        self.y + h / 2.0
    }

    /// 控件区可用宽度（不小于 `min`）。
    pub(super) fn control_width(&self, min: f32) -> f32 {
        (self.right - self.x).max(min)
    }

    /// 推进到下一行。
    pub(super) fn advance(&mut self, h: f32) {
        self.y += h + GAP;
    }
}
