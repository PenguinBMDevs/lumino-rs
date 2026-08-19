//! 力度 / CC / Tempo Canvas 状态定义

use std::collections::HashMap;
use std::time::Instant;

use super::bend_path::BendPathState;

/// 自动化编辑拖拽状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationDrag {
    /// Curve 工具：拖拽锚点，`old_tick` 为原始 tick。
    MoveAnchor {
        /// 拖拽前锚点的原始 tick
        old_tick: u32,
    },
    /// Curve 工具：从起点绘制到当前点。
    CurveDraw {
        /// 绘制起点 tick
        start_tick: u32,
        /// 绘制起点力度/数值
        start_value: u16,
    },
}

/// 力度 / CC / Tempo Canvas 状态
#[derive(Debug)]
pub struct VelocityCanvasState {
    /// 当前正在拖拽的力度点索引（在 points 中的索引，非 note_index）
    pub drag_point_idx: Option<usize>,
    /// 拖拽开始时的力度值（用于 undo）
    pub _drag_start_velocity: u8,
    /// 当前悬停的力度点索引
    pub hover_point_idx: Option<usize>,
    /// Canvas 是否已初始化尺寸
    pub _initialized: bool,
    /// 是否在拖拽 resize 手柄
    pub resize_dragging: bool,
    /// resize 拖拽起始 Y 坐标（绝对屏幕坐标）
    pub resize_drag_start_y: f32,
    /// resize 拖拽开始时的面板高度
    pub resize_start_height: f32,
    /// 鼠标是否悬停在 resize 手柄区域
    pub hover_resize_handle: bool,
    /// 是否正在力度曲线绘制模式
    pub curve_active: bool,
    /// 力度曲线绘制起始 X 坐标（local）
    pub curve_start_x: f32,
    /// 力度曲线绘制起始 Y 对应的力度值
    pub curve_start_velocity: u8,
    /// 当前笔触影响的音符索引 → 新力度值
    pub curve_affected: HashMap<usize, u8>,
    /// 自动化编辑拖拽状态（Curve 工具）。
    pub automation_drag: Option<AutomationDrag>,
    /// 当前悬停的自动化锚点 tick。
    pub hover_anchor_tick: Option<u32>,
    /// 上次左键点击时间与位置，用于检测双击。
    pub last_click: Option<(Instant, iced_core::Point)>,
    /// 自动化曲线绘制时的当前 ghost 值（仅用于绘制反馈）。
    pub automation_curve_current: Option<(u32, u16)>,
    /// 当前键盘修饰键状态，用于滚轮缩放判断。
    pub modifiers: iced_core::keyboard::Modifiers,
    /// 当前拖拽的 tempo 点索引
    pub tempo_drag_idx: Option<usize>,
    /// 当前悬停的 tempo 点索引
    pub tempo_hover_idx: Option<usize>,
    /// 弯音贝塞尔路径编辑状态（Bend 模式 Curve 工具，全部操作实时生效）
    pub bend_path: BendPathState,
}

impl Default for VelocityCanvasState {
    fn default() -> Self {
        Self::new()
    }
}

impl VelocityCanvasState {
    /// 创建新的 Canvas 状态
    pub fn new() -> Self {
        Self {
            drag_point_idx: None,
            _drag_start_velocity: 0,
            hover_point_idx: None,
            _initialized: false,
            resize_dragging: false,
            resize_drag_start_y: 0.0,
            resize_start_height: 0.0,
            hover_resize_handle: false,
            curve_active: false,
            curve_start_x: 0.0,
            curve_start_velocity: 0,
            curve_affected: HashMap::new(),
            automation_drag: None,
            hover_anchor_tick: None,
            last_click: None,
            automation_curve_current: None,
            modifiers: iced_core::keyboard::Modifiers::default(),
            tempo_drag_idx: None,
            tempo_hover_idx: None,
            bend_path: BendPathState::default(),
        }
    }

    /// 检测双击：第二次按下与第一次释放之间的时间与距离阈值。
    pub fn detect_double_click(&mut self, pos: iced_core::Point) -> bool {
        const DOUBLE_CLICK_MS: u128 = 300;
        const DOUBLE_CLICK_DIST_PX: f32 = 5.0;

        let now = Instant::now();
        if let Some((last_time, last_pos)) = self.last_click {
            let dt = now.duration_since(last_time).as_millis();
            let dist = ((pos.x - last_pos.x).powi(2) + (pos.y - last_pos.y).powi(2)).sqrt();
            self.last_click = Some((now, pos));
            dt <= DOUBLE_CLICK_MS && dist <= DOUBLE_CLICK_DIST_PX
        } else {
            self.last_click = Some((now, pos));
            false
        }
    }

    /// 记录一次单击，用于后续双击检测。
    pub fn record_click(&mut self, pos: iced_core::Point) {
        self.last_click = Some((Instant::now(), pos));
    }

    /// 重置力度曲线绘制状态
    pub fn reset_velocity_curve(&mut self) {
        self.curve_active = false;
        self.curve_affected.clear();
    }

    /// 重置自动化编辑拖拽状态
    pub fn reset_automation_drag(&mut self) {
        self.automation_drag = None;
        self.automation_curve_current = None;
    }

    /// 设置 Curve 工具移动锚点拖拽。
    pub fn start_move_anchor(&mut self, old_tick: u32) {
        self.automation_drag = Some(AutomationDrag::MoveAnchor { old_tick });
    }

    /// 设置 Curve 工具绘制。
    pub fn start_curve_draw(&mut self, start_tick: u32, start_value: u16) {
        self.automation_drag = Some(AutomationDrag::CurveDraw {
            start_tick,
            start_value,
        });
    }

    /// 当前是否处于自动化拖拽中。
    pub fn is_automation_dragging(&self) -> bool {
        self.automation_drag.is_some()
    }
}
