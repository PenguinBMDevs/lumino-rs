//! 自动化面板类型 — 对应 yinhe `automation_panel/types.rs`
//!
//! 复用 `lumino_note_core::{AutomationLane, AutomationEvent, SegmentShape}`，
//! 不新定义这些类型；面板视图状态 `AutomationPanelView / AnchorSelRect` 为 UI 层
//! 本地类型（lumino 侧尚未在 note-core 中提供面板持久化模型）。

use iced_core::{Point, Rectangle};

use lumino_note_core::{AutomationLane, AutomationTarget, SegmentShape};

// ── 面板持久化模型（对齐 yinhe `automation_panel_view.rs`） ───────────────

/// 持久化的锚点选框（音乐坐标），与 yinhe `AnchorSelRect` 语义一致。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnchorSelRect {
    /// tick 范围起点（f64 便于与 ViewState 的 f64 tick 互操作）
    pub tick_start: f64,
    /// tick 范围终点
    pub tick_end: f64,
    /// value 范围；`None` = 垂直全选（仅按 tick 匹配，SelectVertical 语义）
    pub value_range: Option<(f32, f32)>,
}

impl AnchorSelRect {
    /// 判断 `(tick, value)` 是否落入选框。
    #[must_use]
    pub fn contains(&self, tick: u32, value: f32) -> bool {
        let ts = self.tick_start.min(self.tick_end);
        let te = self.tick_start.max(self.tick_end);
        let tick_in = (tick as f64) >= ts && (tick as f64) <= te;
        let value_in = match self.value_range {
            None => true,
            Some((vmin, vmax)) => {
                let lo = vmin.min(vmax);
                let hi = vmin.max(vmax);
                value >= lo && value <= hi
            }
        };
        tick_in && value_in
    }
}

/// 水平时间轴共享状态（对齐 yinhe `TimelineViewBase` 的最小集，复用 `ViewState` 语义）。
#[derive(Clone, Debug)]
pub struct TimelineViewBase {
    /// 每 tick 像素数（= `ViewState::zoom_x`）
    pub pixels_per_tick: f32,
    /// 水平滚动（像素，= `ViewState::scroll_x`）
    pub scroll_x: f32,
    /// 垂直滚动占位（面板内未使用，保留与 yinhe 字段对齐）
    pub scroll_y: f32,
    /// 左侧键盘/标签列宽度（= `ViewState::keyboard_width`）
    pub left_panel_width: f32,
    pub dirty: bool,
}

impl Default for TimelineViewBase {
    fn default() -> Self {
        Self {
            pixels_per_tick: 0.15,
            scroll_x: 0.0,
            scroll_y: 0.0,
            left_panel_width: 60.0,
            dirty: true,
        }
    }
}

/// 单个自动化面板的视图状态（对齐 yinhe `AutomationPanelView`）。
#[derive(Clone, Debug)]
pub struct AutomationPanelView {
    /// 水平时间轴共享状态（每帧由 `ViewState` 同步）
    pub base: TimelineViewBase,
    /// 面板高度（像素）
    pub panel_height: f32,
    /// 当前显示的自动化目标；`show_velocity = true` 时忽略
    pub selected_target: AutomationTarget,
    /// 为 `true` 时渲染力度柱状（`VelocityPoint`），否则渲染 `AutomationLane`
    pub show_velocity: bool,
    /// 在 `automation_lanes` 中的缓存索引（加速查找，失效时回退线性查找）
    pub lane_index: usize,
    /// 内容是否需要重建（触发 `CcBarRenderer::prepare` / canvas 重绘）
    pub dirty: bool,
    /// 垂直缩放（1.0 = 满量程映射到面板高度）
    pub value_zoom: f32,
    /// 垂直滚动偏移（值空间单位，面板顶部对应值）
    pub value_scroll: f32,
    /// 在宿主纹理中的 y 偏移（多面板堆叠时使用；单面板为 0）
    pub y_offset: f32,
    /// 持久化选框列表（支持多选框，Shift 追加）
    pub anchor_sel_rects: Vec<AnchorSelRect>,
}

impl Default for AutomationPanelView {
    fn default() -> Self {
        Self {
            base: TimelineViewBase::default(),
            panel_height: crate::piano_view::automation_panel::constants::DEFAULT_PANEL_HEIGHT,
            selected_target: AutomationTarget::CC { controller: 7 },
            show_velocity: true,
            lane_index: 0,
            dirty: true,
            value_zoom: 1.0,
            value_scroll: 0.0,
            y_offset: 0.0,
            anchor_sel_rects: Vec::new(),
        }
    }
}

impl AutomationPanelView {
    /// 由 `ViewState` 同步水平状态（`scroll_x / zoom_x / keyboard_width`）。
    pub fn sync_from_view_state(
        &mut self,
        scroll_x: f32,
        pixels_per_tick: f32,
        left_panel_width: f32,
    ) {
        if self.base.scroll_x != scroll_x
            || self.base.pixels_per_tick != pixels_per_tick
            || self.base.left_panel_width != left_panel_width
        {
            self.base.scroll_x = scroll_x;
            self.base.pixels_per_tick = pixels_per_tick;
            self.base.left_panel_width = left_panel_width;
            self.dirty = true;
        }
    }

