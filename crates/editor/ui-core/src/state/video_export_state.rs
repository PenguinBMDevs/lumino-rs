//! 视频导出面板状态

/// MIDITrail Z 方向显示距离默认值。
pub const MIDITRAIL_Z_FAR_DEFAULT: f32 = 7.5;
/// MIDITrail Z 方向显示距离最大值（也是滑杆上限）。
pub const MIDITRAIL_Z_FAR_MAX: f32 = 15.0;
/// MIDITrail 视图模式默认值（Normal 普通视图）。
pub const MIDITRAIL_VIEW_MODE_DEFAULT: &str = "Normal";
/// MIDITrail Normal/Top 视图滚动速度默认值（与瀑布流速度默认值一致）。
pub const MIDITRAIL_SPEED_DEFAULT: f32 = 1.0;

/// 计数器默认文本模板（参考 Zenith-MIDI NoteCountRender 的 default 模板）。
pub const COUNTER_DEFAULT_TEXT: &str =
    "Notes: {nc} / {tn}\nBPM: {bpm}\nNPS: {nps}\nPPQ: {ppq}\nPolyphony: {plph}\nTime: {currtime}";
/// 计数器完整文本模板（全部占位符演示）。
pub const COUNTER_FULL_TEXT: &str = "Notes: {nc} / {tn} / {nr}\nBPM: {bpm}\nNPS: {nps} (Max: {mnps})\nPolyphony: {plph} (Max: {mplph})\nSeconds: {currsec} / {totalsec} / {remsec}\nTime: {currtime} / {totaltime} / {remtime}\nTicks: {currticks} / {totalticks} / {remticks}\nBars: {currbars} / {totalbars} / {rembars}\nFrames: {currframes} / {totalframes} / {remframes}\nPPQ: {ppq}\nTime Signature: {tsn}/{tsd}\nAverage NPS: {avgnps}\n\n-----Progress-----\nNotes: {notep}%\nTicks: {tickp}%\nTime: {timep}%";
/// 计数器默认 CSV 行格式。
pub const COUNTER_DEFAULT_CSV_FORMAT: &str = "{nps},{plph},{bpm},{nc}";

/// 视频导出覆盖层状态（参照 nezha ExportState）
#[derive(Debug, Clone, Default)]
pub enum VideoExportOverlayState {
    /// 空闲（无覆盖层）
    #[default]
    None,
    /// 导出中（渲染+写帧）
    Exporting,
    /// 编码收尾（等待 ffmpeg 封装）
    Finalizing,
    /// 完成
    Completed {
        /// 总帧数
        total_frames: u64,
        /// 总用时（秒）
        elapsed_secs: f64,
        /// 平均渲染速度
        avg_fps: f64,
    },
    /// 错误
    Error(String),
}

/// 计数器（Counter）渲染设置（视频导出状态的分区）。
#[derive(Debug, Clone, PartialEq)]
pub struct CounterSection {
    pub text: String,
    pub alignment: String,
    pub font_size: u32,
    pub font_mode: String,
    pub font_family: String,
    pub font_path: String,
    pub use_commas: bool,
    pub padding_zeroes: bool,
    pub save_csv: bool,
    pub csv_output: String,
    pub csv_format: String,
    pub bpm_int_pad: u32,
    pub bpm_dec_pad: u32,
    pub note_count_pad: u32,
    pub polyphony_pad: u32,
    pub nps_pad: u32,
    pub ticks_pad: u32,
    pub bars_pad: u32,
    pub frames_pad: u32,
}

impl Default for CounterSection {
    fn default() -> Self {
        Self {
            text: COUNTER_DEFAULT_TEXT.to_string(),
            alignment: "左上".to_string(),
            font_size: 40,
            font_mode: "内置点阵".to_string(),
            font_family: "微软雅黑".to_string(),
            font_path: String::new(),
            use_commas: true,
            padding_zeroes: false,
            save_csv: false,
            csv_output: String::new(),
            csv_format: COUNTER_DEFAULT_CSV_FORMAT.to_string(),
            bpm_int_pad: 3,
            bpm_dec_pad: 2,
            note_count_pad: 5,
            polyphony_pad: 3,
            nps_pad: 3,
            ticks_pad: 5,
            bars_pad: 3,
            frames_pad: 5,
        }
    }
}
/// MIDITrail 渲染设置（从 VideoExportDialogState 拆出的第一个域）。
#[derive(Debug, Clone, PartialEq)]
pub struct MiditrailSection {
    /// 视图模式（Normal / Top）。
    pub view_mode: String,
    /// Normal 视图播放速度（默认 1.0）。
    pub normal_speed: f32,
    /// Top 视图播放速度（默认 1.0，与 Normal 独立）。
    pub top_speed: f32,
    /// Z 轴最远显示距离（默认 7.5，最小 0.1）。
    pub z_far: f32,
    /// 3D 音符开关（false = 平面，true = 原生粒子）。
    pub notes_3d: bool,
}

