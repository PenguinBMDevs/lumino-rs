//! 滚动条：绘制 + 拖拽（滑块滚动 / 边缘缩放），行为对齐 上一代 `ScrollbarWidget`。
//!
//! 常量与 上一代 侧同源（`lumino_ui_core::constants::scrollbar`）：
//! 边缘命中 6px、滑块最小 20px、轨道内边距 2px。
//! 交互状态存在 egui 临时数据里（按滚动条 id 区分，跨帧保持）。

use egui::{CursorIcon, Rect, Vec2};

use crate::theme;

/// 边缘检测宽度（与 `scrollbar::EDGE_WIDTH_PX` 一致）。
const EDGE_WIDTH: f32 = 6.0;
/// 滑块最小尺寸（与 `scrollbar::THUMB_MIN_SIZE_PX` 一致）。
const THUMB_MIN: f32 = 20.0;
/// 轨道内边距（与 `scrollbar::TRACK_THUMB_GAP` 一致）。
const TRACK_GAP: f32 = 2.0;
/// 滑块与轨道边缘的间距（与 `scrollbar::THUMB_TRACK_EDGE_GAP` 一致）。
const THUMB_EDGE_GAP: f32 = 2.0;

/// 滚动条几何与取值。
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarModel {
    /// 整条滚动条的屏幕矩形。
    pub rect: Rect,
    /// 当前滚动位置（像素）。
    pub scroll: f32,
    /// 内容总长度（像素）。
    pub max_scroll: f32,
    /// 当前缩放倍率。
    pub zoom: f32,
    /// 视口尺寸；`None` 时以轨道长度代替（上一代 同口径）。
    pub viewport: Option<f32>,
    /// 滑块长度占轨道比例；`Some` 时优先于 `viewport/max_scroll` 计算
    /// （草稿场景与生产 上一代 基线的显式比例）。
    pub thumb_ratio: Option<f32>,
    /// 是否垂直方向。
    pub vertical: bool,
}

/// 滚动条动作。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarAction {
    /// 滚动到指定像素位置。
    Scroll(f32),
    /// 边缘拖拽缩放：目标缩放 + 固定比例（0 = 起始边，1 = 结束边）。
    Zoom {
        /// 目标缩放倍率。
        zoom: f32,
        /// 固定比例。
        fixed_ratio: f32,
    },
}

/// 带方向的滚动条动作（滚动条本体不知道要发哪条 `Message`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectedAction {
    /// 是否垂直滚动条。
    pub vertical: bool,
    /// 动作。
    pub action: ScrollbarAction,
}

const STATE_ID: &str = "lumino.scrollbar.state";
const ACTION_ID: &str = "lumino.scrollbar.action";

/// 拖拽状态。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum DragState {
    #[default]
    Idle,
    Dragging {
        start_pos: f32,
        start_scroll: f32,
    },
    DraggingEdge {
        start_pos: f32,
        start_zoom: f32,
        start_thumb: f32,
        /// `true` = 结束边（`edge_at` 的返回值语义）。
        edge_is_end: bool,
    },
}

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, vertical: bool, action: ScrollbarAction) {
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(ACTION_ID),
            Some(DirectedAction { vertical, action }),
        )
    });
}

