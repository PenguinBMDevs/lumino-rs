//! 设置面板模块
//!
//! 该模块已拆分为以下子模块：
//! - `pages`: 各设置页面（常规、音频、界面、快捷键、关于）
//! - `components`: 可复用组件（样式、常量）
//! - `menu`: 设置面板菜单渲染

pub mod components;
pub mod menu;
pub mod pages;

use iced_core::{Border, Length};
use iced_widget::{column, container, row, scrollable, text};

use lumino_core::storage::config::{SynthBackend, TrackAddBehavior};
use lumino_extras::i18n::Language;
use lumino_ui_core::{Element, Message, Theme, window};

use components::*;
use pages::*;

pub use lumino_ui_core::settings_event::Event;

#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub selected_menu_index: usize,
    pub synth_backend: SynthBackend,
    pub soundfont_path: String,
    pub use_native_titlebar: bool,
    pub xsynth_buffer_ms: f64,
    pub xsynth_sample_rate: u32,
    pub xsynth_threads: i32,
    pub xsynth_fade_out: bool,
    pub xsynth_max_voices_per_key: Option<usize>,
    pub eraser_behavior: lumino_core::storage::config::EraserBehavior,
    pub selection_box_mode: lumino_core::storage::config::SelectionBoxMode,
    pub program_font_name: String,
    pub program_font_path: String,
    // 自动滚动配置
    pub auto_scroll_fixed_position: u32,
    pub auto_scroll_page_trigger_offset: u32,
    pub auto_scroll_page_return_position: u32,
    // 力度过滤
    pub velocity_filter_threshold: u8,
    /// HiDPI 图标渲染（true=2x 清晰，false=1x 零额外开销）
    pub icon_hidpi: bool,
    /// 256键扩展钢琴卷帘
    pub enable_256key: bool,
    /// 力度面板显示样式（true=曲线折线图，false=柱状图）
    pub velocity_curve_style: bool,
    /// 可用的 MIDI 输入设备列表
    pub midi_devices: Vec<(u32, String)>,
    /// 当前选中的 MIDI 输入设备 ID
    pub selected_midi_device: Option<u32>,
    /// 界面语言
    pub language: Language,
    // 高精度洋葱皮贴图设置
    pub hires_onion_enabled: bool,
    pub hires_measures_per_group: u32,
    pub hires_tile_width_px: u32,
    pub hires_cooldown_secs: u64,
    pub hires_gpu_mem_limit_mb: u32,
    // 播放键盘颜色指示
    pub playback_key_colors_enabled: bool,
    /// 添加音轨行为
    pub track_add_behavior: TrackAddBehavior,
    /// 当前选中的调色板名称
    pub selected_palette: String,
    /// 可用调色板名称列表
    pub available_palettes: Vec<&'static str>,
    // 编辑设置
    /// 操作日志总条数上限（建议 50-200，默认 100）
    pub history_total_limit: usize,
    /// 单条日志条目上限（建议 500-2000，默认 1000）
    pub history_entry_limit: usize,
    /// 合并窗口毫秒（仅 Pencil 绘制，0=不合并，默认 300）
    pub merge_window_ms: u64,
    /// 编辑拦截时是否显示 Toast 提示
    pub intercept_notification_enabled: bool,
    /// 自动化曲线连线粗细（像素，1-10）
    pub automation_line_thickness: f32,
    /// 日志文件保留份数
    pub log_retention_count: usize,
    /// 底边栏监控数据刷新间隔（毫秒，50-2000，默认 100）
    pub monitor_refresh_interval_ms: f32,
}

