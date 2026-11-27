//! 框选交互的 UI 状态（T-30 slice 7）。
//!
//! 鼠标框选/弹簧动画/增量边界缓存同属一份状态：按下建立锚点、拖动更新
//! delta 边界、松手后动画收敛。集中到一个 bundle，`Editor` 只保留一个字段。

use std::cell::Cell;

use crate::SelectionBoxAnimState;

/// 框选交互状态。
#[derive(Debug, Default)]
pub(crate) struct SelectionUiState {
    /// 选择框边界缓存（raw 坐标），增量维护。
    bounds: Cell<Option<(f32, f32, u16, u16)>>,
    /// 上一帧框选矩形 raw 边界，用于计算增量 delta。
    cached_bounds: Cell<Option<(f32, f32, u16, u16)>>,
    /// 框选 X 向量化锚点（按下时 floor 到音符精度网格低边）。
    marquee_anchor: Option<f32>,
    /// 框选框弹簧动画显示状态。
    anim: Cell<Option<SelectionBoxAnimState>>,
}

impl SelectionUiState {
    /// 当前框选包围盒（raw 坐标）。
    pub(crate) fn bounds(&self) -> Option<(f32, f32, u16, u16)> {
        self.bounds.get()
    }

    /// 写入框选包围盒。
    pub(crate) fn set_bounds(&self, bounds: Option<(f32, f32, u16, u16)>) {
        self.bounds.set(bounds);
    }

    /// 上一帧框选包围盒（增量 delta 用）。
    pub(crate) fn cached_bounds(&self) -> Option<(f32, f32, u16, u16)> {
        self.cached_bounds.get()
    }

    /// 写入上一帧包围盒。
    pub(crate) fn set_cached_bounds(&self, bounds: Option<(f32, f32, u16, u16)>) {
        self.cached_bounds.set(bounds);
    }

    /// 清空上一帧包围盒。
    pub(crate) fn clear_cached_bounds(&self) {
        self.cached_bounds.set(None);
    }

    /// 框选 X 向量化锚点。
    pub(crate) fn marquee_anchor(&self) -> Option<f32> {
        self.marquee_anchor
    }

    /// 写入/清空框选锚点。
    pub(crate) fn set_marquee_anchor(&mut self, anchor: Option<f32>) {
        self.marquee_anchor = anchor;
    }

    /// 当前框选弹簧动画。
    pub(crate) fn anim(&self) -> Option<SelectionBoxAnimState> {
        self.anim.get()
    }

    /// 写入框选弹簧动画。
    pub(crate) fn set_anim(&self, anim: Option<SelectionBoxAnimState>) {
        self.anim.set(anim);
    }

    /// 清空框选弹簧动画。
    pub(crate) fn clear_anim(&self) {
        self.anim.set(None);
    }
}