/// 直接写入带方向的滚动条动作（纵向卷帘的时间轴需要先取反再上报）。
pub fn push_directed(ctx: &egui::Context, directed: DirectedAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(directed)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<DirectedAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<DirectedAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

impl ScrollbarModel {
    fn track_len(&self) -> f32 {
        let len = if self.vertical {
            self.rect.height()
        } else {
            self.rect.width()
        };
        (len - TRACK_GAP * 2.0).max(0.0)
    }

    fn effective_viewport(&self, track: f32) -> f32 {
        self.viewport.unwrap_or(track)
    }

    fn actual_max_scroll(&self, track: f32) -> f32 {
        (self.max_scroll - self.effective_viewport(track)).max(0.0)
    }

    fn content_fits(&self, track: f32) -> bool {
        self.max_scroll <= self.effective_viewport(track)
    }

    /// `(轨道长度, 滑块长度, 滑块矩形)`。
    fn thumb(&self) -> (f32, f32, Rect) {
        let track = self.track_len();
        let viewport = self.effective_viewport(track);
        let scrollable = self.actual_max_scroll(track);
        let thumb_len = if let Some(ratio) = self.thumb_ratio {
            (track * ratio).max(THUMB_MIN).min(track)
        } else if scrollable <= 0.0 {
            track
        } else {
            (track * viewport / self.max_scroll.max(f32::EPSILON))
                .max(THUMB_MIN)
                .min(track)
        };
        let clamped = self.scroll.clamp(0.0, scrollable);
        let offset = THUMB_EDGE_GAP + (clamped / scrollable.max(1.0)) * (track - thumb_len);
        let thumb = if self.vertical {
            Rect::from_min_size(
                egui::pos2(self.rect.left() + THUMB_EDGE_GAP, self.rect.top() + offset),
                Vec2::new((self.rect.width() - TRACK_GAP * 2.0).max(0.0), thumb_len),
            )
        } else {
            Rect::from_min_size(
                egui::pos2(self.rect.left() + offset, self.rect.top() + THUMB_EDGE_GAP),
                Vec2::new(thumb_len, (self.rect.height() - TRACK_GAP * 2.0).max(0.0)),
            )
        };
        (track, thumb_len, thumb)
    }

    /// 命中滑块的起始/结束边缘。
    fn edge_at(&self, pos: egui::Pos2, thumb: Rect) -> Option<bool> {
        let (start, end) = if self.vertical {
            (
                pos.y >= thumb.top() && pos.y <= thumb.top() + EDGE_WIDTH,
                pos.y >= thumb.bottom() - EDGE_WIDTH && pos.y <= thumb.bottom(),
            )
        } else {
            (
                pos.x >= thumb.left() && pos.x <= thumb.left() + EDGE_WIDTH,
                pos.x >= thumb.right() - EDGE_WIDTH && pos.x <= thumb.right(),
            )
        };
        if start {
            Some(false)
        } else if end {
            Some(true)
        } else {
            None
        }
    }
}

/// 绘制并处理一条滚动条的输入，返回本帧动作（同时写入动作通道）。
pub fn show(ui: &egui::Ui, id: &str, model: &ScrollbarModel) -> Option<ScrollbarAction> {
    let action = show_raw(ui, id, model);
    if let Some(action) = action {
        push_action(ui.ctx(), model.vertical, action);
    }
    action
}

/// 同 [`show`]，但不写动作通道（调用方自行映射到自己的消息）。
pub fn show_raw(ui: &egui::Ui, id: &str, model: &ScrollbarModel) -> Option<ScrollbarAction> {
    if model.rect.width() <= 0.0 || model.rect.height() <= 0.0 {
        return None;
    }
    // 底：轨道；上：滑块。
    ui.painter()
        .rect_filled(model.rect, 0, theme::selected_bg());
    let (track, thumb_len, thumb) = model.thumb();
    ui.painter().rect_filled(thumb, 0, theme::toolbar_bg());

    let ctx = ui.ctx();
    // 图层归属判定：本控件用原始指针状态驱动（跨帧拖拽），不走 `ui.interact` 的
    // 命中集合；若不做这一步，更高图层（`Foreground` 下拉菜单 / 弹层）盖住滚动条时，
    // 同一次按压会既操作弹层又拖动下层滚动条，表现为「菜单点击穿透」。
    // `contains_pointer` 由 egui 命中测试得出：被更高图层覆盖时为 `false`。
    let hit = ui.interact(
        model.rect,
        egui::Id::unique(("lumino.scrollbar.hit", id)),
        egui::Sense::hover(),
    );
    let pos = ctx.input(|i| i.pointer.interact_pos());
    let pressed = ctx.input(|i| i.pointer.primary_pressed());
    let released = ctx.input(|i| i.pointer.primary_released());
    let down = ctx.input(|i| i.pointer.primary_down());

    let state_id = egui::Id::unique((STATE_ID, id));
    let mut state: DragState = ctx.data(|d| d.get_temp(state_id)).unwrap_or_default();
    let mut action = None;

    let pos = pos?;
    let inside = model.rect.contains(pos);
    // `Area` 边缘的命中半径内，两层可能同时进入命中集合；再按图层顺序兜一层，
    // 保证弹层边缘这一圈也由弹层独占（否则边缘几像素仍能拖动滚动条）。
    let overlay_above = ctx
        .layer_id_at(pos)
        .is_some_and(|layer| layer.order > ui.layer_id().order);
    let pointer_owned = hit.contains_pointer() && !overlay_above;

    if pressed && inside && pointer_owned {
        action = start_drag(model, pos, track, thumb_len, thumb, &mut state);
        ctx.data_mut(|d| d.insert_temp(state_id, state));
    } else if down {
        let (next_action, rebased) = continue_drag(model, pos, track, thumb_len, &mut state);
        action = next_action;
        if rebased {
            ctx.data_mut(|d| d.insert_temp(state_id, state));
        }
    }

    if released && !matches!(state, DragState::Idle) {
        state = DragState::Idle;
        ctx.data_mut(|d| d.insert_temp(state_id, state));
    }

    // 光标反馈（仅悬停且本层拥有指针时：被弹层盖住不得显示滚动光标）。
    if inside && pointer_owned {
        ctx.set_cursor_icon(hover_cursor(model, state, pos, thumb));
    }

    action
}

/// 按下时的拖拽起点：命中滑块进拖拽（边缘为缩放），否则点击轨道跳转。
/// 返回本帧动作（轨道点击时给出目标位置）。
fn start_drag(
    model: &ScrollbarModel,
    pos: egui::Pos2,
    track: f32,
    thumb_len: f32,
    thumb: Rect,
    state: &mut DragState,
) -> Option<ScrollbarAction> {
    let start_pos = if model.vertical { pos.y } else { pos.x };
    if thumb.contains(pos) {
        if let Some(edge) = model.edge_at(pos, thumb) {
            *state = DragState::DraggingEdge {
                start_pos,
                start_zoom: model.zoom,
                start_thumb: thumb_len,
                edge_is_end: edge,
            };
        } else {
            *state = DragState::Dragging {
                start_pos,
                start_scroll: model.scroll.clamp(0.0, model.actual_max_scroll(track)),
            };
        }
        return None;
    }

    // 点击轨道：直接跳到对应比例。
    let relative = if model.vertical {
        (pos.y - model.rect.top() - THUMB_EDGE_GAP) / (track - thumb_len).max(1.0)
    } else {
        (pos.x - model.rect.left() - THUMB_EDGE_GAP) / (track - thumb_len).max(1.0)
    };
    let target = relative.clamp(0.0, 1.0) * model.actual_max_scroll(track);
    Some(ScrollbarAction::Scroll(target))
}

/// 按住期间的拖拽推进；第二个返回值表示拖拽基线被重写。
fn continue_drag(
    model: &ScrollbarModel,
    pos: egui::Pos2,
    track: f32,
    thumb_len: f32,
    state: &mut DragState,
) -> (Option<ScrollbarAction>, bool) {
    let current = if model.vertical { pos.y } else { pos.x };
    match *state {
        DragState::Dragging {
            start_pos,
            start_scroll,
        } => {
            let delta = current - start_pos;
            let scrollable = model.actual_max_scroll(track);
            let ratio = delta / (track - thumb_len).max(1.0);
            let target = (start_scroll + ratio * scrollable).clamp(0.0, scrollable);
            (Some(ScrollbarAction::Scroll(target)), false)
        }
        DragState::DraggingEdge {
            start_pos,
            start_zoom,
            start_thumb,
            edge_is_end,
        } => {
            let delta = current - start_pos;
            // 与 上一代 一致：结束边向右/下拖为放大，起始边反向。
            let effective = if edge_is_end { delta } else { -delta };
            let content_fits = model.content_fits(track);
            if content_fits && effective > 0.0 {
                // 内容已满屏：阻止继续缩小，基线跟随当前位置避免抽动。
                *state = DragState::DraggingEdge {
                    start_pos: current,
                    start_zoom: model.zoom,
                    start_thumb,
                    edge_is_end,
                };
                (None, true)
            } else {
                let max_delta = (track - start_thumb).max(track * 0.5);
                let clamped = effective.clamp(-track * 0.9, max_delta);
                let ratio = (1.0 + clamped / start_thumb.max(1.0)).max(0.05);
                let zoom = start_zoom / ratio;
                (
                    Some(ScrollbarAction::Zoom {
                        zoom,
                        fixed_ratio: if edge_is_end { 0.0 } else { 1.0 },
                    }),
                    false,
                )
            }
        }
        DragState::Idle => (None, false),
    }
}

/// 悬停光标：拖拽中或命中边缘为缩放，命中滑块为抓取，其余为默认。
fn hover_cursor(
    model: &ScrollbarModel,
    state: DragState,
    pos: egui::Pos2,
    thumb: Rect,
) -> CursorIcon {
    if !matches!(state, DragState::Idle) {
        if model.vertical {
            CursorIcon::ResizeVertical
        } else {
            CursorIcon::ResizeHorizontal
        }
    } else if thumb.contains(pos) {
        if model.edge_at(pos, thumb).is_some() {
            if model.vertical {
                CursorIcon::ResizeVertical
            } else {
                CursorIcon::ResizeHorizontal
            }
        } else {
            CursorIcon::Grab
        }
    } else {
        CursorIcon::Default
    }
}
