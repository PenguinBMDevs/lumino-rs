use crate::locale::LanguagePreference;
use crate::types::Language;
use serde::{Deserialize, Serialize};

mod autoscroll;
mod defaults;
mod enums;

pub use autoscroll::{AutoScrollConfig, AutoScrollMode};
pub use enums::{
    AudioEngineKind, EraserBehavior, GpuLoadDisplay, NoteCountDisplay, SelectionBoxMode,
    SynthBackend, TrackAddBehavior,
};

use defaults::*;

/// LGS (GPU) 全局复音上限的「自动」值（REND-016 #139）。
///
/// 与 CPU 的 auto=10000 同口径的硬上限（GPU 声部更廉价故略高）；
/// 用于防止黑 MIDI 声部风暴一次性全量进入 dispatch 导致实时欠载（无声/断续）。
pub const LGS_AUTO_MAX_VOICES: usize = 16_384;

/// 用户界面配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 用户界面配置
    pub ui: UiConfig,
}

/// 用户界面配置默认值
impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
        }
    }
}

/// 用户界面配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    /// 主题名称（例如 "Light"）
    #[serde(default)]
    pub theme: String,
    /// 界面语言（**始终是已解析的生效值**，全仓 UI 直接读它；UI-018）
    #[serde(default)]
    pub language: Language,
    /// 界面语言偏好（UI-018）：`None` = 老配置（未声明偏好，语义等同「固定为当前
    /// `language`」）；全新安装由 [`UiConfig::default`] 写 `Some(System)`。
    ///
    /// 与 `language` 分开存是刻意的：`language` 只表示「现在用哪种语言」，
    /// 本字段记住用户「要不要跟随系统」的**意图**——否则用户手动选定的英文
    /// 会在下次启动被系统语言检测悄悄覆盖。
    #[serde(default)]
    pub language_preference: Option<LanguagePreference>,
    /// 用户偏好的合成器后端（用户设置中选择的）
    #[serde(default = "default_synth_backend")]
    pub preferred_backend: SynthBackend,
    /// 音色库路径
    #[serde(default)]
    pub soundfont_path: String,
    /// 是否使用经典系统标题栏（默认使用自定义标题栏）
    #[serde(default)]
    pub use_native_titlebar: bool,
    /// XSynth 渲染缓冲区大小(毫秒)，影响延迟与音符时值量化（默认 30ms）
    #[serde(default = "default_synth_buffer")]
    pub xsynth_buffer_ms: f64,
    /// XSynth 采样率
    #[serde(default = "default_synth_sample_rate")]
    pub xsynth_sample_rate: u32,
    /// 【已废弃】XSynth 多线程设置：线程策略改由后端按机器核数强制决定，
    /// 本字段不再被读取，保留仅为兼容旧配置文件
    #[serde(default = "default_synth_threads")]
    pub xsynth_threads: i32,
    /// XSynth 每个键允许的最大同音数（None=不限，默认 4）
    /// 调高可减少密集钢琴/快速重复音符/拖音过程中的 voice stealing，但渲染负载线性增加
    #[serde(default = "default_max_voices_per_key")]
    pub xsynth_max_voices_per_key: Option<usize>,
    /// 框选框显示模式
    #[serde(default)]
    pub selection_box_mode: SelectionBoxMode,
    /// 橡皮擦工具行为模式
    #[serde(default)]
    pub eraser_behavior: EraserBehavior,
    /// 程序字体名称（系统字体名称）
    #[serde(default)]
    pub program_font_name: String,
    /// 程序字体路径（自定义字体路径，优先于 program_font_name）
    #[serde(default)]
    pub program_font_path: String,
    /// 自动滚动配置
    #[serde(default)]
    pub auto_scroll: AutoScrollConfig,
    /// 力度过滤阈值（力度 <= 此值的音符不播放，0=关闭过滤，最大127）
    #[serde(default = "default_velocity_filter_threshold")]
    pub velocity_filter_threshold: u8,
    /// XSynth 全局最大并发 voice 数（硬上限/量程）
    /// 设置越低，渲染越快，但并发发音数越少。
    /// None = 自动（引擎默认硬上限 10000，由负载治理器决定运行目标）
    #[serde(default)]
    pub xsynth_global_voice_limit: Option<usize>,
    /// XSynth 复音软目标比例：运行目标 = 比例 × 硬上限（负载反馈只会更低、不会更高）
    /// 默认 1-1/e≈0.632（约 37% 暂态余量）；1-1/e²≈0.865 更激进
    #[serde(default = "default_xsynth_voice_target_ratio")]
    pub xsynth_voice_target_ratio: f64,
    /// XSynth 过载保命闸（软 NPS 闸）：仅在重度过载时临时限速，默认关闭。
    /// 关闭时不存在任何 NoteOn 丢弃路径。
    #[serde(default)]
    pub xsynth_soft_nps_gate: bool,
    /// LGS (GPU) 渲染采样率（Hz），GPU 合成管线以此速率渲染
    #[serde(default = "default_lgs_sample_rate")]
    pub lgs_sample_rate: u32,
    /// LGS (GPU) 每块渲染帧数（GPU 一次 dispatch 的帧数，2 的幂，至少 16）
    #[serde(default = "default_lgs_block_size")]
    pub lgs_block_size: usize,
    /// LGS (GPU) 每个 (通道, 键) 最大同音数
    #[serde(default = "default_lgs_max_voices_per_key")]
    pub lgs_max_voices_per_key: usize,
    /// LGS (GPU) 全局最大复音数（硬上限/量程；None = 自动，见 [`LGS_AUTO_MAX_VOICES`]）
    #[serde(default)]
    pub lgs_global_voice_limit: Option<usize>,
    /// LGS (GPU) 防爆闸（发送端软 NPS 闸；REND-016 #139，默认开启）
    #[serde(default = "default_lgs_soft_nps_gate")]
    pub lgs_soft_nps_gate: bool,
    /// LGS (GPU) 是否使用 64 点 sinc 高质量插值（否则线性插值）
    #[serde(default)]
    pub lgs_use_sinc: bool,
    /// LGS (GPU) 专属响度(力度)过滤阈值：力度 <= 此值的音符不渲染（0=关闭过滤），与 XSynth 全局力度过滤相互独立
    #[serde(default = "default_lgs_velocity_filter_threshold")]
    pub lgs_velocity_filter_threshold: u8,
    /// 状态栏 GPU 缓冲状态指示器总开关（默认开启）
    #[serde(default = "default_true")]
    pub gpu_buffer_status_enabled: bool,
    /// 指示器中的 GOV/DROP 防爆治理指标段（默认开启；总开关关闭时整体隐藏）
    #[serde(default = "default_true")]
    pub gpu_governor_metrics_enabled: bool,
    /// 状态栏 LOAD 显示口径（默认 3 秒短窗平均）
    #[serde(default)]
    pub gpu_load_display: GpuLoadDisplay,
    /// 是否启用 HiDPI 图标渲染（关闭时使用1x获得零性能开销，开启时使用2x获得视网膜清晰度）
    #[serde(default = "default_true")]
    pub icon_hidpi: bool,
    /// 是否启用 256 键扩展钢琴卷帘（默认关闭）
    #[serde(default)]
    pub enable_256key: bool,
    /// 音符总量统计的显示位置（默认工程设置对话框；UI-015）
    #[serde(default)]
    pub note_count_display: NoteCountDisplay,
    /// 力度面板显示样式（默认曲线=折线图，false=柱状图）
    #[serde(default = "default_true")]
    pub velocity_curve_style: bool,
    /// 高精度洋葱皮贴图：是否启用
    #[serde(default = "default_true")]
    pub hires_onion_enabled: bool,
    /// 高精度洋葱皮贴图：每组小节数（1-16）
    #[serde(default = "default_hires_measures_per_group")]
    pub hires_measures_per_group: u32,
    /// 高精度洋葱皮贴图：贴图宽度像素（480-7680）
    #[serde(default = "default_hires_tile_width")]
    pub hires_tile_width_px: u32,
    /// 高精度洋葱皮贴图：编辑后重生成冷静期秒数（3-60）
    #[serde(default = "default_hires_cooldown")]
    pub hires_cooldown_secs: u64,
    /// 高精度洋葱皮贴图：GPU 显存上限 MB（128-4096）
    #[serde(default = "default_hires_gpu_mem_limit")]
    pub hires_gpu_mem_limit_mb: u32,
    /// 播放时键盘颜色指示（默认关闭以节省内存和性能）
    #[serde(default)]
    pub playback_key_colors_enabled: bool,
    /// 添加音轨时的行为（自动跳转到新音轨 / 保持当前音轨）
    #[serde(default)]
    pub track_add_behavior: TrackAddBehavior,
    /// 当前选中的调色板名称（空字符串表示使用默认）
    #[serde(default)]
    pub selected_palette: String,
    /// 编辑历史：操作日志总条数上限（默认 100）
    #[serde(default = "default_history_total_limit")]
    pub history_total_limit: usize,
    /// 编辑历史：单条日志条目上限（默认 1000，超限自动分割）
    #[serde(default = "default_history_entry_limit")]
    pub history_entry_limit: usize,
    /// 编辑历史：合并窗口毫秒数（仅 Pencil 绘制，默认 300ms，0=不合并）
    #[serde(default = "default_merge_window_ms")]
    pub merge_window_ms: u64,
    /// 编辑拦截：是否显示 Toast 提示（默认 true）
    #[serde(default = "default_true")]
    pub intercept_notification_enabled: bool,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）
    #[serde(default = "default_automation_line_thickness")]
    pub automation_line_thickness: f32,
    /// Tempo 面板 BPM 绘制上限（默认 512，可配置 256~65536 或自定义）
    #[serde(default = "default_tempo_max_bpm")]
    pub tempo_max_bpm: f64,
    /// 日志文件保留份数（默认 10，0 = 不限制）
    #[serde(default = "default_log_retention_count")]
    pub log_retention_count: usize,
    /// 底边栏监控数据刷新间隔（毫秒，50-2000，默认 100）
    #[serde(default = "default_monitor_refresh_interval_ms")]
    pub monitor_refresh_interval_ms: f32,
    /// 音频引擎后端（当前仅 Realtime）
    #[serde(default)]
    pub audio_engine: AudioEngineKind,
    /// 系统 MIDI (WinMM) 输出设备 ID（None = 使用系统默认/第一个输出设备）
    #[serde(default)]
    pub system_output_device_id: Option<u32>,
    /// 音频播放输出设备（CPAL 音频设备名；None = 使用系统默认输出设备）
    ///
    /// 仅对软件合成器后端（XSynth / LGS）生效；系统 MIDI / KDMAPI 走 WinMM 播表
    /// （见 `system_output_device_id`），不受此字段影响。设备名来自 CPAL 枚举，
    /// 跨会话不保证稳定，但单次运行内唯一可识别。
    #[serde(default)]
    pub audio_output_device: Option<String>,
    /// 是否在每次启动时执行 GPU 兼容性检查（默认开启；设置页可关闭）
    #[serde(default = "default_true")]
    pub gpu_check_on_startup: bool,
    /// 启动 GPU 兼容性警告是否被抑制（`None` = 全新配置按未抑制处理；
    /// 旧版配置在加载时归一化为 `Some(true)`，实现"老用户升级静默"）
    #[serde(default)]
    pub gpu_warning_suppressed: Option<bool>,
    /// 上次 GPU 检测的适配器指纹（启动缓存比对，避免每次全量试画）
    #[serde(default)]
    pub gpu_last_fingerprint: Option<String>,
    /// 上次 GPU 检测是否通过（配合指纹决定是否可跳过全量检测）
    #[serde(default)]
    pub gpu_last_passed: Option<bool>,
    /// 上次完整 GPU 检测完成时刻（Unix 秒；配合 TTL 决定缓存是否仍可复用）
    ///
    /// 适配器指纹在 macOS（Metal）上恒为常量（`driver` / `driver_info` 均为空串），
    /// 仅靠指纹无法感知系统 / 驱动大版本升级，故缓存必须带时间兜底。
    /// `None` = 旧配置缺该字段或系统时钟异常，一律按"缓存过期"处理（宁可多检不漏检）。
    #[serde(default)]
    pub gpu_last_check_time: Option<u64>,
    /// 是否启用 Domino（TAKABO SOFT）剪贴板互粘（默认关闭；仅 Windows 生效）
    ///
    /// 开启后 Windows 下复制会额外编码一份 `MidiPortalSequence` 格式、粘贴会额外
    /// 尝试解码 Domino 载荷。该编码在 20 万音符选中档位上额外阻塞 UI 线程
    /// 60~150ms 并抬高约 26MB 堆峰值，而多数用户从不与 Domino 互粘，故默认关闭。
    /// 旧配置缺该字段时落 `false`（由 `#[serde(default)]` 提供），升级后保持关闭。
    #[serde(default)]
    pub domino_clipboard_enabled: bool,
    /// 是否启用周期性自动保存（默认关闭）
    ///
    /// 开启后按 [`UiConfig::autosave_interval_mins`] 间隔，把当前工程异步写入
    /// 独立的自动保存文件（配置目录 `autosave/autosave.lmpj`），**不改动用户
    /// 手动保存的 .lmpj 源文件**。崩溃 / 强杀后该文件留存，下次启动弹窗询问
    /// 恢复；正常退出时删除（用户已明确处置过更改，文件失去意义）。
    #[serde(default)]
    pub autosave_enabled: bool,
    /// 自动保存间隔（分钟；读取时经 [`UiConfig::effective_autosave_interval_mins`]
    /// 收敛到 1..=120，防止手输越界值）
    #[serde(default = "default_autosave_interval_mins")]
    pub autosave_interval_mins: u32,
}

