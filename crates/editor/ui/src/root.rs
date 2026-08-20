//! Root 模块 - 应用程序根组件
//!
//! 该模块已拆分为以下子模块：
//! - `handlers`: 消息处理器主入口
//! - `collaboration`: 协作功能处理器
//! - `view`: 视图渲染
//! - `editor_ops`: 编辑器操作
//! - `state`: 状态同步（云存储快照、素材扫描、播放帧）
//! - `arrangement`: 工程走带视图（最大 tick 缓存、自动滚动、播放状态）
//! - `document`: MIDI 文档挂载（自动化 lane 重建）
//! - `memory`: 内存占用快照收集

use crate::Theme;
use crate::state::root_state::{DialogType, RootState};
use crate::{editor, right_sidebar, settings, sidebar, statusbar, titlebar, toolbar, window};
use lumino_core::storage::config::UiConfig;

pub use lumino_ui_core::visual_state::VisualState;

/// 根组件各组件的内存占用快照（字节和计数）
#[derive(Debug, Clone, Default)]
pub struct MemoryBreakdown {
    /// 编辑器内各组件的细分
    pub editor: editor::EditorMemory,
    /// track_midi_events HashMap 中的总条目数和估算字节
    pub track_midi_events_entries: usize,
    /// track_midi_events 数据的估算字节数
    pub track_midi_events_bytes: usize,
    /// note_instances_buffer 三缓冲信息（由 Host::memory_breakdown 填充）
    pub note_instances_writer_cap: usize,
    /// 写入缓冲当前占用长度
    pub note_instances_writer_len: usize,
    /// 就绪缓冲容量
    pub note_instances_ready_cap: usize,
    /// 就绪缓冲当前占用长度
    pub note_instances_ready_len: usize,
    /// 读取缓冲容量
    pub note_instances_reading_cap: usize,
    /// 读取缓冲当前占用长度
    pub note_instances_reading_len: usize,
    /// 单个音符实例的大小（字节）
    pub note_instance_size: usize,
}

mod arrangement;
mod collaboration;
mod document;
mod editor_ops;
pub mod handlers;
mod memory;
mod state;
mod view;

pub use editor_ops::dialog::ProjectSettingsDialogData;

/// 应用程序根组件
pub struct Root {
    pub(crate) sidebar: sidebar::Sidebar,
    /// 右侧栏
    pub right_sidebar: right_sidebar::RightSidebar,
    /// 全屏瀑布流播放器状态（仅在 `AppMode::Waterfall` 下渲染，铺满主界面）
    pub(crate) waterfall_player: crate::right_sidebar::piano_waterfall::WaterfallPlayerState,
    pub(crate) titlebar: titlebar::Titlebar,
    pub(crate) statusbar: statusbar::StatusBar,
    /// 工具栏
    pub toolbar: toolbar::Toolbar,
    /// 编辑器
    pub editor: editor::Editor,
    /// 音轨总览视图
    pub arrangement_view: editor::arrangement::ArrangementView,
    pub(crate) window: window::Window,
    pub(crate) settings: settings::SettingsPanel,
    pub(crate) progress: Option<(String, f64)>,
    pub(crate) is_progress_window: bool,
    /// 当前窗口是否使用系统标题栏；弹窗不应在该模式下再绘制自制标题栏。
    pub(crate) use_native_titlebar: bool,
    /// UI 状态
    pub(crate) state: RootState,
    /// 播放状态（播放管理器、Tempo 变化、MIDI 输出等）
    pub playback: crate::state::playback_state::PlaybackState,
    /// 视觉/渲染状态（洋葱皮缓存、力度面板等）
    pub visual: lumino_ui_core::visual_state::VisualState,
    /// MIDI 连接状态（文档引用、输入连接、缓冲区、API）
    pub midi: crate::state::midi_state::MidiConnectionState,
    /// 录制状态
    pub recording: editor::recording::RecordingState,
    /// Toast 通知管理器（用于编辑拦截/操作反馈等临时通知）
    pub toast: crate::toast::ToastManager,
    /// 窗口最大化/还原保护标志，阻止路由意外切换
    pub window_resize_guard: bool,
    /// 图片转 MIDI 后台转换结果接收端（`right_sidebar::ConvertResult`）
    pub(crate) pending_i2m: Option<std::sync::mpsc::Receiver<crate::right_sidebar::ConvertResult>>,
    /// 素材扫描结果接收端（后台扫描完成后的素材列表）
    pub(crate) pending_material_scan:
        Option<std::sync::mpsc::Receiver<Vec<crate::right_sidebar::MaterialEntry>>>,
    /// 图片转 MIDI 转换前的工具，√ 写入成功后还原
    pub(crate) i2m_restore_tool: Option<lumino_message::Tool>,
    /// 云存储 UI 状态（连接表单 / 文件浏览）
    pub cloud: crate::state::cloud_state::CloudUiState,
}

