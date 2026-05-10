//! Root 模块 - 应用程序根组件
//!
//! 该模块已拆分为以下子模块：
//! - `handlers`: 消息处理器主入口
//! - `collaboration`: 协作功能处理器
//! - `view`: 视图渲染
//! - `editor_ops`: 编辑器操作

use crate::state::root_state::RootState;
use crate::{editor, message, settings, sidebar, statusbar, titlebar, toolbar, window};
use lumino_core::midi::MidiDocument;
use lumino_core::storage::config::UiConfig;
use std::sync::Arc;

/// 根组件各组件的内存占用快照（字节和计数）
#[derive(Debug, Clone, Default)]
pub struct MemoryBreakdown {
    /// 编辑器内各组件的细分
    pub editor: editor::EditorMemory,
    /// track_midi_events HashMap 中的总条目数和估算字节
    pub track_midi_events_entries: usize,
    pub track_midi_events_bytes: usize,
    /// cached_onion_skin_notes 的字节数
    pub cached_onion_skin_bytes: usize,
    /// note_instances_buffer 双缓冲信息（由 Host::memory_breakdown 填充）
    pub note_instances_front_cap: usize,
    pub note_instances_front_len: usize,
    pub note_instances_back_cap: usize,
    pub note_instances_back_len: usize,
    pub note_instance_size: usize,
}

mod collaboration;
mod editor_ops;
pub mod handlers;
mod view;

pub type Message = message::Message;
pub type Theme = iced_core::Theme;
pub type Renderer = iced_wgpu::Renderer;
pub type Element<'a> = iced_core::Element<'a, Message, Theme, Renderer>;

/// 应用程序根组件
pub struct Root {
    pub(crate) sidebar: sidebar::Sidebar,
    pub(crate) titlebar: titlebar::Titlebar,
    pub(crate) statusbar: statusbar::StatusBar,
    pub toolbar: toolbar::Toolbar,
    pub editor: editor::Editor,
    pub(crate) window: window::Window,
    pub(crate) settings: settings::SettingsPanel,
    pub(crate) progress: Option<(String, f64)>,
    pub(crate) is_progress_window: bool,
    /// UI 状态
    pub(crate) state: RootState,
    /// 播放管理器
    pub(crate) playback_manager: Option<crate::playback::PlaybackManager>,
    /// 延迟应用的 Tempo 变化（播放管理器未初始化时缓存）
    pub(crate) pending_tempo_changes: Option<Vec<crate::playback::TempoChange>>,
    /// 延迟应用的 MIDI 输出（播放管理器未初始化时缓存）
    pub(crate) pending_midi_output: Option<Box<dyn lumino_midi::OutputConnection>>,
    /// 各音轨的 MIDI 控制事件（CC/PC/PB），供播放时使用
    pub(crate) track_midi_events:
        std::collections::HashMap<usize, Vec<crate::playback::MidiTrackEvent>>,
    /// 洋葱皮音符原始数据缓存（tick, key, length, color）
    /// 存原始数据而非 NoteInstance，因为 NoteInstance 含屏幕坐标（随 scroll/zoom 变化）
    pub(crate) cached_onion_skin_notes: Option<Vec<(f32, u16, f32, iced_core::Color)>>,
    /// 缓存失效计数器（只有音轨数据/开关变化才递增）
    pub(crate) onion_skin_generation: u64,
    /// 力度过滤阈值
    pub(crate) velocity_filter_threshold: u8,
    /// 力度面板高度（可拖拽调整）
    pub(crate) velocity_panel_height: f32,
    /// MIDI 文档引用（用于懒加载非当前音轨的音符，避免全量 preload）
    pub(crate) midi_document: Option<Arc<MidiDocument>>,
}

/// Root 构造参数
struct RootInitParams {
    theme: String,
    ui_config: UiConfig,
    is_progress_window: bool,
    dialog_type: Option<crate::state::root_state::DialogType>,
}

