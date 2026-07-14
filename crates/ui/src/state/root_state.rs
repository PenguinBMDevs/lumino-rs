#[path = "toggle_animation.rs"]
pub mod toggle_animation;

use toggle_animation::ToggleAnimationState;

use crate::titlebar::mode_toggle::AppMode;
use crate::toolbar::DotType;
use crate::toolbar::NotePrecision;

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialogState {
    pub is_open: bool,
    pub tuplet_count: String,
    pub note_value: String,
    pub tuplet_type: crate::toolbar::TupletType,
    pub dot_type: DotType,
    pub divisor: String,
}

impl Default for CustomPrecisionDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            tuplet_count: "3".to_string(),
            note_value: "4".to_string(),
            tuplet_type: crate::toolbar::TupletType::Triplet,
            dot_type: DotType::None,
            divisor: "2".to_string(),
        }
    }
}

impl CustomPrecisionDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算自定义精度对应的 tick 数
    pub fn calculate_ticks(&self, ppq: u32) -> Option<f32> {
        let numerator = self.tuplet_count.parse::<f32>().ok()?;
        let denominator = self.note_value.parse::<f32>().ok()?;
        let divisor = self.divisor.parse::<f32>().ok()?;

        if denominator == 0.0 || divisor == 0.0 {
            return None;
        }

        // 计算基础 tick 数
        let base_ticks = (ppq as f32) * 4.0 * numerator / denominator;

        // 应用除数
        let ticks = base_ticks / divisor;

        Some(ticks)
    }
}

/// 工程设置对话框状态
#[derive(Debug, Clone)]
pub struct ProjectSettingsDialogState {
    pub is_open: bool,
    /// 项目名称
    pub title: String,
    /// BPM 速度 (字符串以便编辑)
    pub tempo: String,
    /// 版权信息
    pub copyright: String,
    /// 创建日期 (格式化后的字符串)
    pub created_display: String,
    /// 累计创作时间 (秒)
    pub total_editing_time_seconds: f64,
}

impl Default for ProjectSettingsDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            title: String::new(),
            tempo: "120".to_string(),
            copyright: String::new(),
            created_display: String::new(),
            total_editing_time_seconds: 0.0,
        }
    }
}

impl ProjectSettingsDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 格式化累计创作时间 (自适应单位)
    pub fn format_editing_time(&self) -> String {
        let total_seconds = self.total_editing_time_seconds;
        if total_seconds < 1.0 {
            return "不足 1 秒".to_string();
        }

        let days = (total_seconds / 86400.0) as u64;
        let hours = ((total_seconds % 86400.0) / 3600.0) as u64;
        let minutes = ((total_seconds % 3600.0) / 60.0) as u64;
        let seconds = (total_seconds % 60.0) as u64;

        if days > 0 {
            format!("{} 天 {} 小时 {} 分钟", days, hours, minutes)
        } else if hours > 0 {
            format!("{} 小时 {} 分钟", hours, minutes)
        } else if minutes > 0 {
            format!("{} 分钟 {} 秒", minutes, seconds)
        } else {
            format!("{} 秒", seconds)
        }
    }

    /// 解析 BPM 值 (20-10000)
    pub fn parse_tempo(&self) -> Option<f64> {
        let value = self.tempo.parse::<f64>().ok()?;
        if (20.0..=10000.0).contains(&value) {
            Some(value)
        } else {
            None
        }
    }
}

/// 对话框类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogType {
    #[default]
    None,
    CustomPrecision,
    Collaboration,
    LoadConfirm,
    ProjectSettings,
    Settings,
    SpeedChange,
    ExportProgress,
    VideoExport,
}

/// 协作视图状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollaborationViewState {
    #[default]
    Connect, // 连接服务器界面
    Connecting,  // 正在连接中
    RoomActions, // 创建/加入房间界面
    InRoom,      // 在房间内界面
}

/// 加载确认对话框状态
#[derive(Debug, Clone)]
pub struct LoadConfirmDialogState {
    pub is_open: bool,
    pub file_name: String,
    pub file_path: String,
    pub size_mb: f64,
}

impl Default for LoadConfirmDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            file_name: String::new(),
            file_path: String::new(),
            size_mb: 0.0,
        }
    }
}

/// 协作对话框状态
#[derive(Debug, Clone)]
pub struct CollaborationDialogState {
    pub is_open: bool,
    /// 服务器地址
    pub server_host: String,
    /// 服务器端口
    pub server_port: String,
    /// 用户名
    pub username: String,
    /// 房间名称（创建房间用）
    pub room_name: String,
    /// 邀请码（加入房间用）
    pub invite_code: String,
    /// 当前视图状态
    pub view_state: CollaborationViewState,
    /// 连接状态
    pub connection_status: String,
}