impl Default for MiditrailSection {
    fn default() -> Self {
        Self {
            view_mode: MIDITRAIL_VIEW_MODE_DEFAULT.to_string(),
            normal_speed: MIDITRAIL_SPEED_DEFAULT,
            top_speed: MIDITRAIL_SPEED_DEFAULT,
            z_far: MIDITRAIL_Z_FAR_DEFAULT,
            notes_3d: false,
        }
    }
}

/// 视频导出面板状态（主界面侧边栏面板）
///
/// 纯 UI 状态，保存控件值与导出进度。
/// 配置值用 String 存储（UI pick_list 原生支持），Runner 端解析回强类型。
#[derive(Debug, Clone)]
pub struct VideoExportDialogState {
    /// 容器格式（"MP4"/"MOV"/"MKV"/"AVI"）
    pub container: String,
    /// 视频编码器（"H.264"/"H.265 / HEVC"/"ProRes"/"VP9"/"AV1"）
    pub codec: String,
    /// 硬件加速后端（"Software (CPU)"/"NVENC (NVIDIA)" 等）
    pub backend: String,
    /// 质量预设（"高"/"中"/"低"）
    pub quality: String,
    /// 渲染模式（"Lumino瀑布流"/"Lumino卷帘"/"MIDITrail"/"计数器"）
    pub render_mode: String,
    /// MidiConsole 渲染后端（"GPU"/"CPU"，默认 GPU）
    pub midi_console_backend: String,
    /// 瀑布流滚动速度（默认 1.0）
    pub waterfall_speed: f32,
    /// MIDITrail 渲染设置。
    pub miditrail: MiditrailSection,

    // ── 计数器设置（参考 Zenith-MIDI NoteCountRender 设置面板） ──
    /// 计数器渲染设置。
    pub counter: CounterSection,
    // ── 数据曲线设置（参考 MIDIGraphRenderer graph 设置面板） ──
    /// 数据来源指标（"NPS（每秒音符数）"/"复音数"/"累计音符数"/"BPM（速度）"）
    pub dc_metric: String,
    /// 曲线窗口时长（秒）
    pub dc_graph_duration: String,
    /// 缩放动画平滑度
    pub dc_zoom_smoothness: String,
    /// 折线平滑窗口（0=关闭）
    pub dc_graph_smoothness: String,
    /// 纵轴缩放 padding 放大系数
    pub dc_padding_mul: String,
    /// 背景颜色（hex 字符串，如 "#000000"，支持 8 位 hex 带 alpha）
    pub dc_bg_color: String,
    /// 折线颜色（hex 字符串）
    pub dc_line_color: String,
    /// 刻度文字颜色（hex 字符串）
    pub dc_text_color: String,
    /// 水平网格线颜色（hex 字符串）
    pub dc_bar_color: String,
    /// 折线宽度（像素）
    pub dc_line_thickness: String,
    /// 水平网格线宽度（像素）
    pub dc_bar_thickness: String,
    /// 刻度文字字号（像素）
    pub dc_font_size: u32,
    /// 刻度文字字体来源（"内置点阵"/"系统字体"/"自定义字体"）
    pub dc_font_mode: String,
    /// 系统字体名称
    pub dc_font_family: String,
    /// 自定义字体文件路径
    pub dc_font_path: String,
    /// 刻度文字 X 偏移（像素）
    pub dc_text_x_offset: String,
    /// 刻度文字 Y 偏移（像素）
    pub dc_text_y_offset: String,
    /// 里程碑文字放大倍数
    pub dc_milestone_scale_mul: String,
    /// 刻度数字缩写（1,000 → 1K）
    pub dc_abbreviate: bool,
    /// 缩写保留小数位数
    pub dc_abbreviate_digits: String,
    /// 显示刻度文字
    pub dc_show_text: bool,
    /// 显示水平网格线
    pub dc_show_bars: bool,
    /// 分辨率宽度
    pub width: u32,
    /// 分辨率高度
    pub height: u32,
    /// 帧率
    pub fps: u32,
    /// MIDI 文件路径（流式读取模式使用；内存模式优先使用已加载的 MidiDocument）
    pub midi_path: String,
    /// 输出路径
    pub output_path: String,
    /// 覆盖层状态（None=空闲，其余=显示模态覆盖层）
    pub overlay: VideoExportOverlayState,
    /// 进度 (0.0 - 1.0)
    pub progress: f64,
    /// 状态消息
    pub status_message: String,
    /// 当前已渲染帧
    pub current_frame: u64,
    /// 总帧数
    pub total_frames: u64,
    /// 渲染速度（fps，EMA 平滑）
    pub render_fps: f64,
    /// 已用时间（秒，墙钟真实时间，由导出线程测量并通过进度通道传入）
    pub elapsed_secs: f64,
    /// 预览帧数据（RGBA 格式，压缩后用于 dialog 内显示预览图像）
    pub preview_frame: Option<std::sync::Arc<Vec<u8>>>,
    /// 预览帧宽度
    pub preview_width: u32,
    /// 预览帧高度
    pub preview_height: u32,
}

