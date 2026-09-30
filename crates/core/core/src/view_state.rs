//! 视图状态（滚动、缩放、显示参数）

use crate::smooth_scroll::SmoothScrollAnimation;
use crate::storage::config::{EraserBehavior, SelectionBoxMode};

/// 默认的歌曲位置 (tick)
pub const DEFAULT_SCROLL_X: f32 = 0.0;
/// 默认的键盘滚动位置 (pixel)
pub const DEFAULT_SCROLL_Y: f32 = 0.0;
/// 默认横向缩放 (Pixels per Tick)
pub const DEFAULT_ZOOM_X: f32 = 0.1;
/// 默认纵向缩放 (Pixels per Key)
pub const DEFAULT_ZOOM_Y: f32 = 20.0;
/// 默认分辨率 (Pulses Per Quarter note)
pub const DEFAULT_PPQ: u16 = 1920;
/// 默认歌曲总长度 (tick)
pub const DEFAULT_TOTAL_TICKS: u32 = (DEFAULT_PPQ as u32) * 4 * 100;
/// 默认键盘总键数
pub const DEFAULT_KEY_COUNT: u16 = 128;
/// 默认显示的琴键数量
pub const DEFAULT_VISIBLE_KEY_COUNT: u16 = 128;
/// 默认键盘宽度 (pixel)
pub const DEFAULT_KEYBOARD_WIDTH: f32 = 120.0;
/// 默认音符对齐精度 (tick)
pub const DEFAULT_SNAP_PRECISION: f32 = DEFAULT_PPQ as f32;
/// 默认音符长度 (tick)
pub const DEFAULT_NOTE_LENGTH: f32 = DEFAULT_PPQ as f32;
/// 默认时间轴标尺高度 (pixel)
pub const DEFAULT_RULER_HEIGHT: f32 = 24.0;

/// 视图状态（滚动、缩放、显示参数）
#[derive(Debug, Clone)]
pub struct ViewState {
    /// 水平滚动偏移（pixel）
    pub scroll_x: f32,
    /// 垂直滚动偏移（pixel）—— 横向钢琴卷帘：键盘高度轴
    pub scroll_y: f32,
    /// 横向缩放（Pixels per Tick）
    pub zoom_x: f32,
    /// 纵向缩放（Pixels per Key）—— 横向钢琴卷帘：键盘高度轴
    pub zoom_y: f32,
    /// 歌曲总长度（tick）
    pub total_ticks: u32,
    /// 键盘总键数
    pub key_count: u16,
    /// 当前显示的琴键数量
    pub visible_key_count: u16,
    /// 分辨率（Pulses Per Quarter note）
    pub ppq: u16,
    /// 键盘宽度（pixel）
    pub keyboard_width: f32,
    /// 音符对齐精度（tick）
    pub snap_precision: f32,
    /// 默认音符长度（tick）
    pub default_note_length: f32,
    /// 上一次放置音符的长度（用于预览矩形和下次放置的默认长度）
    pub last_note_length: Option<f32>,
    /// 时间轴标尺高度（pixel）
    pub ruler_height: f32,
    /// 橡皮擦行为
    pub eraser_behavior: EraserBehavior,
    /// 框选框显示模式
    pub selection_box_mode: SelectionBoxMode,
    /// 平滑滚动动画状态
    pub smooth_scroll: SmoothScrollAnimation,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            scroll_x: DEFAULT_SCROLL_X,
            scroll_y: DEFAULT_SCROLL_Y,
            zoom_x: DEFAULT_ZOOM_X,
            zoom_y: DEFAULT_ZOOM_Y,
            total_ticks: DEFAULT_TOTAL_TICKS,
            key_count: DEFAULT_KEY_COUNT,
            visible_key_count: DEFAULT_VISIBLE_KEY_COUNT,
            ppq: DEFAULT_PPQ,
            keyboard_width: DEFAULT_KEYBOARD_WIDTH,
            snap_precision: DEFAULT_SNAP_PRECISION,
            default_note_length: DEFAULT_NOTE_LENGTH,
            last_note_length: None,
            ruler_height: DEFAULT_RULER_HEIGHT,
            eraser_behavior: EraserBehavior::default(),
            selection_box_mode: SelectionBoxMode::default(),
            smooth_scroll: SmoothScrollAnimation::new(),
        }
    }
}

impl ViewState {
    /// tick 转换为 x 坐标
    pub fn tick_to_x(&self, tick: f32) -> f32 {
        tick * self.zoom_x + self.keyboard_width - self.scroll_x
    }