/// 界面语言偏好的实际语义（UI-018）。
///
/// 老配置（`language_preference == None`）视为「用户固定为当前 `language`」，而不是
/// 「跟随系统」——这样升级不会把用户手动选过的英文改回中文；想跟随系统的人
/// 只需在设置面板显式选一次「跟随系统」。
impl UiConfig {
    /// 生效的语言偏好（把老配置的 `None` 归一化为 `Fixed(language)`）。
    pub fn effective_language_preference(&self) -> LanguagePreference {
        self.language_preference
            .unwrap_or(LanguagePreference::Fixed(self.language))
    }

    /// 生效的自动保存间隔（分钟）：手输越界值收敛到 1..=120。
    pub fn effective_autosave_interval_mins(&self) -> u32 {
        self.autosave_interval_mins.clamp(1, 120)
    }
}

/// 用户界面配置默认值
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "Light".into(),
            language: Language::default(),
            // 全新安装默认跟随系统语言（UI-018）；老配置反序列化得到 `None`，
            // 走「保留既有 language」分支，不受此处默认值影响。
            language_preference: Some(LanguagePreference::System),
            preferred_backend: SynthBackend::XSynth,
            soundfont_path: String::new(),
            use_native_titlebar: false,
            xsynth_buffer_ms: default_synth_buffer(),
            xsynth_sample_rate: default_synth_sample_rate(),
            xsynth_threads: default_synth_threads(),
            xsynth_max_voices_per_key: default_max_voices_per_key(),
            lgs_sample_rate: default_lgs_sample_rate(),
            lgs_block_size: default_lgs_block_size(),
            lgs_max_voices_per_key: default_lgs_max_voices_per_key(),
            lgs_global_voice_limit: None,
            lgs_soft_nps_gate: true,
            lgs_use_sinc: false,
            lgs_velocity_filter_threshold: default_lgs_velocity_filter_threshold(),
            gpu_buffer_status_enabled: true,
            gpu_governor_metrics_enabled: true,
            gpu_load_display: GpuLoadDisplay::default(),
            selection_box_mode: SelectionBoxMode::default(),
            eraser_behavior: EraserBehavior::default(),
            program_font_name: String::from("Microsoft YaHei"),
            program_font_path: String::new(),
            auto_scroll: AutoScrollConfig::default(),
            velocity_filter_threshold: default_velocity_filter_threshold(),
            xsynth_global_voice_limit: None,
            xsynth_voice_target_ratio: default_xsynth_voice_target_ratio(),
            xsynth_soft_nps_gate: false,
            icon_hidpi: true,
            enable_256key: false,
            note_count_display: NoteCountDisplay::default(),
            velocity_curve_style: true,
            hires_onion_enabled: true,
            hires_measures_per_group: default_hires_measures_per_group(),
            hires_tile_width_px: default_hires_tile_width(),
            hires_cooldown_secs: default_hires_cooldown(),
            hires_gpu_mem_limit_mb: default_hires_gpu_mem_limit(),
            playback_key_colors_enabled: false,
            track_add_behavior: TrackAddBehavior::default(),
            selected_palette: String::new(),
            history_total_limit: default_history_total_limit(),
            history_entry_limit: default_history_entry_limit(),
            merge_window_ms: default_merge_window_ms(),
            intercept_notification_enabled: true,
            automation_line_thickness: default_automation_line_thickness(),
            tempo_max_bpm: default_tempo_max_bpm(),
            log_retention_count: default_log_retention_count(),
            monitor_refresh_interval_ms: default_monitor_refresh_interval_ms(),
            audio_engine: AudioEngineKind::default(),
            system_output_device_id: None,
            audio_output_device: None,
            gpu_check_on_startup: true,
            gpu_warning_suppressed: None,
            gpu_last_fingerprint: None,
            gpu_last_passed: None,
            gpu_last_check_time: None,
            domino_clipboard_enabled: false,
            autosave_enabled: false,
            autosave_interval_mins: default_autosave_interval_mins(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧配置（UI-010 之前写入的 `config.json`）缺少 `gpu_last_check_time`：
    /// 反序列化不得失败，字段落为 `None`，由调用方按"缓存过期"处理。
    #[test]
    fn test_legacy_config_without_gpu_check_time_deserializes_to_none() {
        let mut value =
            serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
        let removed = value
            .as_object_mut()
            .expect("UiConfig 序列化结果应为 JSON 对象")
            .remove("gpu_last_check_time");
        assert!(removed.is_some(), "默认配置应写出 gpu_last_check_time 键");

        let restored: UiConfig =
            serde_json::from_value(value).expect("缺时间戳字段的旧配置应能反序列化");
        assert_eq!(
            restored.gpu_last_check_time, None,
            "旧配置缺时间戳字段应落为 None（按缓存过期处理）"
        );
    }

    /// 新写入的时间戳可正确往返，供启动 TTL 判定读取。
    #[test]
    fn test_gpu_check_time_serde_roundtrip() {
        let config = UiConfig {
            gpu_last_check_time: Some(1_700_000_000),
            ..UiConfig::default()
        };
        let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
        let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
        assert_eq!(restored.gpu_last_check_time, Some(1_700_000_000));
    }

    /// Domino 互粘开关默认关闭：全新配置不得为用不到的 Domino 编码付费。
    #[test]
    fn test_domino_clipboard_defaults_to_disabled() {
        assert!(
            !UiConfig::default().domino_clipboard_enabled,
            "Domino 剪贴板互粘开关默认应为关闭"
        );
    }

    /// 旧配置（UI-016 之前写入的 `config.json`）缺少 `domino_clipboard_enabled`：
    /// 反序列化不得失败，字段落为 `false`（关闭），保证升级后不为 Domino 编码付费。
    #[test]
    fn test_legacy_config_without_domino_clipboard_deserializes_to_false() {
        let mut value =
            serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
        let removed = value
            .as_object_mut()
            .expect("UiConfig 序列化结果应为 JSON 对象")
            .remove("domino_clipboard_enabled");
        assert!(
            removed.is_some(),
            "默认配置应写出 domino_clipboard_enabled 键"
        );

        let restored: UiConfig =
            serde_json::from_value(value).expect("缺 Domino 开关字段的旧配置应能反序列化");
        assert!(
            !restored.domino_clipboard_enabled,
            "旧配置缺该字段应落为 false（关闭）"
        );
    }

    /// 自动保存默认关闭：全新安装不改变用户现有行为（写入与恢复都是用户显式开启后才有）。
    #[test]
    fn test_autosave_defaults_to_disabled() {
        let config = UiConfig::default();
        assert!(!config.autosave_enabled, "自动保存开关默认应为关闭");
        assert_eq!(
            config.autosave_interval_mins, 5,
            "自动保存间隔默认应为 5 分钟"
        );
        assert_eq!(
            config.effective_autosave_interval_mins(),
            5,
            "默认间隔经 clamp 后应保持 5"
        );
    }

    /// 旧配置（自动保存功能之前写入的 `config.json`）缺少两个 autosave 字段：
    /// 反序列化不得失败，开关落 `false`、间隔落默认值，保证升级静默兼容。
    #[test]
    fn test_legacy_config_without_autosave_deserializes_to_defaults() {
        let mut value =
            serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
        let object = value
            .as_object_mut()
            .expect("UiConfig 序列化结果应为 JSON 对象");
        let removed_enabled = object.remove("autosave_enabled");
        let removed_interval = object.remove("autosave_interval_mins");
        assert!(
            removed_enabled.is_some(),
            "默认配置应写出 autosave_enabled 键"
        );
        assert!(
            removed_interval.is_some(),
            "默认配置应写出 autosave_interval_mins 键"
        );

        let restored: UiConfig =
            serde_json::from_value(value).expect("缺自动保存字段的旧配置应能反序列化");
        assert!(
            !restored.autosave_enabled,
            "旧配置缺开关字段应落为 false（关闭）"
        );
        assert_eq!(
            restored.autosave_interval_mins, 5,
            "旧配置缺间隔字段应落为默认值 5"
        );
    }

    /// 间隔收敛：手输越界值（0 / 负数语义不适用 u32，越上界）经
    /// `effective_autosave_interval_mins` 收敛到 1..=120。
    #[test]
    fn test_autosave_interval_effective_clamps() {
        let zero = UiConfig {
            autosave_interval_mins: 0,
            ..UiConfig::default()
        };
        assert_eq!(
            zero.effective_autosave_interval_mins(),
            1,
            "0 分钟应收敛到下界 1"
        );
        let over = UiConfig {
            autosave_interval_mins: 999,
            ..UiConfig::default()
        };
        assert_eq!(
            over.effective_autosave_interval_mins(),
            120,
            "999 分钟应收敛到上界 120"
        );
        let valid = UiConfig {
            autosave_interval_mins: 30,
            ..UiConfig::default()
        };
        assert_eq!(
            valid.effective_autosave_interval_mins(),
            30,
            "合法值应原样通过"
        );
    }

    /// 开启状态可正确往返，供设置面板与持久化读取。
    #[test]
    fn test_domino_clipboard_serde_roundtrip() {
        let config = UiConfig {
            domino_clipboard_enabled: true,
            ..UiConfig::default()
        };
        let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
        let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
        assert!(restored.domino_clipboard_enabled);
    }

    /// 旧配置缺少 GPU 缓冲状态指示器字段时应回落默认
    /// （总开关/GOV 段 = 开，LOAD 口径 = 3 秒平均），不得导致反序列化失败。
    #[test]
    fn test_legacy_config_without_gpu_status_defaults() {
        let mut value =
            serde_json::to_value(UiConfig::default()).expect("UiConfig 应能序列化为 JSON");
        let obj = value
            .as_object_mut()
            .expect("UiConfig 序列化结果应为 JSON 对象");
        for key in [
            "gpu_buffer_status_enabled",
            "gpu_governor_metrics_enabled",
            "gpu_load_display",
        ] {
            assert!(
                obj.remove(key).is_some(),
                "默认配置应写出 {key} 键（以便本用例验证缺字段行为）"
            );
        }

        let restored: UiConfig =
            serde_json::from_value(value).expect("缺 GPU 指示器字段的旧配置应能反序列化");
        assert!(restored.gpu_buffer_status_enabled, "缺字段应落为默认开启");
        assert!(
            restored.gpu_governor_metrics_enabled,
            "缺字段应落为默认开启"
        );
        assert_eq!(
            restored.gpu_load_display,
            GpuLoadDisplay::ShortAverage,
            "缺字段应落为 3 秒平均"
        );
    }

    /// LOAD 口径可正确往返（瞬时模式持久化）。
    #[test]
    fn test_gpu_load_display_serde_roundtrip() {
        let config = UiConfig {
            gpu_load_display: GpuLoadDisplay::Instant,
            gpu_buffer_status_enabled: false,
            ..UiConfig::default()
        };
        let json = serde_json::to_string(&config).expect("UiConfig 应能序列化");
        let restored: UiConfig = serde_json::from_str(&json).expect("UiConfig 应能反序列化");
        assert_eq!(restored.gpu_load_display, GpuLoadDisplay::Instant);
        assert!(!restored.gpu_buffer_status_enabled);
    }
}
