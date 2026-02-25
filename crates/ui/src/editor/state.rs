#[derive(Debug, Clone)]
pub struct ViewState {
    pub scroll_x: f32, // x轴滚动位置，对应歌曲位置，单位为tick
    pub scroll_y: f32, // y轴滚动位置，对应键盘位置，单位可能为pixel

    pub zoom_x: f32, // 横向缩放: Pixels per Tick
    pub zoom_y: f32, // 纵向缩放: Pixels per Key

    pub total_ticks: u32,       // 歌曲总长度，单位为tick
    pub key_count: u16,         // 键盘总键数，默认128，目前计划支持88/128/256键
    pub visible_key_count: u16, // 显示的琴键数量，默认128，最大256
    pub ppq: u16,               // 分辨率，整数，默认设定为1920，最大值65535
    pub keyboard_width: f32,    // 键盘宽度，单位为像素，默认120
    pub snap_precision: f32,    // 音符对齐精度，单位为tick，默认ppq/2（四分之一拍子线）
    pub default_note_length: f32, // 默认音符长度（ticks），等于拍子线间隔
                                // pub scale: Scale  // TODO: 之后我们需要支持不同的调式/微分音
}

impl Default for ViewState {
    fn default() -> Self {
        // 这里给个默认值，默认打开钢琴卷帘就是这样的坐标位置和大小
        Self {
            scroll_x: 0.0,               // 歌曲位置0tick
            scroll_y: 0.0,               // 理应把焦点放在中间音区最合适，之后看看多少像素最合适
            zoom_x: 0.1,                 // 每像素10tick，gate1920的音符长度是1920像素
            zoom_y: 20.0,                // 琴键高度20像素
            total_ticks: 1920 * 4 * 100, // 默认100小节
            key_count: 128,              // 显示为128键（不影响MIDI内部数据）
            visible_key_count: 128,      // 显示128个琴键分割线
            ppq: 1920,                   // 分辨率1920
            keyboard_width: 120.0,       // 键盘宽度120像素
            snap_precision: 960.0,       // 对齐精度960 ticks（ppq/2，四分之一拍子线）
            default_note_length: 960.0,  // ppq/2，等于拍子线间隔
        }
    }
}
