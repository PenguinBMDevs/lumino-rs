//! 力度面板的数据快照类型（与 旧版 无耦合）。

/// 力度点快照（供 egui 输入命中/曲线绘制）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityPointModel {
    /// 音符索引（document 当前轨保序）。
    pub note_index: usize,
    /// 起始 tick。
    pub tick: f32,
    /// 力度 0..=127。
    pub velocity: u8,
}

/// Tempo 控制点快照。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPointModel {
    /// tick 位置。
    pub tick: f32,
    /// BPM。
    pub bpm: f64,
}

/// 自动化锚点快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationAnchor {
    /// tick 位置。
    pub tick: u32,
    /// 原始值（CC 0..=127 / PB 0..=16383）。
    pub value: u16,
}

/// 自动化 lane 快照（CC / Bend）。
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationLaneModel {
    /// 音轨索引。
    pub track_idx: u16,
    /// lane 索引（按事件列表顺序）；`None` = 该目标尚无 lane。
    pub lane_idx: Option<usize>,
    /// `None` = Pitch Bend；`Some(n)` = CC 控制器 n。
    pub controller: Option<u8>,
    /// 值上限（CC = 127，PB = 16383）。
    pub max_value: f32,
    /// 全部锚点（按 tick 升序）。
    pub anchors: Vec<AutomationAnchor>,
}

/// 时间轴视图参数（与钢琴卷帘同源）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityView {
    /// 横向缩放（px/tick）。
    pub zoom_x: f32,
    /// 横向滚动（px）。
    pub scroll_x: f32,
    /// 键盘列宽（px）。
    pub keyboard_width: f32,
    /// 吸附精度（tick）。
    pub snap_precision: f32,
}

impl Default for VelocityView {
    fn default() -> Self {
        Self {
            zoom_x: 0.1,
            scroll_x: 0.0,
            keyboard_width: 120.0,
            snap_precision: 1920.0,
        }
    }
}