impl Default for CollaborationDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl CollaborationDialogState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            server_host: "localhost".to_string(),
            server_port: "3000".to_string(),
            username: "用户".to_string(),
            room_name: "我的房间".to_string(),
            invite_code: String::new(),
            view_state: CollaborationViewState::Connect,
            connection_status: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.is_open = false;
        self.view_state = CollaborationViewState::Connect;
        self.connection_status.clear();
    }
}

/// 音频导出面板状态（主界面侧边栏面板，非独立对话框）
///
/// 纯 UI 状态，仅保存控件值，不含导出处理逻辑。
#[derive(Debug, Clone)]
pub struct AudioExportDialogState {
    /// 工程名称
    pub project_name: String,
    /// MIDI 文件路径
    pub midi_path: String,
    /// SF2 音色库路径
    pub soundfont_path: String,
    /// 采样率
    pub sample_rate: u32,
    /// 通道数
    pub channels: AudioChannels,
    /// 每通道层数限制
    pub layers: u32,
    /// GPU 导出时最大同时 voice 数（0 = 使用默认值 2048）
    pub max_voices: u32,
    /// 通道多线程
    pub channel_threading: ThreadingOption,
    /// 按键多线程
    pub key_threading: ThreadingOption,
    /// 应用限制器
    pub apply_limiter: bool,
    /// 禁用淡出
    pub disable_fade_out: bool,
    /// 线性包络
    pub linear_envelope: bool,
    /// 插值算法
    pub interpolation: Interpolation,
    /// 输出格式
    pub format: AudioFormat,
    /// 编码比特率（kbps，仅 MP3/Vorbis 有效）
    pub audio_bitrate: u32,
    /// 忽略音色变化事件
    pub ignore_program_changes: bool,
    /// 启用音符力度过滤
    pub filter_velocity: bool,
    /// 最低力度
    pub velocity_low: u8,
    /// 最高力度
    pub velocity_high: u8,
    /// 启用键位过滤
    pub filter_key: bool,
    /// 最低键位
    pub key_low: u8,
    /// 最高键位
    pub key_high: u8,
    /// 音符强制结束延迟（毫秒）
    pub note_force_end_delay: u32,
    /// 输出路径
    pub output_path: String,
    /// 是否正在渲染（显示内嵌进度条）
    pub is_rendering: bool,
    /// 渲染进度消息
    pub render_message: String,
    /// 渲染进度 (0.0 - 1.0)
    pub render_progress: f64,
    /// 渲染是否完成
    pub render_completed: bool,
    /// 渲染错误信息
    pub render_error: Option<String>,
}

/// 音频通道数（UI用）— 重新导出自 lumino-message
pub use lumino_message::{AudioChannels, AudioFormat, Interpolation, ThreadingOption};

impl Default for AudioExportDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioExportDialogState {
    pub fn new() -> Self {
        Self {
            project_name: String::new(),
            midi_path: String::new(),
            soundfont_path: String::new(),
            sample_rate: 48000,
            channels: AudioChannels::default(),
            layers: 32,
            max_voices: 2048,
            channel_threading: ThreadingOption::default(),
            key_threading: ThreadingOption::default(),
            apply_limiter: true,
            disable_fade_out: false,
            linear_envelope: false,
            interpolation: Interpolation::default(),
            format: AudioFormat::default(),
            audio_bitrate: 320,
            ignore_program_changes: false,
            filter_velocity: false,
            velocity_low: 0,
            velocity_high: 127,
            filter_key: false,
            key_low: 0,
            key_high: 127,
            note_force_end_delay: 0,
            output_path: String::new(),
            is_rendering: false,
            render_message: String::new(),
            render_progress: 0.0,
            render_completed: false,
            render_error: None,
        }
    }
}

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
    /// 分辨率宽度
    pub width: u32,
    /// 分辨率高度
    pub height: u32,
    /// 帧率
    pub fps: u32,
    /// 输出路径
    pub output_path: String,
    /// 视频导出渲染模式（强类型枚举）
    pub render_mode: lumino_event::window::video::RenderMode,
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
    /// 预览帧数据（RGBA 格式，压缩后用于 dialog 内显示预览图像）
    pub preview_frame: Option<Vec<u8>>,
    /// 预览帧宽度
    pub preview_width: u32,
    /// 预览帧高度
    pub preview_height: u32,
    /// 缓存的 iced image handle（避免每帧创建唯一 ID 导致 GPU 纹理缓存失效）
    ///
    /// `Handle::from_rgba` 每次调用生成唯一 ID，iced_wgpu 对大图（>2MB）走异步上传，
    /// 每个新 ID 都被视为全新图像重新上传。缓存 handle 后，相同数据复用已上传的纹理。
    pub cached_image_handle: Option<iced_core::image::Handle>,
}