    /// key 转换为 y 坐标
    pub fn key_to_y(&self, key: u16) -> f32 {
        let max_key_index = (self.visible_key_count - 1) as f32;
        (max_key_index - key as f32) * self.zoom_y - self.scroll_y + self.ruler_height
    }

    /// x 坐标转换为 tick
    pub fn x_to_tick(&self, x: f32) -> f32 {
        (x - self.keyboard_width + self.scroll_x) / self.zoom_x
    }

    /// y 坐标转换为 key
    pub fn y_to_key(&self, y: f32) -> u16 {
        let adjusted_y = y - self.ruler_height;
        let max_key_index = (self.visible_key_count - 1) as f32;
        let key_f32 = max_key_index - (adjusted_y + self.scroll_y) / self.zoom_y;
        key_f32.round().clamp(0.0, max_key_index) as u16
    }

    /// 设置键盘宽度
    pub fn set_keyboard_width(&mut self, width: f32) {
        self.keyboard_width = width.max(0.0);
    }

    /// 设置吸附精度
    pub fn set_snap_precision(&mut self, precision: f32) {
        self.snap_precision = precision.max(1.0);
    }

    /// 设置默认音符长度
    pub fn set_default_note_length(&mut self, length: f32) {
        self.default_note_length = length.max(1.0);
    }

    /// 设置上一次放置音符的长度
    pub fn set_last_note_length(&mut self, length: f32) {
        self.last_note_length = Some(length.max(1.0));
    }

    /// 设置橡皮擦行为
    pub fn set_eraser_behavior(&mut self, behavior: EraserBehavior) {
        self.eraser_behavior = behavior;
    }

    /// 设置选择框模式
    pub fn set_selection_box_mode(&mut self, mode: SelectionBoxMode) {
        self.selection_box_mode = mode;
    }

    /// 吸附 tick 到网格
    ///
    /// **绝对 tick 量化**（下拉框起点、拉伸首尾、绘制落点等）的唯一入口：`floor`
    /// 对齐到 [`Self::snap_precision`]。
    ///
    /// ⚠️ 框选 X 向边界**不要**用它直接刷两端（见 [`Self::snap_marquee_edges`]）。
    pub fn snap_tick(&self, tick: f32) -> f32 {
        (tick / self.snap_precision).floor() * self.snap_precision
    }

    /// 框选锚点单元低边（按下时调用一次）：`floor` 对齐到精度网格。
    pub fn snap_marquee_anchor(&self, tick: f32) -> f32 {
        self.snap_tick(tick)
    }

