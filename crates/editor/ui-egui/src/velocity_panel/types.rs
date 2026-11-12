//! 力度面板的数据快照类型（与 iced 无耦合）。

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
