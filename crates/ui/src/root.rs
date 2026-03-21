//! Root 模块 - 应用程序根组件
//!
//! 该模块已拆分为以下子模块：
//! - `handlers`: 消息处理器主入口
//! - `collaboration`: 协作功能处理器
//! - `view`: 视图渲染
//! - `editor_ops`: 编辑器操作

use iced_core::Length;
use iced_widget::container;
use lumino_gfx::NoteInstance;

use crate::state::root_state::RootState;
use crate::{editor, message, settings, sidebar, statusbar, titlebar, toolbar, window};
use lumino_core::storage::config::UiConfig;

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
}

impl Root {
    /// 创建新的 Root
    pub fn new(ui_config: &UiConfig) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(&ui_config.theme),
            settings: settings::SettingsPanel::new(ui_config),
            progress: None,
            is_progress_window: false,
            state: RootState::new(),
        }
    }

    /// 创建进度窗口 Root
    pub fn new_progress(theme: &str) -> Self {
        let default_config = UiConfig::default();
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            settings: settings::SettingsPanel::new(&default_config),
            progress: None,
            is_progress_window: true,
            state: RootState::new(),
        }
    }

    /// 创建对话框 Root
    pub fn new_dialog(theme: &str) -> Self {
        use crate::state::root_state::DialogType;
        let default_config = UiConfig::default();
        let mut state = RootState::new();
        state.is_dialog_window = true;
        state.dialog_type = DialogType::CustomPrecision;

        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            settings: settings::SettingsPanel::new(&default_config),
            progress: None,
            is_progress_window: false,
            state,
        }
    }

    /// 获取当前主题
    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        &self.settings
    }

    /// 获取编辑器引用
    pub fn editor_ref(&self) -> &editor::Editor {
        &self.editor
    }
}
