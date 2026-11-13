//! 力度面板动作通道。

use lumino_message::CcOption;

/// 本帧动作。
#[derive(Debug, Clone, PartialEq)]
pub enum VelocityPanelAction {
    /// 切换模式（力度 → 速度 → CC → Bend）。
    ToggleMode,
    /// 选择 CC / Bend。
    CcSelected(CcOption),
    /// 力度拖拽开始（note_index, 当前力度）。
    VelocityDragStart(usize, u8),
    /// 力度拖拽移动（note_index, 新力度）。
    VelocityDragMove(usize, u8),
    /// 力度拖拽结束。
    VelocityDragEnd,
    /// 力度曲线绘制开始。
    VelocityCurveStart,
    /// 力度曲线批量更新。
    VelocityCurvePaint(Vec<(usize, u8)>),
    /// 力度曲线绘制结束。
    VelocityCurveEnd,
    /// Tempo 拖拽开始（控制点索引）。
    TempoDragStart(usize),
    /// Tempo 拖拽移动（索引, BPM）。
    TempoDragMove(usize, f64),
    /// Tempo 拖拽结束。
    TempoDragEnd,
    /// Tempo 新增控制点（tick, BPM）。
    TempoAdd(f32, f64),
    /// Tempo 删除控制点。
    TempoDelete(usize),
    /// 自动化锚点移动（首次移动由生产侧记历史，后续走批量）。
    AutomationMove {
        /// 音轨索引。
        track_idx: u16,
        /// lane 索引。
        lane_idx: usize,
        /// 原 tick。
        old_tick: u32,
        /// 原值（跳变对场景精确定位）。
        old_value: Option<u16>,
        /// 新 tick。
        new_tick: u32,
        /// 新值。
        new_value: u16,
        /// 本次拖拽的首个移动（生产侧据此记一次历史）。
        first: bool,
    },
    /// 自动化新增锚点（`controller = None` = Pitch Bend）。
    AutomationAdd {
        /// 音轨索引。
        track_idx: u16,
        /// CC 控制器；`None` = Pitch Bend。
        controller: Option<u8>,
        /// tick。
        tick: u32,
        /// 值。
        value: u16,
    },
    /// 自动化删除锚点。
    AutomationDelete {
        /// 音轨索引。
        track_idx: u16,
        /// lane 索引。
        lane_idx: usize,
        /// 目标 tick。
        tick: u32,
    },
    /// 双轴滚轮（时间轴 / 曲线）。
    Wheel { delta_x: f32, delta_y: f32 },
    /// 面板高度调整（顶部手柄拖拽）。
    Resize(f32),
}

const ACTION_ID: &str = "lumino.velocity-panel.action";

/// 记录本帧动作（一帧一条）。
pub fn push_action(ctx: &egui::Context, action: VelocityPanelAction) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<VelocityPanelAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<VelocityPanelAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}
