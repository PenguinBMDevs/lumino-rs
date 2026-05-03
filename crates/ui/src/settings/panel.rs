//! 设置面板状态管理

use super::Event;
use lumino_core::storage::config::SynthBackend;

#[derive(Debug, Clone)]
pub struct SettingsPanel {
    pub selected_menu_index: usize,
    pub synth_backend: SynthBackend,
    pub soundfont_path: String,
    pub use_native_titlebar: bool,
    pub xsynth_buffer_ms: f64,
    pub xsynth_threads: i32,
    pub xsynth_fade_out: bool,
    pub xsynth_max_voices_per_key: Option<usize>,
    pub eraser_behavior: lumino_core::storage::config::EraserBehavior,
    pub program_font_name: String,
    pub program_font_path: String,
    pub auto_scroll_fixed_position: u32,
    pub auto_scroll_page_trigger_offset: u32,
    pub auto_scroll_page_return_position: u32,
    pub velocity_filter_threshold: u8,
}

impl SettingsPanel {
    pub fn new(ui_config: &lumino_core::storage::config::UiConfig) -> Self {
        Self {
            selected_menu_index: 0,
            synth_backend: ui_config.preferred_backend,
            soundfont_path: ui_config.soundfont_path.clone(),
            use_native_titlebar: ui_config.use_native_titlebar,
            xsynth_buffer_ms: ui_config.xsynth_buffer_ms,
            xsynth_threads: ui_config.xsynth_threads,
            xsynth_fade_out: ui_config.xsynth_fade_out_killing,
            xsynth_max_voices_per_key: ui_config.xsynth_max_voices_per_key,
            eraser_behavior: ui_config.eraser_behavior,
            program_font_name: ui_config.program_font_name.clone(),
            program_font_path: ui_config.program_font_path.clone(),
            auto_scroll_fixed_position: ui_config.auto_scroll.fixed_indicator_position,
            auto_scroll_page_trigger_offset: ui_config.auto_scroll.page_trigger_offset,
            auto_scroll_page_return_position: ui_config.auto_scroll.page_return_position,
            velocity_filter_threshold: ui_config.velocity_filter_threshold,
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
            Event::XSynthThreadsChanged(t) => {
                self.xsynth_threads = t;
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
            Event::VelocityFilterThresholdChanged(value) => {
                if let Ok(val) = value.parse::<u8>() {
                    self.velocity_filter_threshold = val;
                }
            }
        }
    }
}
