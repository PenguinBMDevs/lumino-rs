//! 设置面板模块
//!
//! 该模块已拆分为以下子模块：
//! - `pages`: 各设置页面（常规、音频、界面、快捷键、关于）
//! - `components`: 可复用组件（样式、常量）
//! - `menu`: 设置面板菜单渲染
//! - `panel`: 状态模型与事件处理（`SettingsPanel::new`/`update`）
//! - `tests`: 单元测试

pub mod components;
pub mod menu;
pub mod pages;
mod panel;
#[cfg(test)]
mod tests;

use iced_core::{Border, Length};
use iced_widget::{column, container, row, scrollable, text};

use lumino_core::storage::config::{SynthBackend, TrackAddBehavior};
use lumino_extras::i18n::Language;
use lumino_ui_core::{Element, Message, Theme, window};

use components::*;
use pages::*;

pub use lumino_ui_core::settings_event::Event;

/// 合成器与音频输出设置
#[derive(Debug, Clone)]
pub struct SynthSettings {
    /// 合成器后端类型
    pub backend: SynthBackend,
    /// 音频引擎后端（当前仅 Realtime）
    pub audio_engine: lumino_core::storage::config::AudioEngineKind,
    /// 音色库文件路径（SF2/SFZ）
    pub soundfont_path: String,
    /// 是否使用原生系统标题栏
    pub use_native_titlebar: bool,
    /// xsynth 缓冲时长（毫秒）
    pub xsynth_buffer_ms: f64,
    /// xsynth 采样率
    pub xsynth_sample_rate: u32,
    /// xsynth 渲染线程数
    pub xsynth_threads: i32,
    /// xsynth 是否启用渐弱终止
    pub xsynth_fade_out: bool,
    /// 每个键的最大并发音点数
    pub xsynth_max_voices_per_key: Option<usize>,
    /// LGS (GPU) 缓冲区大小（GPU 块大小，2 的幂，默认 512）
    pub lgs_block_size: usize,
    /// LGS (GPU) 每个 (通道, 键) 最大同音数（0=不限制，默认 4）
    pub lgs_max_voices_per_key: usize,
}

/// 编辑行为设置（橡皮/框选/字体/历史/自动化/Tempo/音轨）
#[derive(Debug, Clone)]
pub struct EditingSettings {
    /// 橡皮擦行为
    pub eraser_behavior: lumino_core::storage::config::EraserBehavior,
    /// 框选模式
    pub selection_box_mode: lumino_core::storage::config::SelectionBoxMode,
    /// 程序界面字体名称
    pub program_font_name: String,
    /// 程序界面字体文件路径
    pub program_font_path: String,
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
    /// Tempo 面板 BPM 绘制上限（默认 512）
    pub tempo_max_bpm: f64,
    /// 自定义 BPM 上限输入面板是否打开
    pub tempo_custom_open: bool,
    /// 自定义 BPM 上限输入框内容
    pub tempo_custom_input: String,
    /// 添加音轨行为
    pub track_add_behavior: TrackAddBehavior,
}

/// 界面显示设置（HiDPI/256键/力度样式/语言/键色/调色板）
#[derive(Debug, Clone)]
pub struct DisplaySettings {
    /// HiDPI 图标渲染（true=2x 清晰，false=1x 零额外开销）
    pub icon_hidpi: bool,
    /// 256键扩展钢琴卷帘
    pub enable_256key: bool,
    /// 力度面板显示样式（true=曲线折线图，false=柱状图）
    pub velocity_curve_style: bool,
    /// 界面语言
    pub language: Language,
    /// 播放键盘颜色指示
    pub playback_key_colors_enabled: bool,
    /// 当前选中的调色板名称
    pub selected_palette: String,
    /// 可用调色板名称列表
    pub available_palettes: Vec<&'static str>,
}

/// 自动滚动设置
#[derive(Debug, Clone)]
pub struct AutoScrollSettings {
    /// 固定指示器位置
    pub fixed_position: u32,
    /// 翻页触发偏移
    pub page_trigger_offset: u32,
    /// 翻页返回位置
    pub page_return_position: u32,
}

/// MIDI 设备设置
#[derive(Debug, Clone)]
pub struct MidiSettings {
    /// 可用的 MIDI 输入设备列表
    pub devices: Vec<(u32, String)>,
    /// 当前选中的 MIDI 输入设备 ID
    pub selected_device: Option<u32>,
    /// 力度过滤阈值
    pub velocity_filter_threshold: u8,
}

/// 高精度洋葱皮贴图设置
#[derive(Debug, Clone)]
pub struct HiresSettings {
    /// 是否启用高精度洋葱皮贴图
    pub onion_enabled: bool,
    /// 每组小节数
    pub measures_per_group: u32,
    /// 贴图瓦片宽度（像素）
    pub tile_width_px: u32,
    /// 重新生成冷却时间（秒）
    pub cooldown_secs: u64,
    /// GPU 显存使用上限（MB）
    pub gpu_mem_limit_mb: u32,
}

/// 日志与监控设置
#[derive(Debug, Clone)]
pub struct LoggingSettings {
    /// 日志文件保留份数
    pub log_retention_count: usize,
    /// 底边栏监控数据刷新间隔（毫秒，50-2000，默认 100）
    pub monitor_refresh_interval_ms: f32,
}

/// 云存储设置
#[derive(Debug, Clone)]
pub struct CloudSettings {
    /// 云存储连接列表（云管理页，由 runner 注入快照）
    pub connections: Vec<CloudConnItem>,
    /// 云存储断连/失败提醒（始终显示，实时更新）
    pub alert: Option<String>,
}

/// 设置面板状态（按配置类别分组的子结构聚合）。
#[derive(Debug, Clone)]
pub struct SettingsPanel {
    /// 当前选中的菜单索引
    pub selected_menu_index: usize,
    /// 合成器与音频输出
    pub synth: SynthSettings,
    /// 编辑行为（橡皮/框选/字体/历史/自动化/Tempo/音轨）
    pub editing: EditingSettings,
    /// 界面显示（HiDPI/256键/力度样式/语言/键色/调色板）
    pub display: DisplaySettings,
    /// 自动滚动
    pub auto_scroll: AutoScrollSettings,
    /// MIDI 设备
    pub midi: MidiSettings,
    /// 高精度洋葱皮
    pub hires: HiresSettings,
    /// 日志与监控
    pub logging: LoggingSettings,
    /// 云存储
    pub cloud: CloudSettings,
}

/// 云存储连接条目（设置面板云管理页展示）
#[derive(Debug, Clone)]
pub struct CloudConnItem {
    /// 连接 ID
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 协议显示名
    pub protocol: String,
    /// 服务器地址
    pub address: String,
    /// 是否在线
    pub online: bool,
}

/// 渲染设置面板主视图
pub fn view<'a>(
    settings: &'a SettingsPanel,
    window: &'a window::Window,
    system_fonts: &'a [lumino_note_core::font_scanner::FontInfo],
) -> Element<'a> {
    let menu_items = menu::create_menu_items(settings.display.language);

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
        7 => cloud_view(settings),
        8 => about_view(settings),
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
