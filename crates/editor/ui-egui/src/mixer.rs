//! 混音台（独立对话框窗口）：通道条布局与数据模型。
//!
//! 混音台由 `DialogType::Mixer` 创建**独立 OS 窗口**，不再内嵌在主窗口里。
//! 主窗口每帧把轨道 / 电平快照推入对话框 Root（`RunnerInner::sync_mixer_dialog`），
//! 对话框产生的 [`MixerAction`] 再由 Runner 映射回主窗口的 `SidebarEvent`。
//!
//! 几何口径沿用原浮动面板：通道条宽 88、间距 8、步进 96；推子 / 标尺 / 电平表
//! 统一高 180；电平条宽 6；视口裁剪外扩 2 个步进。主控条固定在右侧，不随轨道滚动。
//!
//! 绘制与输入在 [`show::show_dialog`]；本文件只保留纯数据与几何。

mod show;

#[cfg(test)]
mod tests;

pub use show::show_dialog;

use egui::{Id, Rect};

use crate::layout;

/// 通道条宽（逻辑像素）。
pub const STRIP_WIDTH: f32 = 88.0;
/// 通道条间距。
pub const STRIP_SPACING: f32 = 8.0;
/// 相邻通道条步进。
pub const STRIP_STEP: f32 = STRIP_WIDTH + STRIP_SPACING;
/// 推子 / 标尺 / 电平表高度。
pub const FADER_HEIGHT: f32 = 180.0;
/// 电平条宽。
pub const METER_WIDTH: f32 = 6.0;
/// 视口裁剪外扩。
pub const CULL_MARGIN: f32 = STRIP_STEP * 2.0;
/// 主体内边距。
pub const BODY_PAD: f32 = 8.0;
/// 主控条与普通通道条之间的额外间距。
pub const MASTER_GAP: f32 = 8.0;

/// 单条通道条数据。
#[derive(Debug, Clone, PartialEq)]
pub struct MixerStrip {
    /// 显示名（轨道名 / `Master`）。
    pub name: String,
    /// 音量 0..=1（线性）。
    pub volume: f32,
    /// 声像 -1..=1。
    pub pan: f32,
    /// 静音。
    pub muted: bool,
    /// 独奏。
    pub soloed: bool,
    /// 电平 0..=1（峰值）。
    pub meter: f32,
    /// 是否主控条。
    pub master: bool,
}

/// 混音台模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixerModel {
    /// 普通通道条横向滚动（像素）；主控条不参与滚动。
    pub scroll_x: f32,
    /// 通道条（不含主控）。
    pub strips: Vec<MixerStrip>,
    /// 主控条（`None` = 不显示）。
    pub master: Option<MixerStrip>,
}

/// 混音台对话框的整窗模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MixerDialogModel {
    /// 顶部是否有 30px 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 通道条数据。
    pub mixer: MixerModel,
}

/// 面板几何（窗口逻辑坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerLayout {
    /// 整个客户端区域。
    pub panel: Rect,
    /// 通道条主体（自制标题栏之下）。
    pub body: Rect,
    /// 普通通道条视口（已扣除右侧主控条）。
    pub strips_viewport: Rect,
    /// 主控条矩形（固定在右侧；`None` = 无主控）。
    pub master: Option<Rect>,
}

impl MixerLayout {
    /// 由对话框客户端矩形算布局。
    pub fn dialog(rect: Rect, titlebar: bool, model: &MixerModel) -> Self {
        // 30px 自制标题栏由 `chrome::popup_titlebar` 独占，内容区不再画标题行。
        let chrome_top = if titlebar { layout::TITLEBAR_H } else { 0.0 };
        let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + chrome_top), rect.max);
        let master_width = if model.master.is_some() {
            STRIP_WIDTH + MASTER_GAP
        } else {
            0.0
        };
        let strips_viewport = Rect::from_min_max(
            body.min,
            egui::pos2(
                (body.right() - master_width).max(body.left()),
                body.bottom(),
            ),
        );
        let master = model.master.as_ref().map(|_| {
            let left = (body.right() - STRIP_WIDTH - BODY_PAD).max(strips_viewport.left());
            Rect::from_min_size(
                egui::pos2(left, body.top() + BODY_PAD),
                egui::vec2(STRIP_WIDTH, FADER_HEIGHT),
            )
        });
        Self {
            panel: rect,
            body,
            strips_viewport,
            master,
        }
    }

    /// 第 `index` 个普通通道条矩形（含滚动偏移）。
    pub fn strip_rect(&self, index: usize, scroll_x: f32) -> Rect {
        Rect::from_min_size(
            egui::pos2(
                self.strips_viewport.left() + BODY_PAD + index as f32 * STRIP_STEP - scroll_x,
                self.body.top() + BODY_PAD,
            ),
            egui::vec2(STRIP_WIDTH, FADER_HEIGHT),
        )
    }

    /// 可见通道条下标（视口裁剪，外扩 2 个步进）。
    pub fn visible(&self, count: usize, scroll_x: f32) -> Vec<usize> {
        (0..count)
            .filter(|i| {
                let r = self.strip_rect(*i, scroll_x);
                r.right() >= self.strips_viewport.left() - CULL_MARGIN
                    && r.left() <= self.strips_viewport.right() + CULL_MARGIN
            })
            .collect()
    }

    /// 横向滚动的最大值（内容恰好贴住视口右缘）。
    pub fn max_scroll(&self, count: usize) -> f32 {
        if count == 0 {
            return 0.0;
        }
        let content = BODY_PAD * 2.0 + count as f32 * STRIP_STEP - STRIP_SPACING;
        (content - self.strips_viewport.width()).max(0.0)
    }
}

