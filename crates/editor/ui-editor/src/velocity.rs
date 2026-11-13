//! 力度/Tempo/CC 编辑面板 - 类 Cubase 的 Controller Lane
//!
//! X 轴与钢琴卷帘对齐联动。力度/CC 的 Y 轴 0-127，Tempo 的 Y 轴为可视 BPM 范围。

pub mod layout;

// 重新导出自 lumino-note-core / lumino-editor-state 的数据类型
pub use lumino_editor_state::EditorTransform;
pub use lumino_note_core::{
    BendDisplay, BendPoint, CC_CONTROLLER_NAMES, CcDisplay, CcPoint, EditMode, VelocityPoint,
};

/// 面板高度（像素）
pub const VELOCITY_PANEL_HEIGHT: f32 = 150.0;
/// 面板最小高度
pub const VELOCITY_PANEL_MIN_HEIGHT: f32 = 60.0;
/// 面板最大高度
pub const VELOCITY_PANEL_MAX_HEIGHT: f32 = 400.0;
/// 点绘制半径
pub const POINT_RADIUS: f32 = 4.0;
/// 悬停高亮半径
pub const HOVER_RADIUS: f32 = 7.0;
/// 点击/拖拽检测半径
pub const HIT_RADIUS: f32 = 10.0;
/// 面板上下内边距
pub const PANEL_PADDING_Y: f32 = 12.0;
/// 面板左右内边距
pub const PANEL_PADDING_X: f32 = 8.0;
/// 顶部 resize 拖拽手柄高度
pub const RESIZE_HANDLE_HEIGHT: f32 = 5.0;
/// 工具栏高度
pub const TOOLBAR_HEIGHT: f32 = 28.0;

/// CC 或 Bend 下拉选项 — 重新导出自 lumino-message
pub use lumino_message::CcOption;

/// 力度/CC 编辑面板组件
pub struct VelocityPanel {
    /// 当前编辑模式
    pub edit_mode: EditMode,
    /// CC 模式下选择的控制器编号
    pub selected_cc: u8,
    /// 自动化曲线垂直缩放。1.0 = 满量程映射到面板数据区高度。
    pub value_zoom: f32,
    /// 自动化曲线垂直滚动偏移（值空间单位）。
    pub value_scroll: f32,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）。
    pub automation_line_thickness: f32,
    /// Tempo 面板 BPM 绘制上限（默认 512，范围 256~65536，可自定义）。
    pub tempo_max_bpm: f64,
}

impl VelocityPanel {
    /// 创建默认的力度/CC 编辑面板组件。
    ///
    /// # 返回
    /// 处于力度模式、CC 编号为 1 的 `VelocityPanel`。
    pub fn new() -> Self {
        Self {
            edit_mode: EditMode::Velocity,
            selected_cc: 1, // 默认调制轮
            value_zoom: 1.0,
            value_scroll: 0.0,
            automation_line_thickness: 2.0,
            tempo_max_bpm: 512.0,
        }
    }

    /// 根据当前目标的最大值限制 value_scroll，避免滚出有效范围。
    pub fn clamp_value_scroll(&mut self, max_value: f32) {
        let visible_range = max_value / self.value_zoom.max(0.001);
        let max_scroll = (max_value - visible_range).max(0.0);
        self.value_scroll = self.value_scroll.clamp(0.0, max_scroll);
    }

    /// 构建速度点数据
    pub fn build_tempo_points(
        editor: &crate::Editor,
    ) -> Vec<lumino_note_core::midi_types::TempoPoint> {
        editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| lumino_note_core::midi_types::TempoPoint {
                tick: tp.tick,
                bpm: tp.bpm,
            })
            .collect()
    }

    /// 构建力度点数据（直接遍历 document 当前轨，避免 clone）。
    pub fn build_velocity_points(
        notes: &lumino_midi_loader::ChunkedList<lumino_midi_loader::NoteEvent>,
    ) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = Vec::with_capacity(notes.len());
        for (i, n) in notes.iter().enumerate() {
            points.push(VelocityPoint {
                note_index: i,
                tick: n.start_tick as f32,
                velocity: n.velocity,
                length: (n.end_tick - n.start_tick) as f32,
            });
        }
        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });
        points
    }

    /// 构建 CC 数据（从 automation_lanes 读取当前音轨的 CC 事件）。
    pub fn build_cc_points(editor: &crate::Editor, cc_number: u8) -> Vec<CcPoint> {
        editor.editor_state.data.build_cc_points(cc_number)
    }

    /// 构建弯音数据（从 automation_lanes 读取当前轨的 PitchBend 事件）。
    pub fn build_bend_points(editor: &crate::Editor) -> Vec<BendPoint> {
        editor.editor_state.data.build_bend_points()
    }
}

impl Default for VelocityPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