impl SettingsPanel {
    pub fn new(ui_config: &lumino_core::storage::config::UiConfig) -> Self {
        let palette_mgr = &*lumino_extras::palette::PALETTE_MANAGER;
        let available_palettes = palette_mgr.names().to_vec();
        let selected_palette = if ui_config.selected_palette.is_empty() {
            palette_mgr.default().name.to_string()
        } else {
            palette_mgr
                .resolve_name(&ui_config.selected_palette)
                .to_string()
        };

        Self {
            selected_menu_index: 0,
            synth_backend: ui_config.preferred_backend,
            soundfont_path: ui_config.soundfont_path.clone(),
            use_native_titlebar: ui_config.use_native_titlebar,
            xsynth_buffer_ms: ui_config.xsynth_buffer_ms,
            xsynth_sample_rate: ui_config.xsynth_sample_rate,
            xsynth_threads: ui_config.xsynth_threads,
            xsynth_fade_out: ui_config.xsynth_fade_out_killing,
            xsynth_max_voices_per_key: ui_config.xsynth_max_voices_per_key,
            eraser_behavior: ui_config.eraser_behavior,
            selection_box_mode: ui_config.selection_box_mode,
            program_font_name: ui_config.program_font_name.clone(),
            program_font_path: ui_config.program_font_path.clone(),
            auto_scroll_fixed_position: ui_config.auto_scroll.fixed_indicator_position,
            auto_scroll_page_trigger_offset: ui_config.auto_scroll.page_trigger_offset,
            auto_scroll_page_return_position: ui_config.auto_scroll.page_return_position,
            velocity_filter_threshold: ui_config.velocity_filter_threshold,
            icon_hidpi: ui_config.icon_hidpi,
            enable_256key: ui_config.enable_256key,
            velocity_curve_style: ui_config.velocity_curve_style,
            midi_devices: Vec::new(),
            selected_midi_device: None,
            language: ui_config.language,
            hires_onion_enabled: ui_config.hires_onion_enabled,
            hires_measures_per_group: ui_config.hires_measures_per_group,
            hires_tile_width_px: ui_config.hires_tile_width_px,
            hires_cooldown_secs: ui_config.hires_cooldown_secs,
            hires_gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
            playback_key_colors_enabled: ui_config.playback_key_colors_enabled,
            track_add_behavior: ui_config.track_add_behavior,
            selected_palette,
            available_palettes,
            history_total_limit: ui_config.history_total_limit,
            history_entry_limit: ui_config.history_entry_limit,
            merge_window_ms: ui_config.merge_window_ms,
            intercept_notification_enabled: ui_config.intercept_notification_enabled,
            automation_line_thickness: ui_config.automation_line_thickness,
            log_retention_count: ui_config.log_retention_count,
            monitor_refresh_interval_ms: ui_config.monitor_refresh_interval_ms,
        }
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::MenuSelected(idx) => {
                self.selected_menu_index = idx;
            }
            Event::SynthBackendChanged(backend) => {
                self.synth_backend = backend;
            }
            Event::SoundfontPathChanged(path) => {
                self.soundfont_path = path;
            }
            Event::BrowseSoundfont => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("音色库文件", &["sf2", "sfz"])
                    .add_filter("SF2 文件", &["sf2"])
                    .add_filter("SFZ 文件", &["sfz"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    self.soundfont_path = path.to_string_lossy().into_owned();
                }
            }
            Event::NativeTitlebarChanged(enabled) => {
                self.use_native_titlebar = enabled;
            }
            Event::XSynthBufferChanged(ms) => {
                self.xsynth_buffer_ms = ms;
            }
            Event::XSynthSampleRateChanged(sr) => {
                self.xsynth_sample_rate = sr;
            }
            Event::XSynthFadeOutChanged(f) => {
                self.xsynth_fade_out = f;
            }
            Event::XSynthMaxVoicesChanged(v) => {
                self.xsynth_max_voices_per_key = v;
            }
            Event::ThemeChanged(_) => {
                // 主题变更由外部处理
            }
            Event::EraserBehaviorChanged(behavior) => {
                self.eraser_behavior = behavior;
            }
            Event::SelectionBoxModeChanged(mode) => {
                self.selection_box_mode = mode;
            }
            Event::ProgramFontNameChanged(name) => {
                self.program_font_name = name;
            }
            Event::ProgramFontPathChanged(path) => {
                self.program_font_path = path;
            }
            Event::BrowseProgramFont => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("字体文件", &["ttf", "otf", "ttc", "woff", "woff2"])
                    .add_filter("TrueType 字体", &["ttf"])
                    .add_filter("OpenType 字体", &["otf"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                {
                    self.program_font_path = path.to_string_lossy().into_owned();
                }
            }
            // 自动滚动配置事件
            Event::AutoScrollFixedPositionChanged(value) => {
                if let Ok(val) = value.parse::<u32>() {
                    self.auto_scroll_fixed_position = val;
                }
            }
            Event::AutoScrollPageTriggerOffsetChanged(value) => {
                if let Ok(val) = value.parse::<u32>() {
                    self.auto_scroll_page_trigger_offset = val;
                }
            }
            Event::AutoScrollPageReturnPositionChanged(value) => {
                if let Ok(val) = value.parse::<u32>() {
                    self.auto_scroll_page_return_position = val;
                }
            }
            // 力度过滤
            Event::VelocityFilterThresholdChanged(value) => {
                if let Ok(val) = value.parse::<u8>() {
                    self.velocity_filter_threshold = val;
                }
            }
            Event::IconHiDPIChanged(enabled) => {
                self.icon_hidpi = enabled;
            }
            Event::Enable256keyChanged(enabled) => {
                self.enable_256key = enabled;
            }
            Event::VelocityCurveStyleChanged(enabled) => {
                self.velocity_curve_style = enabled;
            }
            Event::DeviceSelected(id) => {
                self.selected_midi_device = Some(id);
                tracing::debug!("设置: MIDI 输入设备选择为 #{}", id);
            }
            Event::LanguageChanged(lang) => {
                self.language = lang;
                tracing::debug!("设置: 界面语言切换为 {:?}", lang);
            }
            // 高精度洋葱皮贴图设置
            Event::HiresOnionEnabledChanged(v) => {
                self.hires_onion_enabled = v;
            }
            Event::HiresMeasuresPerGroupChanged(s) => {
                if let Ok(v) = s.parse::<u32>() {
                    self.hires_measures_per_group = v.clamp(1, 16);
                }
            }
            Event::HiresTileWidthChanged(s) => {
                if let Ok(v) = s.parse::<u32>() {
                    self.hires_tile_width_px = v.clamp(480, 7680);
                }
            }
            Event::HiresCooldownChanged(s) => {
                if let Ok(v) = s.parse::<u64>() {
                    self.hires_cooldown_secs = v.clamp(3, 60);
                }
            }
            Event::HiresGpuMemLimitChanged(s) => {
                if let Ok(v) = s.parse::<u32>() {
                    // 用户硬约束：不得限制 GPU 内存使用——移除 clamp(128, 4096)
                    self.hires_gpu_mem_limit_mb = v;
                }
            }
            Event::PlaybackKeyColorsEnabledChanged(v) => {
                self.playback_key_colors_enabled = v;
            }
            Event::TrackAddBehaviorChanged(v) => {
                self.track_add_behavior = v;
            }
            Event::PaletteChanged(name) => {
                if let Some(p) = self.available_palettes.iter().find(|n| **n == name) {
                    self.selected_palette = p.to_string();
                    tracing::debug!("设置: 调色板切换为 '{}'", name);
                }
            }
            // 编辑设置
            Event::HistoryTotalLimitChanged(s) => {
                if let Ok(v) = s.parse::<usize>() {
                    self.history_total_limit = v.clamp(10, 1000);
                }
            }
            Event::HistoryEntryLimitChanged(s) => {
                if let Ok(v) = s.parse::<usize>() {
                    self.history_entry_limit = v.clamp(100, 10000);
                }
            }
            Event::MergeWindowMsChanged(s) => {
                if let Ok(v) = s.parse::<u64>() {
                    self.merge_window_ms = v.min(5000);
                }
            }
            Event::InterceptNotificationChanged(enabled) => {
                self.intercept_notification_enabled = enabled;
            }
            Event::AutomationLineThicknessChanged(v) => {
                self.automation_line_thickness = v.clamp(1.0, 10.0);
            }
            Event::LogRetentionCountChanged(s) => {
                if let Ok(v) = s.parse::<usize>() {
                    self.log_retention_count = v;
                }
            }
            Event::MonitorRefreshIntervalChanged(v) => {
                self.monitor_refresh_interval_ms = v.clamp(50.0, 2000.0);
            }
        }
    }
}

