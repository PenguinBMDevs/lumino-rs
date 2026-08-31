//! 自动化面板 — iced 迁移桩（P4）
//!
//! 对应 yinhe `piano_view/automation_panel/` 15 文件（`interaction.rs 1422` +
//! `render 292` + `velocity 391` 等）的 egui 实现；lumino 侧以
//! `iced::canvas::Program` 重写，数据与渲染走
//! `lumino_note_core::{AutomationLane, AutomationEvent, SegmentShape}` +
//! `lumino_gfx::{CcBarRenderer, AutomationViewParams, build_lane_instances}`
//! 全链路，不自建 wgpu，不引 egui。
//!
//! 文件布局（与任务要求一致）：
//! - `constants.rs` — 常量（命中半径/分割条/阈值等）
//! - `layout.rs`    — 布局几何与滚动（iced `Rectangle` 版本）
//! - `types.rs`     — 面板视图/选框/交互上下文等类型（复用 `AutomationLane` 等）
//! - `render.rs`    — `canvas::Program` 绘制曲线/柱状 + `CcBarRenderer` 实例构建
//! - `interaction/mod.rs` — 锚点拖拽/曲线控制点/选框（iced `Event` 驱动）
//! - `velocity.rs`  — 力度柱笔划交互（插值命中 + 预览）
//! - `value.rs`     — 值域/缩放计算
//! - `widgets.rs`   — 分割条/目标下拉/切换按钮

#![allow(clippy::module_inception)]

pub mod constants;
pub mod interaction;
pub mod layout;
pub mod render;
pub mod types;
pub mod value;
pub mod velocity;
pub mod widgets;

// ── 公开重导出（便于 `piano_view` 与上层 `EditorHost` 组合） ─────────────

pub use constants::{
    ANCHOR_HIT_PX, AUTOMATION_TARGETS, DEFAULT_PANEL_HEIGHT, HOVER_DELAY, MARQUEE_THRESHOLD,
    MAX_PANEL_HEIGHT, MIN_PANEL_HEIGHT, SPLIT_H,
};
pub use interaction::{AutoDrag, SelOp, SelRectOp};
pub use layout::{
    FrameCtx, begin_frame, make_panels_layout, panel_rects, sync_panels_from_pianoroll,
};
pub use render::{
    AutomationPanelProgram, AutomationPanelProgramState, build_instances_for_lane,
    view_params_for_panel,
};
pub use types::{
    AnchorSelRect, AutomationGhost, AutomationPanelView, ControlPointHit, CtrlEnd, HoverTooltip,
    PanelInteractionOut, PanelOverlayData, PanelPianorollFeedback, PanelsCfg, PanelsData,
    PanelsLayout, TimelineViewBase, Tool,
};
pub use value::{min_value_zoom, panel_max_val, panel_max_val_simple, value_upper_bound};
pub use velocity::{VelocityEdit, VelocityHover, VelocityPreview, VelocityStroke};

/// 自动化面板集合的 iced 入口（多面板 `Column` 组合的最小桩）。
///
/// 上层 `piano_view` 将 `ViewState` 的滚动/缩放同步到各 `AutomationPanelView::base`，
/// 并以 `AutomationPanelProgram` 逐面板嵌入 `canvas::Canvas`。
#[derive(Debug, Default)]
pub struct AutomationPanelSet {
    pub panels: Vec<AutomationPanelView>,
    pub scroll_y: f32,
}

impl AutomationPanelSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, panel: AutomationPanelView) {
        self.panels.push(panel);
    }

    pub fn len(&self) -> usize {
        self.panels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }

    /// 由 `ViewState` 同步水平状态到全部面板。
    pub fn sync_from_view_state(
        &mut self,
        scroll_x: f32,
        pixels_per_tick: f32,
        left_panel_width: f32,
    ) {
        sync_panels_from_pianoroll(
            &mut self.panels,
            scroll_x,
            pixels_per_tick,
            left_panel_width,
        );
    }
}
