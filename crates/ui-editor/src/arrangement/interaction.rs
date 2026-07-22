//! 工程走带视图交互逻辑
//!
//! 移植自 yinhe 的 `arrange/view_ui.rs` 中的选择/移动/橡皮擦交互，
//! 以及 `selection/drag.rs` 中的拖拽自动滚动行为。
//! 本模块是无状态纯函数 + 状态结构体，负责把 iced canvas 事件翻译为
//! `Message`，由 `Root` 统一修改 `EditorData` / `ArrangementView`。

use std::time::Instant;

use iced_core::{Point, Rectangle, mouse};
use iced_widget::canvas;

use lumino_core::{NotePrecision, Tool};

use crate::Message;
use crate::arrangement::ArrangementViewport;

pub mod auto_scroll;
pub mod curve;
pub mod eraser;
pub mod geometry;
pub mod pointer;

pub use geometry::{arrange_snapped_bounds, snap_tick};

/// 工程走带交互状态。
///
/// 存储在 canvas 的 `Program::State` 中，随事件持续更新。
#[derive(Debug, Default, Clone)]
pub struct ArrangementInteractionState {
    /// 框选/选择框拖拽起点（音乐坐标 + 视图起点像素）
    pub drag: Option<((f64, f32), Point)>,
    /// 移动已有选择：((origin_tick, origin_track_f), (current_tick, current_track_f))
    pub move_drag: Option<((f64, f32), (f64, f32))>,
    /// 移动开始时保存的原始选择矩形
    pub move_orig_sel: Option<(f64, f64, usize, usize)>,
    /// 橡皮擦拖拽
    pub eraser_drag: Option<((f64, f32), Point)>,
    /// 切割工具拖拽
    pub razor_drag: Option<((f64, f32), Point)>,
    /// 当前鼠标是否在已有选择矩形内（仅用于 Pointer 工具光标）
    pub hover_inside_selection: bool,
    /// 当前鼠标位置（视图局部坐标）
    pub last_local_pos: Option<Point>,
    /// 上次自动滚动时间，用于计算时间步长
    pub last_auto_scroll_time: Option<Instant>,
    /// 主鼠标键（左键）当前是否处于按下状态，用于检测丢失的释放事件。
    pub primary_down: bool,
    /// 中键拖拽起始位置（视图局部坐标），用于平移视口。
    pub middle_drag: Option<Point>,
    /// Curve 工具拖拽绘制音符：起点（tick, track）与当前局部坐标。
    pub curve_drag: Option<((f64, usize), Point)>,
}

impl ArrangementInteractionState {
    /// 当前是否有未完成的拖拽
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
            || self.move_drag.is_some()
            || self.eraser_drag.is_some()
            || self.razor_drag.is_some()
            || self.curve_drag.is_some()
    }
}

/// 一次事件处理可能产生的多条消息。
pub type InteractionOutput = Vec<Message>;

/// 处理单个 canvas 事件，返回需要发布给 `Root` 的消息。
///
/// 参数说明：
/// - `state`：canvas 持久状态
/// - `event`：当前事件
/// - `bounds`：canvas 屏幕区域
/// - `cursor`：鼠标光标状态
/// - `viewport`：走带视口
/// - `current_tool`：当前工具
/// - `track_count`：当前总轨道数
/// - `arr_sel_rect`：当前已提交的选择矩形（来自 `ArrangementView`）
/// - `selected_notes`：当前选中的音符，用于生成 ghost 预览（start_tick, end_tick, track, key）
/// - `ppq`：每四分音符 tick 数
/// - `precision`：网格对齐精度
/// - `ctrl_pressed` / `shift_pressed`：修饰键状态
#[allow(clippy::too_many_arguments)]
pub fn handle_event(
    state: &mut ArrangementInteractionState,
    event: &canvas::Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
    viewport: &mut ArrangementViewport,
    current_tool: Tool,
    track_count: usize,
    arr_sel_rect: Option<(f64, f64, usize, usize)>,
    selected_notes: &[(f64, f64, usize, u8)],
    ppq: u16,
    precision: NotePrecision,
    ctrl_pressed: bool,
    shift_pressed: bool,
) -> InteractionOutput {
    puffin::profile_function!();

    let mut output = InteractionOutput::new();

    // 更新鼠标位置与 hover 状态
    if let Some(pos) = cursor.position() {
        state.last_local_pos = Some(geometry::local_pos(pos, bounds));
        state.hover_inside_selection = geometry::inside_selection_rect(
            state.last_local_pos.unwrap_or(Point::new(0.0, 0.0)),
            arr_sel_rect,
            viewport,
        );
    }

    match current_tool {
        Tool::Pointer => {
            output.extend(pointer::handle_pointer_event(
                state,
                event,
                bounds,
                cursor,
                viewport,
                track_count,
                arr_sel_rect,
                selected_notes,
                ppq,
                precision,
                ctrl_pressed,
                shift_pressed,
            ));
        }
        Tool::Eraser => {
            output.extend(eraser::handle_eraser_event(
                state,
                event,
                bounds,
                cursor,
                viewport,
                track_count,
                ppq,
                precision,
            ));
        }
        Tool::Curve => {
            output.extend(curve::handle_curve_event(
                state,
                event,
                bounds,
                cursor,
                viewport,
                track_count,
                ppq,
                precision,
            ));
        }
        _ => {}
    }

    output
}