    /// 由框选锚点与当前鼠标 tick 推算选框两端 —— **框选 X 向量化的唯一入口**。
    ///
    /// 采用「**单元覆盖式**」量化：选框恰好覆盖鼠标**扫过的全部精度单元**。
    /// - 锚点端取所在单元的**外沿**：右拖取单元低边（锚点本身）；左拖取单元高边
    ///   `锚点 + p`（此时锚点是选框右边界）。
    /// - 鼠标端取所在单元的**外沿**：右拖取 `floor(m) + p`；左拖取 `floor(m)`。
    ///
    /// 与框选命中的半开区间 `[min, max)`（见 `drag::selection::marquee_hits`）配合后：
    /// - **框边界落在格线上** → 用户按「音符精度」思考的边界与视觉一致；
    /// - 鼠标端提前拖过格线一点点会被 `floor` **吸收整整一个单元** → 低精度模式下
    ///   不再"手一抖就多选框缘音符"；
    /// - 选框 ⊇ 鼠标扫过范围 → 扫过的单元内音符**零漏选**。
    ///
    /// ⚠️ 禁止用单侧 [`Self::snap_tick`]（floor）同时刷两端：
    /// - 大端 floor 会**内缩**（框内重叠音符漏选）；
    /// - 反向（向左）拖动时小端 floor 会把左边界**外扩整整一个单元**，选中鼠标从未
    ///   扫过的音符 —— 这是 `ui-editor/src/tests/selection_precision.rs` 记录过的
    ///   历史 bug（曾用 `snap_tick_forward` / 单侧 floor，已回退）。评审勿改回单侧 floor。
    ///
    /// 返回 `(start_tick, current_tick)`：前项为**锚点端**、后项为**鼠标端**（方向保持，
    /// 向左拖时 `start_tick > current_tick`），供 Spring 弹簧动画按"移动端"驱动。
    pub fn snap_marquee_edges(&self, anchor_low: f32, mouse_tick: f32) -> (f32, f32) {
        let p = self.snap_precision.max(1.0);
        let mouse_cell_low = (mouse_tick / p).floor() * p;
        if mouse_tick >= anchor_low {
            (anchor_low, mouse_cell_low + p)
        } else {
            (anchor_low + p, mouse_cell_low)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_with_precision(p: f32) -> ViewState {
        ViewState {
            snap_precision: p,
            ..ViewState::default()
        }
    }

    /// 锚点 = 按下 tick 的单元低边（floor）
    #[test]
    fn test_snap_marquee_anchor_floors_to_cell() {
        let v = view_with_precision(1920.0);
        assert_eq!(v.snap_marquee_anchor(0.0), 0.0);
        assert_eq!(v.snap_marquee_anchor(1919.0), 0.0);
        assert_eq!(v.snap_marquee_anchor(1920.0), 1920.0);
        assert_eq!(v.snap_marquee_anchor(2400.0), 1920.0);
        assert_eq!(v.snap_marquee_anchor(5000.0), 3840.0);
        // 负 tick（框选可越过 0 向左）
        assert_eq!(v.snap_marquee_anchor(-1.0), -1920.0);
    }

    /// 向右拖：覆盖 [锚点单元低边, 鼠标单元高边)
    #[test]
    fn test_snap_marquee_edges_forward_covers_touched_cells() {
        let v = view_with_precision(1920.0);
        let anchor = v.snap_marquee_anchor(2400.0); // 1920
        // 鼠标仍在锚点单元内 → 只覆盖锚点单元
        assert_eq!(v.snap_marquee_edges(anchor, 2400.0), (1920.0, 3840.0));
        assert_eq!(v.snap_marquee_edges(anchor, 3839.0), (1920.0, 3840.0));
        // 鼠标进入下一单元 → 覆盖到该单元高边
        assert_eq!(v.snap_marquee_edges(anchor, 3840.0), (1920.0, 5760.0));
        assert_eq!(v.snap_marquee_edges(anchor, 5000.0), (1920.0, 5760.0));
    }

    /// 向左拖：覆盖 [鼠标单元低边, 锚点单元高边)，方向保持（start > current）
    #[test]
    fn test_snap_marquee_edges_backward_keeps_direction() {
        let v = view_with_precision(1920.0);
        let anchor = v.snap_marquee_anchor(2400.0); // 1920
        let (start, current) = v.snap_marquee_edges(anchor, 1500.0);
        assert_eq!(
            (start, current),
            (3840.0, 0.0),
            "锚点端为单元高边、鼠标端为低边"
        );
        assert!(
            start > current,
            "向左拖必须方向保持（Spring 动画按移动端驱动）"
        );
        // 覆盖的单元并集 = [0, 3840)
        assert_eq!(current.min(start), 0.0);
        assert_eq!(current.max(start), 3840.0);
    }

    /// 两端永远落在格线上（量化契约），四个档位一致
    #[test]
    fn test_snap_marquee_edges_always_on_grid_lines() {
        for p in [1920.0_f32, 960.0, 480.0, 240.0] {
            let v = view_with_precision(p);
            for press in [0.0_f32, 137.0, 700.0, 1920.0, 2401.0, 5000.0] {
                let anchor = v.snap_marquee_anchor(press);
                assert_eq!(anchor % p, 0.0, "精度 {p}：锚点必须落格线");
                for mouse in [0.0_f32, 137.0, 700.0, 1920.0, 2401.0, 5000.0] {
                    let (a, b) = v.snap_marquee_edges(anchor, mouse);
                    assert_eq!(a % p, 0.0, "精度 {p}：锚点端必须落格线");
                    assert_eq!(b % p, 0.0, "精度 {p}：鼠标端必须落格线");
                    // 覆盖性：鼠标所在单元的完整跨度必须被选框包含
                    let cell_low = (mouse / p).floor() * p;
                    assert!(
                        a.min(b) <= cell_low && a.max(b) >= cell_low + p,
                        "精度 {p}：鼠标 tick {mouse} 所在单元 [{cell_low}, {}) 未被选框覆盖",
                        cell_low + p
                    );
                }
            }
        }
    }

    /// 边界口径：鼠标恰在格线上时归属右侧单元（与 `snap_tick` 的 floor 语义一致），
    /// 保证「按在格线=右格」在全项目内只有一套解释。
    #[test]
    fn test_snap_marquee_edges_at_grid_line_belongs_to_right_cell() {
        let v = view_with_precision(1920.0);
        let anchor = v.snap_marquee_anchor(1920.0);
        assert_eq!(anchor, 1920.0);
        assert_eq!(
            v.snap_marquee_edges(anchor, 1920.0),
            (1920.0, 3840.0),
            "恰在格线上：锚点端为格线本身、鼠标端为下一格线（零宽输入覆盖一个单元）"
        );
    }
}