    /// 将值映射到面板本地 y（像素，含 `y_offset`），与 yinhe `value_to_y` 一致。
    #[must_use]
    pub fn value_to_y(&self, value: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return self.y_offset;
        }
        let h = self.panel_height;
        self.y_offset + h - ((value - self.value_scroll) / visible_range) * h
    }

    /// 将面板本地 y（像素，含 `y_offset`）映射回值，与 yinhe `y_to_value` 一致。
    #[must_use]
    pub fn y_to_value(&self, y: f32, max_val: f32) -> f32 {
        let visible_range = max_val / self.value_zoom;
        if visible_range <= 0.0 {
            return 0.0;
        }
        let h = self.panel_height;
        self.value_scroll + (1.0 - (y - self.y_offset) / h) * visible_range
    }

    /// 按 `max_val` 钳制 `value_scroll`。
    pub fn clamp_value_scroll(&mut self, max_val: f32) {
        let visible_range = max_val / self.value_zoom;
        let max_scroll = (max_val - visible_range).max(0.0);
        self.value_scroll = self.value_scroll.clamp(0.0, max_scroll);
    }
}

// ── 交互上下文与布局 ─────────────────────────────────────────────────

/// 工具类型（与 `yinhe` 的 `Tool` 对齐的最小集，供 dispatch/交互分支使用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    SelectVertical,
    Pencil,
    Curve,
    Eraser,
}

/// 自动化编辑上下文（打包 lane/quantize 相关的只读输入）。
#[derive(Clone, Copy, Debug)]
pub struct AutomationEditCtx<'a> {
    pub active_tool: Tool,
    pub active_track: Option<u16>,
    pub ppq: u32,
    pub snap_ticks: Option<u32>,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

/// 面板集合布局几何（iced `Rectangle` 版本）。
#[derive(Clone, Copy, Debug)]
pub struct PanelsLayout {
    pub combo_width: f32,
    pub content_rect: Rectangle,
    pub panels_visible_h: f32,
}

/// 面板渲染配置（与 pianoroll 的滚动/缩放联动）。
#[derive(Clone, Copy, Debug)]
pub struct PanelsCfg {
    pub pianoroll_scroll_x: f32,
    pub pianoroll_ppt: f32,
    pub min_border_width: f32,
    pub revision: u64,
    pub editing_is_conductor: bool,
}

/// 面板模型只读数据（由 `EditorState` / `EditorData` 注入）。
pub struct PanelsData<'a> {
    pub automation_lanes: &'a [AutomationLane],
    pub render_lanes: &'a [&'a AutomationLane],
    pub midi_notes_len: Option<usize>,
    pub track_visible: &'a [bool],
    pub track_colors: &'a [[f32; 4]],
}

/// 由 `automation_panel` 返回、需回写到 `ViewState` 的 pianoroll 联动反馈。
#[derive(Clone, Debug, Default)]
pub struct PanelPianorollFeedback {
    pub scroll_x_delta: f32,
    pub zoom_factor: f32,
    pub zoom_center_x: f32,
    pub status_hint: Option<String>,
}

/// 当帧的临时 overlay（选框矩形 + velocity 笔划预览）。
#[derive(Clone, Debug, Default)]
pub struct PanelOverlayData {
    pub marquee_rect: Option<Rectangle>,
    pub velocity_preview: Option<crate::piano_view::automation_panel::velocity::VelocityPreview>,
}

/// 单个面板的编辑输出（由 `interaction` 产生，`render` 消费 ghost/preview）。
pub struct PanelInteractionOut {
    pub automation_edits: Vec<lumino_note_core::AutomationEdit>,
    pub velocity_edits: Vec<crate::piano_view::automation_panel::velocity::VelocityEdit>,
    /// 自动化 lane 的临时 ghost（拖拽中由 `CcBarRenderer` / `AutomationLane` 叠加绘制）
    pub ghost: Option<AutomationGhost>,
    pub preview: Option<crate::piano_view::automation_panel::velocity::VelocityPreview>,
    /// 锚点拖拽的实时 `(tick, value)`，供状态栏/信息面板显示
    pub anchor_drag: Option<(u32, f32)>,
    pub marquee_rect: Option<Rectangle>,
    pub sel_op: Option<crate::piano_view::automation_panel::interaction::SelOp>,
}

/// 自动化 ghost（拖拽中整 lane 预览，不落盘）。
#[derive(Clone, Debug)]
pub enum AutomationGhost {
    /// 锚点移动/复制/形状编辑后的整 lane 覆盖
    Move {
        lane: AutomationLane,
        color: [f32; 3],
    },
    /// Curve 工具两点拖拽的临时直线段
    Curve {
        start: Point,
        end: Point,
        color: [f32; 3],
    },
}

/// Hover/drag tooltip 数据（与 yinhe `HoverTooltip` 对齐，供 canvas 层绘制文本）。
#[derive(Clone, Copy, Debug)]
pub enum HoverTooltip {
    Anchor {
        tick: u32,
        value: f32,
        pos: Point,
    },
    ControlPoint {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        pos: Point,
    },
}

/// 控制点端别（三次贝塞尔 P1/P2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtrlEnd {
    Out,
    In,
}

/// 命中控制点的结果（`dist_sq` 仅用于择优，调用方不直接消费）。
#[derive(Clone, Copy, Debug)]
pub struct ControlPointHit {
    pub prev_tick: u32,
    pub which: CtrlEnd,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub pos: Point,
    pub dist_sq: f32,
}

/// 未使用的占位，与 yinhe `SegmentShape` 的曲线参数对齐：
///
/// lumino 侧 `SegmentShape::Curve { tension: i8 }` 与 yinhe 的
/// `Curve { x1,y1,x2,y2 }` 模型不同，此处保留 `x1/y1/x2/y2` 用于
/// canvas 层的控制点命中/拖拽几何计算，落盘时映射为 `tension`。
#[allow(dead_code)]
pub fn curve_shape_with_tension(tension: i8) -> SegmentShape {
    SegmentShape::Curve { tension }
}