impl Default for VideoExportDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoExportDialogState {
    /// 创建一个默认的视频导出对话框状态
    pub fn new() -> Self {
        Self {
            container: "MP4".to_string(),
            codec: "H.264".to_string(),
            backend: "Software (CPU)".to_string(),
            quality: "中".to_string(),
            render_mode: "Lumino瀑布流".to_string(),
            midi_console_backend: "GPU".to_string(),
            waterfall_speed: 1.0,
            miditrail: MiditrailSection::default(),
            counter: CounterSection::default(),
            dc_metric: "NPS（每秒音符数）".to_string(),
            dc_graph_duration: "2.0".to_string(),
            dc_zoom_smoothness: "8.0".to_string(),
            dc_graph_smoothness: "0".to_string(),
            dc_padding_mul: "0.1".to_string(),
            dc_bg_color: "#000000".to_string(),
            dc_line_color: "#00FFFF".to_string(),
            dc_text_color: "#FFFFFF7F".to_string(),
            dc_bar_color: "#FFFFFF7F".to_string(),
            dc_line_thickness: "3".to_string(),
            dc_bar_thickness: "1".to_string(),
            dc_font_size: 24,
            dc_font_mode: "内置点阵".to_string(),
            dc_font_family: "微软雅黑".to_string(),
            dc_font_path: String::new(),
            dc_text_x_offset: "2".to_string(),
            dc_text_y_offset: "2".to_string(),
            dc_milestone_scale_mul: "1.5".to_string(),
            dc_abbreviate: false,
            dc_abbreviate_digits: "3".to_string(),
            dc_show_text: true,
            dc_show_bars: true,
            width: 1920,
            height: 1080,
            fps: 60,
            midi_path: String::new(),
            output_path: String::new(),
            overlay: VideoExportOverlayState::None,
            progress: 0.0,
            status_message: String::new(),
            current_frame: 0,
            total_frames: 0,
            render_fps: 0.0,
            elapsed_secs: 0.0,
            preview_frame: None,
            preview_width: 0,
            preview_height: 0,
        }
    }

    /// 是否正在导出（覆盖层可见）
    pub fn is_exporting(&self) -> bool {
        !matches!(self.overlay, VideoExportOverlayState::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MIDITrail 子结构的默认值与旧字段逐一对应。
    #[test]
    fn miditrail_defaults_snapshot() {
        let state = VideoExportDialogState::new();
        assert_eq!(state.miditrail.view_mode, MIDITRAIL_VIEW_MODE_DEFAULT);
        assert_eq!(state.miditrail.normal_speed, MIDITRAIL_SPEED_DEFAULT);
        assert_eq!(state.miditrail.top_speed, MIDITRAIL_SPEED_DEFAULT);
        assert_eq!(state.miditrail.z_far, MIDITRAIL_Z_FAR_DEFAULT);
        assert!(!state.miditrail.notes_3d);
    }

    /// 子结构与基础字段互不影响（拆分不是换名字，而是分区）。
    #[test]
    fn miditrail_section_is_independent_from_basic_fields() {
        let mut state = VideoExportDialogState::new();
        state.miditrail.top_speed = 2.5;
        state.miditrail.notes_3d = true;
        assert_eq!(state.waterfall_speed, 1.0);
        assert_eq!(state.container, "MP4");
        assert_eq!(state.codec, "H.264");
    }
}

#[cfg(test)]
mod counter_tests {
    use super::*;

    /// 计数器子结构的默认值与旧字段逐一对应。
    #[test]
    fn counter_defaults_snapshot() {
        let state = VideoExportDialogState::new();
        assert_eq!(state.counter.text, COUNTER_DEFAULT_TEXT);
        assert_eq!(state.counter.csv_format, COUNTER_DEFAULT_CSV_FORMAT);
        assert_eq!(state.counter.font_size, 40);
        assert!(state.counter.use_commas);
        assert!(!state.counter.padding_zeroes);
        assert_eq!(
            [
                state.counter.bpm_int_pad,
                state.counter.bpm_dec_pad,
                state.counter.note_count_pad,
                state.counter.polyphony_pad,
                state.counter.nps_pad,
                state.counter.ticks_pad,
                state.counter.bars_pad,
                state.counter.frames_pad,
            ],
            [3, 2, 5, 3, 3, 5, 3, 5]
        );
    }

    /// 计数器与其它域互不影响。
    #[test]
    fn counter_section_is_independent() {
        let mut state = VideoExportDialogState::new();
        state.counter.font_size = 96;
        assert_eq!(state.miditrail.z_far, MIDITRAIL_Z_FAR_DEFAULT);
        assert_eq!(state.waterfall_speed, 1.0);
        assert_eq!(state.width, 1920);
    }
}
