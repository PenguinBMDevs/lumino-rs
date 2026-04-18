/// 渲染统计
#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    /// 总帧数
    pub frame_count: u64,
    /// 上一帧耗时 (ms)
    pub last_frame_time_ms: f64,
    /// 平均 FPS
    pub average_fps: f64,
    /// 丢弃的帧数
    pub dropped_frames: u64,
    /// 渲染的音符数量
    pub note_count: usize,
    /// 渲染的网格线数量
    pub grid_line_count: usize,
    /// 渲染的琴键数量
    pub key_count: usize,
    /// 渲染的标尺刻度数量
    pub ruler_tick_count: usize,
}
