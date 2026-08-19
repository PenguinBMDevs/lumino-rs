//! 视频导出帧参数与耗时统计的数据类型。

/// 单帧合成参数（与帧数据 FIFO 一一对应），替代裸 6 元组避免位置解构出错。
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameParams {
    /// 标尺滚动偏移（像素）
    pub(super) scroll_x: f32,
    /// 标尺缩放（像素/tick）
    pub(super) zoom_x: f32,
    /// 键盘宽度（像素）
    pub(super) keyboard_width: f32,
    /// 分辨率（Pulses Per Quarter note）
    pub(super) ppq: u32,
    /// 按键高亮颜色（RGBA × 256 键）
    pub(super) key_colors: [u8; 1024],
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            zoom_x: 1.0,
            keyboard_width: 60.0,
            ppq: 0,
            key_colors: [0u8; 1024],
        }
    }
}

/// 编码帧参数队列（入队顺序与帧数据 FIFO 严格对应）。
pub(super) type EncodeFrameQueue = std::collections::VecDeque<FrameParams>;

/// 单帧处理阶段耗时统计（微秒）
#[derive(Debug, Default)]
pub(super) struct FrameStageStats {
    /// 键盘 + 标尺合成耗时
    pub(super) composite_us: u64,
    /// 预览帧克隆/缩放/发送耗时
    pub(super) preview_us: u64,
    /// ffmpeg 写入耗时
    pub(super) encode_us: u64,
}