impl Default for VideoExportDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoExportDialogState {
    pub fn new() -> Self {
        Self {
            container: "MP4".to_string(),
            codec: "H.264".to_string(),
            backend: "Software (CPU)".to_string(),
            quality: "中".to_string(),
            width: 1920,
            height: 1080,
            fps: 60,
            output_path: String::new(),
            render_mode: lumino_event::window::video::RenderMode::HiResTexture,
            overlay: VideoExportOverlayState::None,
            progress: 0.0,
            status_message: String::new(),
            current_frame: 0,
            total_frames: 0,
            render_fps: 0.0,
            preview_frame: None,
            preview_width: 0,
            preview_height: 0,
            cached_image_handle: None,
        }
    }

    /// 是否正在导出（覆盖层可见）
    pub fn is_exporting(&self) -> bool {
        !matches!(self.overlay, VideoExportOverlayState::None)
    }
}

/// 音符变速对话框状态
#[derive(Debug, Clone)]
pub struct SpeedChangeDialogState {
    pub is_open: bool,
    /// 倍率输入字符串（支持分数格式如 "1/3"）
    pub factor_input: String,
}

impl SpeedChangeDialogState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            factor_input: "0.5".to_string(),
        }
    }

    /// 解析倍率输入，支持小数和分数格式
    /// 返回解析成功的 f32 值
    pub fn parse_factor(&self) -> Option<f32> {
        let input = self.factor_input.trim();
        if input.is_empty() {
            return None;
        }

        // 尝试解析分数格式（如 "1/3"）
        if let Some(idx) = input.find('/') {
            let numerator = input[..idx].trim().parse::<f32>().ok()?;
            let denominator = input[idx + 1..].trim().parse::<f32>().ok()?;
            if denominator == 0.0 {
                return None;
            }
            let result = numerator / denominator;
            if result > 0.0 {
                return Some(result);
            }
            return None;
        }

        // 尝试解析小数格式
        let value = input.parse::<f32>().ok()?;
        if value > 0.0 { Some(value) } else { None }
    }
}

impl Default for SpeedChangeDialogState {
    fn default() -> Self {
        Self::new()
    }
}

/// 音频导出进度对话框状态
#[derive(Debug, Clone)]
pub struct ExportProgressDialogState {
    /// 是否显示
    pub is_open: bool,
    /// 当前进度消息
    pub message: String,
    /// 进度值 (0.0 - 1.0)
    pub progress: f64,
    /// 是否已完成
    pub is_completed: bool,
    /// 是否出错
    pub error: Option<String>,
}

impl ExportProgressDialogState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            message: String::new(),
            progress: 0.0,
            is_completed: false,
            error: None,
        }
    }

    /// 重置状态
    pub fn reset(&mut self) {
        self.is_open = false;
        self.message.clear();
        self.progress = 0.0;
        self.is_completed = false;
        self.error = None;
    }

    /// 更新进度
    pub fn update_progress(&mut self, message: String, progress: f64) {
        self.message = message;
        self.progress = progress;
    }

    /// 标记完成
    pub fn set_completed(&mut self) {
        self.is_completed = true;
        self.progress = 1.0;
        self.message = "导出完成".to_string();
    }

    /// 标记错误
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error.clone());
        self.message = format!("导出失败: {}", error);
    }
}

impl Default for ExportProgressDialogState {
    fn default() -> Self {
        Self::new()
    }
}

/// Root 组件的状态
pub struct RootState {
    /// 是否有菜单/下拉框打开（打开时不渲染预览音符）
    pub is_menu_open: bool,
    /// 对话框结果（用于独立窗口模式）
    pub dialog_result: Option<crate::host::DialogResult>,
    /// 是否是对话框窗口（用于自定义精度对话框等）
    pub is_dialog_window: bool,
    /// 对话框类型
    pub dialog_type: DialogType,
    /// 自定义精度对话框状态
    pub custom_precision_dialog: CustomPrecisionDialogState,
    /// 加载确认对话框状态
    pub load_confirm_dialog: LoadConfirmDialogState,
    /// 协作对话框状态
    pub collaboration_dialog: CollaborationDialogState,
    /// 工程设置对话框状态
    pub project_settings_dialog: ProjectSettingsDialogState,
    /// 音频导出对话框状态
    pub audio_export_dialog: AudioExportDialogState,
    /// 视频导出对话框状态
    pub video_export_dialog: VideoExportDialogState,
    /// 音频导出进度对话框状态
    pub export_progress_dialog: ExportProgressDialogState,
    /// 音符变速对话框状态
    pub speed_change_dialog: SpeedChangeDialogState,
    /// 精度设置
    pub note_precision: NotePrecision,
    /// 系统字体列表
    pub system_fonts: Vec<lumino_core::font_scanner::FontInfo>,
    /// 当前应用模式（编辑器/瀑布流）
    pub current_mode: AppMode,
    /// 模式切换按钮动画状态
    pub toggle_animation: ToggleAnimationState,
}

