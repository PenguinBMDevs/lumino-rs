use lumino_core::storage::config::EraserBehavior;

/// 默认的歌曲位置 (tick)
pub const DEFAULT_SCROLL_X: f32 = 0.0;
/// 默认的键盘滚动位置 (pixel)
pub const DEFAULT_SCROLL_Y: f32 = 0.0;
/// 默认横向缩放 (Pixels per Tick) - 每像素10tick
pub const DEFAULT_ZOOM_X: f32 = 0.1;
/// 默认纵向缩放 (Pixels per Key) - 琴键高度20像素
pub const DEFAULT_ZOOM_Y: f32 = 20.0;
/// 默认分辨率 (Pulses Per Quarter note)
pub const DEFAULT_PPQ: u16 = 1920;
/// 默认歌曲总长度 (tick) - 默认100小节，每小节4拍
pub const DEFAULT_TOTAL_TICKS: u32 = (DEFAULT_PPQ as u32) * 4 * 100;
/// 默认键盘总键数
pub const DEFAULT_KEY_COUNT: u16 = 128;
/// 默认显示的琴键数量
pub const DEFAULT_VISIBLE_KEY_COUNT: u16 = 128;
/// 默认键盘宽度 (pixel)
pub const DEFAULT_KEYBOARD_WIDTH: f32 = 120.0;
/// 默认音符对齐精度 (tick) - 四分音符（一个拍子线间隔 = PPQ）
pub const DEFAULT_SNAP_PRECISION: f32 = DEFAULT_PPQ as f32;
/// 默认音符长度 (tick) - 等于拍子线间隔（四分音符）
pub const DEFAULT_NOTE_LENGTH: f32 = DEFAULT_PPQ as f32;
/// 默认时间轴标尺高度 (pixel)
pub const DEFAULT_RULER_HEIGHT: f32 = 24.0;

#[derive(Debug, Clone)]
pub struct ViewState {
    pub scroll_x: f32, // x轴滚动位置，对应歌曲位置，单位为tick
    pub scroll_y: f32, // y轴滚动位置，对应键盘位置，单位可能为pixel

    pub zoom_x: f32, // 横向缩放: Pixels per Tick
    pub zoom_y: f32, // 纵向缩放: Pixels per Key

    pub total_ticks: u32,         // 歌曲总长度，单位为tick
    pub key_count: u16,           // 键盘总键数，默认128，目前计划支持88/128/256键
    pub visible_key_count: u16,   // 显示的琴键数量，默认128，最大256
    pub ppq: u16,                 // 分辨率，整数，默认设定为1920，最大值65535
    pub keyboard_width: f32,      // 键盘宽度，单位为像素，默认120
    pub snap_precision: f32,      // 音符对齐精度，单位为tick，默认ppq（四分音符拍子线）
    pub default_note_length: f32, // 默认音符长度（ticks），等于拍子线间隔
    pub ruler_height: f32,        // 时间轴标尺高度（小节号显示区域），单位为像素
    // pub scale: Scale // 之后我们需要支持不同的调式/微分音
    /// 橡皮擦工具行为模式
    pub eraser_behavior: EraserBehavior,
}

impl Default for ViewState {
    fn default() -> Self {
        // 这里给个默认值，默认打开钢琴卷帘就是这样的坐标位置和大小
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
        }
    }
}