/// 渲染设置面板主视图
pub fn view<'a>(
    settings: &'a SettingsPanel,
    window: &'a window::Window,
    system_fonts: &'a [lumino_note_core::font_scanner::FontInfo],
) -> Element<'a> {
    let menu_items = menu::create_menu_items(settings.language);

    let menu_list = menu::render_menu_list(settings, window, &menu_items);
    let content_area = render_content_area(settings, window, system_fonts);

    let main_content = row![
        menu_list,
        iced_widget::space().width(SPACING_MAIN),
        content_area,
    ]
    .spacing(SPACING_MENU_CONTENT)
    .padding(PADDING_CONTENT);

    container(main_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(create_main_container_style())
        .into()
}

fn render_content_area<'a>(
    settings: &'a SettingsPanel,
    window: &'a window::Window,
    system_fonts: &'a [lumino_note_core::font_scanner::FontInfo],
) -> iced_widget::Container<'a, Message, Theme, lumino_ui_core::Renderer> {
    let content = match settings.selected_menu_index {
        0 => general_view(settings),
        1 => audio_view(settings),
        2 => ui_settings_view(settings, window, system_fonts),
        3 => shortcuts_view(settings),
        4 => onion_skin_view(settings),
        5 => palette_view(settings),
        6 => editing_view(settings),
        7 => about_view(settings),
        _ => render_placeholder("设置内容区域").into(),
    };

    let scrollable_content = scrollable(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(8).scroller_width(6),
        ));

    container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(create_content_container_style())
}

fn create_content_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(palette.background.base.color)),
            border: Border::default()
                .rounded(BORDER_RADIUS_CONTENT)
                .width(BORDER_WIDTH)
                .color(palette.background.strong.color),
            shadow: iced_core::Shadow {
                color: iced_core::Color::from_rgba(
                    SHADOW_COLOR_CONTENT[0],
                    SHADOW_COLOR_CONTENT[1],
                    SHADOW_COLOR_CONTENT[2],
                    SHADOW_COLOR_CONTENT[3],
                ),
                offset: iced_core::Vector::new(SHADOW_OFFSET_CONTENT.0, SHADOW_OFFSET_CONTENT.1),
                blur_radius: SHADOW_BLUR_CONTENT,
            },
            text_color: Some(palette.background.base.text),
            snap: false,
        }
    }
}

fn create_main_container_style() -> impl Fn(&Theme) -> container::Style + 'static {
    |theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(iced_core::Background::Color(
                palette.background.weakest.color,
            )),
            text_color: Some(palette.background.base.text),
            snap: false,
            ..Default::default()
        }
    }
}

fn render_placeholder<'a>(
    content: &'a str,
) -> iced_widget::Column<'a, Message, Theme, lumino_ui_core::Renderer> {
    column![
        text("设置")
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        text(content)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style()),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
}