/// 命中区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerHit {
    /// 第 `index` 条的推子。
    Fader(usize),
    /// 第 `index` 条的声像。
    Pan(usize),
    /// 第 `index` 条的静音。
    Mute(usize),
    /// 第 `index` 条的独奏。
    Solo(usize),
    /// 主控推子。
    MasterVolume,
}

/// 推子轨道区域（通道条中部竖条；与绘制端的轨道矩形一致）。
fn fader_rect(strip: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(strip.left() + 16.0, strip.top() + 24.0),
        egui::pos2(strip.right() - 16.0, strip.bottom() - 48.0),
    )
}

/// 主控推子区域（主控条没有底部 P/M/S 行）。
fn master_fader_rect(strip: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(strip.left() + 16.0, strip.top() + 24.0),
        egui::pos2(strip.right() - 16.0, strip.bottom() - 24.0),
    )
}

/// 命中测试（`pos` 为窗口逻辑坐标）。
pub fn hit_test(pos: egui::Pos2, layout: &MixerLayout, model: &MixerModel) -> Option<MixerHit> {
    if let Some(master) = layout.master
        && master.contains(pos)
    {
        return master_fader_rect(master)
            .contains(pos)
            .then_some(MixerHit::MasterVolume);
    }
    for index in layout.visible(model.strips.len(), model.scroll_x) {
        let strip = layout.strip_rect(index, model.scroll_x);
        if !strip.contains(pos) {
            continue;
        }
        if fader_rect(strip).contains(pos) {
            return Some(MixerHit::Fader(index));
        }
        // 与绘制端三格顶边一致（`rect.bottom() - 34.0`），避免文字上方 6px 误命中。
        let row_y = strip.bottom() - 34.0;
        if pos.y >= row_y {
            let third = strip.width() / 3.0;
            let rel = ((pos.x - strip.left()) / third).clamp(0.0, 2.99);
            return Some(match rel as u32 {
                0 => MixerHit::Pan(index),
                1 => MixerHit::Mute(index),
                _ => MixerHit::Solo(index),
            });
        }
    }
    None
}

/// 混音台动作（egui 输入 → 主窗口的 `SidebarEvent`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixerAction {
    /// 第 `index` 条音量（0..=1）。
    Gain {
        /// 通道条下标。
        index: usize,
        /// 音量。
        value: f32,
    },
    /// 第 `index` 条声像（-1..=1）。
    Pan {
        /// 通道条下标。
        index: usize,
        /// 声像。
        value: f32,
    },
    /// 第 `index` 条静音切换。
    Mute(usize),
    /// 第 `index` 条独奏切换。
    Solo(usize),
    /// 主音量（0..=1）。
    MasterVolume(f32),
    /// 普通通道条横向滚动到绝对偏移（像素）。
    Scrolled(f32),
}

/// 动作通道：一帧可产生多条（拖拽推子会连续产生）。
const ACTION_ID: &str = "lumino.mixer.dialog.action";

/// 记录本帧动作。
pub fn push_action(ctx: &egui::Context, action: MixerAction) {
    let mut actions = ctx
        .data(|d| d.get_temp::<Vec<MixerAction>>(Id::unique(ACTION_ID)))
        .unwrap_or_default();
    actions.push(action);
    ctx.data_mut(|d| d.insert_temp(Id::unique(ACTION_ID), actions));
}

/// 取走本帧动作。
pub fn take_actions(ctx: &egui::Context) -> Vec<MixerAction> {
    ctx.data_mut(|d| d.remove_temp::<Vec<MixerAction>>(Id::unique(ACTION_ID)))
        .unwrap_or_default()
}

/// 推子拖拽目标（ctx-data 跨帧保存）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragTarget {
    /// 第 `index` 条普通通道。
    Track(usize),
    /// 主控条。
    Master,
}

const DRAG_ID: &str = "lumino.mixer.dialog.drag";

fn drag_target(ctx: &egui::Context) -> Option<DragTarget> {
    ctx.data(|d| d.get_temp::<DragTarget>(Id::unique(DRAG_ID)))
}

fn set_drag_target(ctx: &egui::Context, target: Option<DragTarget>) {
    let id = Id::unique(DRAG_ID);
    match target {
        Some(target) => {
            ctx.data_mut(|d| d.insert_temp(id, target));
        }
        None => {
            ctx.data_mut(|d| d.remove::<DragTarget>(id));
        }
    }
}