impl Default for RootState {
    fn default() -> Self {
        Self::new()
    }
}

impl RootState {
    pub fn new() -> Self {
        Self {
            is_menu_open: false,
            dialog_result: None,
            is_dialog_window: false,
            dialog_type: DialogType::None,
            custom_precision_dialog: CustomPrecisionDialogState::new(),
            load_confirm_dialog: LoadConfirmDialogState::default(),
            collaboration_dialog: CollaborationDialogState::new(),
            project_settings_dialog: ProjectSettingsDialogState::new(),
            audio_export_dialog: AudioExportDialogState::new(),
            video_export_dialog: VideoExportDialogState::new(),
            export_progress_dialog: ExportProgressDialogState::new(),
            speed_change_dialog: SpeedChangeDialogState::new(),
            note_precision: NotePrecision::default(),
            system_fonts: lumino_core::font_scanner::scan_system_fonts(),
            current_mode: AppMode::default(),
            toggle_animation: ToggleAnimationState::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speed_change_dialog_default() {
        let state = SpeedChangeDialogState::new();
        assert!(!state.is_open);
        assert_eq!(state.factor_input, "0.5");
    }

    #[test]
    fn test_speed_change_parse_factor_decimal() {
        let mut state = SpeedChangeDialogState::new();
        state.factor_input = "2.0".to_string();
        assert!(
            (state
                .parse_factor()
                .expect("解析 \"2.0\" 应为有效小数，返回 Some")
                - 2.0)
                .abs()
                < 0.001
        );

        state.factor_input = "0.25".to_string();
        assert!(
            (state
                .parse_factor()
                .expect("解析 \"0.25\" 应为有效小数，返回 Some")
                - 0.25)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn test_speed_change_parse_factor_fraction() {
        let mut state = SpeedChangeDialogState::new();
        state.factor_input = "1/3".to_string();
        assert!(
            (state
                .parse_factor()
                .expect("解析 \"1/3\" 应为有效分数，返回 Some")
                - 1.0 / 3.0)
                .abs()
                < 0.001
        );

        state.factor_input = "3/2".to_string();
        assert!(
            (state
                .parse_factor()
                .expect("解析 \"3/2\" 应为有效分数，返回 Some")
                - 1.5)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn test_speed_change_parse_factor_invalid() {
        let mut state = SpeedChangeDialogState::new();
        state.factor_input = "".to_string();
        assert!(state.parse_factor().is_none());

        state.factor_input = "-1.0".to_string();
        assert!(state.parse_factor().is_none());

        state.factor_input = "abc".to_string();
        assert!(state.parse_factor().is_none());

        state.factor_input = "1/0".to_string();
        assert!(state.parse_factor().is_none());
    }

    #[test]
    fn test_toggle_animation_default() {
        let anim = ToggleAnimationState::default();
        assert!(!anim.active);
        assert!((anim.position - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_toggle_animation_animate_to() {
        let mut anim = ToggleAnimationState::new();
        anim.animate_to(1.0);
        assert!(anim.active);
        assert!((anim.target - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_custom_precision_dialog_default() {
        let state = CustomPrecisionDialogState::new();
        assert!(!state.is_open);
        assert_eq!(state.divisor, "2");
        assert_eq!(state.note_value, "4");
        assert_eq!(state.tuplet_count, "3");
    }

    #[test]
    fn test_project_settings_dialog_default() {
        let state = ProjectSettingsDialogState::new();
        assert!(!state.is_open);
    }

    #[test]
    fn test_collaboration_dialog_default() {
        let state = CollaborationDialogState::new();
        assert!(!state.is_open);
    }

    #[test]
    fn test_root_state_default() {
        let state = RootState::new();
        assert!(!state.is_menu_open);
        assert!(!state.is_dialog_window);
        assert_eq!(state.dialog_type, DialogType::None);
    }
}
