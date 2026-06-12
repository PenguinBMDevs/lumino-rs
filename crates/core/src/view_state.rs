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
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub total_ticks: u32,
    pub key_count: u16,
    pub visible_key_count: u16,
    pub ppq: u16,
    pub keyboard_width: f32,
    pub snap_precision: f32,
    pub default_note_length: f32,
    pub ruler_height: f32,
    pub eraser_behavior: EraserBehavior,
    pub selection_box_mode: SelectionBoxMode,
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

    /// 吸附 tick 到网格
    pub fn snap_tick(&self, tick: f32) -> f32 {
        (tick / self.snap_precision).floor() * self.snap_precision
    }
}