/// Root 构造参数
struct RootInitParams {
    theme: String,
    ui_config: UiConfig,
    is_progress_window: bool,
    dialog_type: Option<crate::state::root_state::DialogType>,
}

impl Root {
    /// 内部构造函数（消除 new/new_progress/new_dialog 的重复代码）
    fn from_params(params: RootInitParams) -> Self {
        puffin::profile_scope!("root_from_params");
        // 使用 UI 内存标签包裹 Root 各子组件初始化，便于内存监控归因
        lumino_diagnostics::memtrace::with_tag(lumino_diagnostics::memtrace::AllocTag::Ui, || {
            let mut state = RootState::new();
            if let Some(dt) = params.dialog_type {
                state.is_dialog_window = true;
                state.dialog_type = dt;
            }

            // 应用已保存的自动滚动配置到 Editor 和 Toolbar，
            // 否则它们始终使用 AutoScrollConfig::default() 导致用户设置不生效。
            let mut editor = editor::Editor::new();
            editor.editor_state.auto_scroll = params.ui_config.auto_scroll;
            let mut toolbar = toolbar::Toolbar::new();
            toolbar.auto_scroll_mode = params.ui_config.auto_scroll.mode;

            Self {
                sidebar: sidebar::Sidebar::new(),
                right_sidebar: right_sidebar::RightSidebar::new(),
                waterfall_player:
                    crate::right_sidebar::piano_waterfall::WaterfallPlayerState::default(),
                titlebar: titlebar::Titlebar::new(),
                statusbar: statusbar::StatusBar::new(),
                toolbar,
                editor,
                arrangement_view: editor::arrangement::ArrangementView::new(),
                window: window::Window::new(&params.theme),
                settings: settings::SettingsPanel::new(&params.ui_config),
                progress: None,
                is_progress_window: params.is_progress_window,
                use_native_titlebar: params.ui_config.use_native_titlebar,
                state,
                playback: crate::state::playback_state::PlaybackState::new(),
                visual: lumino_ui_core::visual_state::VisualState::new(
                    params.ui_config.velocity_filter_threshold,
                    crate::editor::velocity::VELOCITY_PANEL_HEIGHT,
                ),
                midi: crate::state::midi_state::MidiConnectionState::new(),
                recording: editor::recording::RecordingState::new(),
                toast: crate::toast::ToastManager::new(),
                window_resize_guard: false,
                pending_i2m: None,
                pending_material_scan: None,
                i2m_restore_tool: None,
                cloud: crate::state::cloud_state::CloudUiState::default(),
            }
        })
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
        // 同步框选框显示模式
        root.editor
            .set_selection_box_mode(ui_config.selection_box_mode);
        // 同步力度过滤阈值
        root.visual.velocity_filter_threshold = ui_config.velocity_filter_threshold;
        // 应用 256 键初始配置
        if ui_config.enable_256key {
            root.editor.set_visible_key_count(256);
            root.editor.editor_state.view.key_count = 256;
        }
        // 同步播放键盘颜色配置（防止重启后配置被默认值覆盖）
        root.editor
            .set_playback_key_colors_enabled(ui_config.playback_key_colors_enabled);
        // 同步自动化曲线连线粗细
        root.editor.velocity_panel.automation_line_thickness = ui_config.automation_line_thickness;
        // 同步 Tempo 面板 BPM 绘制上限
        root.editor.velocity_panel.tempo_max_bpm = ui_config.tempo_max_bpm;
        // 初始音轨 0 是指挥轨道 → 速度面板应为 Tempo 模式
        root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Tempo;
        root
    }

    /// 创建进度窗口 Root
    pub fn new_progress(theme: &str, ui_config: &UiConfig) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: ui_config.clone(),
            is_progress_window: true,
            dialog_type: None,
        })
    }

    /// 创建对话框 Root
    pub fn new_dialog(theme: &str, dialog_type: DialogType) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: UiConfig::default(),
            is_progress_window: false,
            dialog_type: Some(dialog_type),
        })
    }

    /// 创建对话框 Root（使用主窗口的标题栏配置）
    pub fn new_dialog_with_config(
        theme: &str,
        dialog_type: DialogType,
        ui_config: &UiConfig,
    ) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: ui_config.clone(),
            is_progress_window: false,
            dialog_type: Some(dialog_type),
        })
    }

    /// 创建设置对话框 Root（使用主窗口的配置）
    pub fn new_settings_dialog(theme: &str, ui_config: &UiConfig) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: ui_config.clone(),
            is_progress_window: false,
            dialog_type: Some(crate::state::root_state::DialogType::Settings),
        })
    }

    /// 获取当前主题
    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    /// 检查当前是否为音轨总览模式
    pub fn is_arrangement_mode(&self) -> bool {
        self.sidebar.is_arrangement_route()
    }

    /// 获取状态可变引用
    pub fn state_mut(&mut self) -> &mut RootState {
        &mut self.state
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        &self.settings
    }
}

#[cfg(test)]
mod root_tests;
