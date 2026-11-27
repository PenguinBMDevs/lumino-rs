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

/// 基础导出设置（容器 / 编码 / 分辨率 / 路径 / 进度）。
#[derive(Debug, Clone)]
pub struct BasicSection {
    pub container: String,
    pub codec: String,
    pub backend: String,
    pub quality: String,
    pub render_mode: String,
    pub midi_console_backend: String,
    pub waterfall_speed: f32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub midi_path: String,
    pub output_path: String,
    pub overlay: VideoExportOverlayState,
    pub progress: f64,
    pub status_message: String,
    pub current_frame: u64,
    pub total_frames: u64,
    pub render_fps: f64,
    pub elapsed_secs: f64,
    pub preview_frame: Option<std::sync::Arc<Vec<u8>>>,
    pub preview_width: u32,
    pub preview_height: u32,
}

impl Default for BasicSection {
    fn default() -> Self {
        Self {
            container: "MP4".to_string(),
            codec: "H.264".to_string(),
            backend: "Software (CPU)".to_string(),
            quality: "中".to_string(),
            render_mode: "Lumino瀑布流".to_string(),
            midi_console_backend: "GPU".to_string(),
            waterfall_speed: 1.0,
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
}
/// 数据曲线（DataCurveSection）渲染设置。
#[derive(Debug, Clone, PartialEq)]
pub struct DataCurveSection {
    pub metric: String,
    pub graph_duration: String,
    pub zoom_smoothness: String,
    pub graph_smoothness: String,
    pub padding_mul: String,
    pub bg_color: String,
    pub line_color: String,
    pub text_color: String,
    pub bar_color: String,
    pub line_thickness: String,
    pub bar_thickness: String,
    pub font_size: u32,
    pub font_mode: String,
    pub font_family: String,
    pub font_path: String,
    pub text_x_offset: String,
    pub text_y_offset: String,
    pub milestone_scale_mul: String,
    pub abbreviate: bool,
    pub abbreviate_digits: String,
    pub show_text: bool,
    pub show_bars: bool,
}

impl Default for DataCurveSection {
    fn default() -> Self {
        Self {
            metric: "NPS（每秒音符数）".to_string(),
            graph_duration: "2.0".to_string(),
            zoom_smoothness: "8.0".to_string(),
            graph_smoothness: "0".to_string(),
            padding_mul: "0.1".to_string(),
            bg_color: "#000000".to_string(),
            line_color: "#00FFFF".to_string(),
            text_color: "#FFFFFF7F".to_string(),
            bar_color: "#FFFFFF7F".to_string(),
            line_thickness: "3".to_string(),
            bar_thickness: "1".to_string(),
            font_size: 24,
            font_mode: "内置点阵".to_string(),
            font_family: "微软雅黑".to_string(),
            font_path: String::new(),
            text_x_offset: "2".to_string(),
            text_y_offset: "2".to_string(),
            milestone_scale_mul: "1.5".to_string(),
            abbreviate: false,
            abbreviate_digits: "3".to_string(),
            show_text: true,
            show_bars: true,
        }
    }
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
    /// 基础导出设置。
    pub basic: BasicSection,
    /// MIDITrail 渲染设置。
    pub miditrail: MiditrailSection,

    // ── 计数器设置（参考 Zenith-MIDI NoteCountRender 设置面板） ──
    /// 计数器渲染设置。
    pub counter: CounterSection,
    // ── 数据曲线设置（参考 MIDIGraphRenderer graph 设置面板） ──
    /// 数据曲线渲染设置。
    pub data_curve: DataCurveSection,
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
            basic: BasicSection::default(),
            miditrail: MiditrailSection::default(),
            counter: CounterSection::default(),
            data_curve: DataCurveSection::default(),
        }
    }

    /// 是否正在导出（覆盖层可见）
    pub fn is_exporting(&self) -> bool {
        !matches!(self.basic.overlay, VideoExportOverlayState::None)
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
        assert_eq!(state.basic.waterfall_speed, 1.0);
        assert_eq!(state.basic.container, "MP4");
        assert_eq!(state.basic.codec, "H.264");
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
        assert_eq!(state.basic.waterfall_speed, 1.0);
        assert_eq!(state.basic.width, 1920);
    }
}

#[cfg(test)]
mod data_curve_tests {
    use super::*;

    /// 数据曲线子结构的默认值与旧字段逐一对应。
    #[test]
    fn data_curve_defaults_snapshot() {
        let state = VideoExportDialogState::new();
        assert_eq!(state.data_curve.graph_duration, "2.0");
        assert_eq!(state.data_curve.bg_color, "#000000");
        assert_eq!(state.data_curve.line_color, "#00FFFF");
        assert!(!state.data_curve.abbreviate);
        assert_eq!(state.data_curve.abbreviate_digits, "3");
    }

    /// 数据曲线与其它域互不影响。
    #[test]
    fn data_curve_section_is_independent() {
        let mut state = VideoExportDialogState::new();
        state.data_curve.line_thickness = "9".to_string();
        assert_eq!(state.counter.font_size, 40);
        assert_eq!(state.miditrail.z_far, MIDITRAIL_Z_FAR_DEFAULT);
    }
}

#[cfg(test)]
mod basic_tests {
    use super::*;

    /// 基础子结构的默认值与旧字段逐一对应。
    #[test]
    fn basic_defaults_snapshot() {
        let state = VideoExportDialogState::new();
        assert_eq!(state.basic.container, "MP4");
        assert_eq!(state.basic.codec, "H.264");
        assert_eq!(state.basic.width, 1920);
        assert_eq!(state.basic.height, 1080);
        assert_eq!(state.basic.fps, 60);
        assert_eq!(state.basic.midi_path, "");
        assert!(matches!(state.basic.overlay, VideoExportOverlayState::None));
    }

    /// 基础域与计数器 / 数据曲线互不影响。
    #[test]
    fn basic_section_is_independent() {
        let mut state = VideoExportDialogState::new();
        state.basic.width = 1280;
        assert_eq!(state.counter.font_size, 40);
        assert_eq!(state.data_curve.graph_duration, "2.0");
    }
}
