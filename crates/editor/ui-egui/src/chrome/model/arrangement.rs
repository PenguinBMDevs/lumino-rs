//! 工程走带视图的 egui 模型（标尺 + 轨道列）。
//!
//! 只做数据搬运：几何量（scroll / zoom / ppq / 拍号 / 行高）来自生产
//! `arrangement_view.viewport` 与 `editor_state`，绘制在
//! `crate::chrome::arrangement`。

/// 走带标尺参数。
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementRulerModel {
    /// 水平滚动偏移（像素）。
    pub scroll_x: f32,
    /// 每 tick 的像素数。
    pub zoom_x: f32,
    /// 每四分音符 tick 数。
    pub ppq: u16,
    /// 走带内容区左缘（逻辑点，相对窗口）：左侧栏总宽。
    pub left_inset: f32,
    /// 走带内容区顶缘（逻辑点，相对窗口）：标题栏 + 实际工具栏高度。
    pub top_inset: f32,
    /// 拍号变化表（原样传入；绘制侧负责同 tick 去重）。
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 乐曲总 tick 数（点击设光标的钳制上界）。
    pub total_ticks: u32,
}

impl Default for ArrangementRulerModel {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            zoom_x: 0.1,
            ppq: 960,
            left_inset: 48.0,
            top_inset: 102.0,
            time_signatures: vec![(0, 4, 4)],
            total_ticks: 0,
        }
    }
}

/// 走带左侧轨道列的一行。
#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementTrackRow {
    /// 音轨 ID（显示为三位编号）。
    pub id: usize,
    /// 音轨名（第二行）。
    pub name: String,
    /// 显示标签（第一行右侧，如 `A01`）。
    pub label: String,
    /// 通道号（`label` 为空时按 `{端口}{通道}` 生成）。
    pub channel: u8,
    /// 是否主控音轨（显示 `Master`，且不画静音/独奏按钮）。
    pub conductor: bool,
    /// 是否选中（选中行底色换成主色弱调）。
    pub selected: bool,
    /// 是否静音。
    pub muted: bool,
    /// 是否独奏。
    pub soloed: bool,
    /// 音轨颜色（`None` = 左侧画默认灰块）。
    pub color: Option<egui::Color32>,
}

/// 走带左侧轨道列（160px）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArrangementTrackListModel {
    /// 音轨行（按显示顺序）。
    pub tracks: Vec<ArrangementTrackRow>,
    /// 每行高度（`track_height * zoom_y`）。
    pub track_height: f32,
    /// 垂直滚动偏移（像素）。
    pub scroll_y: f32,
    /// 长按拖拽排序是否已激活。
    pub drag_active: bool,
    /// 被拖拽的音轨 ID（激活时有效）。
    pub drag_track: Option<usize>,
    /// 插入位置指示（`0..=tracks.len()`）。
    pub drop_index: Option<usize>,
}

/// 走带视图模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArrangementModel {
    /// 顶部标尺。
    pub ruler: ArrangementRulerModel,
    /// 左侧轨道列。
    pub track_list: ArrangementTrackListModel,
}