impl Root {
    /// 内部构造函数，消除 new/new_progress/new_dialog 的重复代码
    fn from_params(params: RootInitParams) -> Self {
        let mut state = RootState::new();
        if let Some(dt) = params.dialog_type {
            state.is_dialog_window = true;
            state.dialog_type = dt;
        }

        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(&params.theme),
            settings: settings::SettingsPanel::new(&params.ui_config),
            progress: None,
            is_progress_window: params.is_progress_window,
            state,
            playback_manager: None,
            pending_tempo_changes: None,
            pending_midi_output: None,
            track_midi_events: std::collections::HashMap::new(),
            cached_onion_skin_notes: None,
            onion_skin_generation: 0,
            velocity_filter_threshold: params.ui_config.velocity_filter_threshold,
            velocity_panel_height: crate::editor::velocity::VELOCITY_PANEL_HEIGHT,
            midi_document: None,
        }
    }

    /// 创建新的 Root
    pub fn new(ui_config: &UiConfig) -> Self {
        let mut root = Self::from_params(RootInitParams {
            theme: ui_config.theme.clone(),
            ui_config: ui_config.clone(),
            is_progress_window: false,
            dialog_type: None,
        });
        // 同步橡皮擦行为配置到编辑器
        root.editor.set_eraser_behavior(ui_config.eraser_behavior);
        // 同步力度过滤阈值
        root.velocity_filter_threshold = ui_config.velocity_filter_threshold;
        root
    }

    /// 创建进度窗口 Root
    pub fn new_progress(theme: &str) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: UiConfig::default(),
            is_progress_window: true,
            dialog_type: None,
        })
    }

    /// 创建对话框 Root
    pub fn new_dialog(theme: &str) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: UiConfig::default(),
            is_progress_window: false,
            dialog_type: Some(crate::state::root_state::DialogType::CustomPrecision),
        })
    }

    /// 获取当前主题
    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    /// 获取状态可变引用
    pub fn state_mut(&mut self) -> &mut RootState {
        &mut self.state
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        &self.settings
    }

    /// 获取编辑器引用
    pub fn editor_ref(&self) -> &editor::Editor {
        &self.editor
    }

    /// 更新播放状态（应在主循环中定期调用）
    pub fn update_playback(&mut self) -> Option<f32> {
        if let Some(manager) = &mut self.playback_manager {
            if manager.state() != crate::playback::PlaybackState::Playing {
                return None;
            }
            manager.update();
            Some(manager.current_tick())
        } else {
            None
        }
    }

    /// 获取播放状态
    pub fn is_playing(&self) -> bool {
        self.playback_manager
            .as_ref()
            .map(|m| m.state() == crate::playback::PlaybackState::Playing)
            .unwrap_or_default()
    }

    /// 标记洋葱皮缓存失效（任何影响洋葱皮渲染的变化都调用）
    pub fn invalidate_onion_skin_cache(&mut self) {
        self.onion_skin_generation += 1;
        self.editor.invalidate_onion_skin_cache();
    }

    /// 设置 MIDI 文档引用（供懒加载使用）
    pub fn set_midi_document(&mut self, doc: Arc<MidiDocument>) {
        self.midi_document = Some(doc);
    }

    /// 收集各组件的内存占用快照
    pub fn memory_breakdown(&self) -> MemoryBreakdown {
        let editor_mem = self.editor.memory_breakdown();

        // track_midi_events: HashMap<usize, Vec<MidiTrackEvent>>
        let track_midi_events_entries = self.track_midi_events.len();
        let track_midi_events_bytes = self
            .track_midi_events
            .values()
            .map(|v| v.capacity() * std::mem::size_of::<crate::playback::MidiTrackEvent>())
            .sum();

        // cached_onion_skin_notes: Option<Vec<(f32, u16, f32, Color)>>
        // tuple = 4 + 2 + 4 + 16 = 26 bytes, with alignment ~28 bytes
        let cached_onion_skin_bytes = self
            .cached_onion_skin_notes
            .as_ref()
            .map(|v| v.capacity() * 28)
            .unwrap_or(0);

        MemoryBreakdown {
            editor: editor_mem,
            track_midi_events_entries,
            track_midi_events_bytes,
            cached_onion_skin_bytes,
            ..Default::default()
        }
    }
}
