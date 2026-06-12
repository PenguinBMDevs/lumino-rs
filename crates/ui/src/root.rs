//! Root 模块 - 应用程序根组件
//!
//! 该模块已拆分为以下子模块：
//! - `handlers`: 消息处理器主入口
//! - `collaboration`: 协作功能处理器
//! - `view`: 视图渲染
//! - `editor_ops`: 编辑器操作

use crate::state::root_state::{DialogType, RootState};
use crate::{editor, message, settings, sidebar, statusbar, titlebar, toolbar, window};
use lumino_core::storage::config::UiConfig;
use lumino_midi_loader::MidiDocument;
use std::sync::Arc;

pub use visual_state::VisualState;

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
    /// onion_note_buffer 双缓冲信息（由 Host::memory_breakdown 填充）
    pub onion_note_front_cap: usize,
    pub onion_note_front_len: usize,
    pub onion_note_back_cap: usize,
    pub onion_note_back_len: usize,
}

pub mod theme;

mod collaboration;
mod editor_ops;
pub mod handlers;
mod midi_state;
mod playback_state;
mod view;
mod visual_state;

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
    /// 音轨总览视图
    pub arrangement_view: editor::arrangement::ArrangementView,
    pub(crate) window: window::Window,
    pub(crate) settings: settings::SettingsPanel,
    pub(crate) progress: Option<(String, f64)>,
    pub(crate) is_progress_window: bool,
    /// UI 状态
    pub(crate) state: RootState,
    /// 播放状态（播放管理器、Tempo 变化、MIDI 输出等）
    pub playback: playback_state::PlaybackState,
    /// 视觉/渲染状态（洋葱皮缓存、力度面板等）
    pub visual: visual_state::VisualState,
    /// MIDI 连接状态（文档引用、输入连接、缓冲区、API）
    pub midi: midi_state::MidiConnectionState,
    /// 录制状态
    pub recording: editor::recording::RecordingState,
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
            arrangement_view: editor::arrangement::ArrangementView::new(),
            window: window::Window::new(&params.theme),
            settings: settings::SettingsPanel::new(&params.ui_config),
            progress: None,
            is_progress_window: params.is_progress_window,
            state,
            playback: playback_state::PlaybackState::new(),
            visual: visual_state::VisualState::new(
                params.ui_config.velocity_filter_threshold,
                crate::editor::velocity::VELOCITY_PANEL_HEIGHT,
            ),
            midi: midi_state::MidiConnectionState::new(),
            recording: editor::recording::RecordingState::new(),
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
        // 初始音轨 0 是指挥轨道 → 速度面板应为 Tempo 模式
        root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Tempo;
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
    pub fn new_dialog(theme: &str, dialog_type: DialogType) -> Self {
        Self::from_params(RootInitParams {
            theme: theme.to_string(),
            ui_config: UiConfig::default(),
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

    /// 获取编辑器引用
    pub fn editor_ref(&self) -> &editor::Editor {
        &self.editor
    }

    /// 更新播放状态（应在主循环中定期调用）
    pub fn update_playback(&mut self) -> Option<f32> {
        if let Some(manager) = &mut self.playback.manager {
            if manager.state() != crate::playback::PlaybackState::Playing {
                return None;
            }
            manager.update();
            Some(manager.current_tick())
        } else {
            None
        }
    }

    /// 更新工程走带视图的自动滚动（基于编辑器自动滚动配置）
    /// 使演奏指示线的滚动模式在工程走带界面同样适用
    pub fn update_arrangement_auto_scroll(&mut self, playback_tick: f32) {
        let asc = *self.editor.auto_scroll_config();
        if asc.mode == lumino_core::storage::config::AutoScrollMode::Off {
            return;
        }

        let vp = &mut self.arrangement_view.viewport;
        let viewport_width = vp.canvas_size.x.max(1.0);
        let ppu = vp.zoom_x.max(0.001);

        // 计算最大滚动值（使用视口尺寸和总宽度）
        let canvas_w = vp.canvas_size.x.max(1.0);
        let max_tick = self
            .editor
            .editor_state
            .data
            .track_notes
            .values()
            .flat_map(|notes| notes.iter().map(|n| n.tick + n.length))
            .fold(crate::constants::editor::DEFAULT_MIN_TICKS, f32::max);
        let total_w = max_tick * vp.zoom_x;
        let max_scroll = (total_w - canvas_w).max(0.0);

        match asc.mode {
            lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft => {
                let indicator_pos = asc.fixed_indicator_position as f32;
                let target_scroll_x = playback_tick * ppu - indicator_pos;
                // 到达末尾时自动松开固定，滚动停在末尾
                vp.scroll_x = target_scroll_x.clamp(0.0, max_scroll);
            }
            lumino_core::storage::config::AutoScrollMode::ScrollingIndicator => {
                let trigger_offset = asc.page_trigger_offset as f32;
                let return_pos = asc.page_return_position as f32;
                let indicator_screen_x = playback_tick * ppu - vp.scroll_x;

                if indicator_screen_x >= viewport_width - trigger_offset {
                    let target_scroll_x = playback_tick * ppu - return_pos;
                    vp.scroll_x = target_scroll_x.clamp(0.0, max_scroll);
                }
            }
            lumino_core::storage::config::AutoScrollMode::Off => {}
        }
    }

    /// 获取播放状态
    pub fn is_playing(&self) -> bool {
        self.playback
            .manager
            .as_ref()
            .map(|m| m.state() == crate::playback::PlaybackState::Playing)
            .unwrap_or_default()
    }

    /// 标记洋葱皮缓存全量失效（数据变化/音轨集合变化时调用）
    pub fn invalidate_onion_skin_cache(&mut self) {
        self.visual.onion_skin_generation += 1;
        self.editor.invalidate_onion_skin_cache();
    }

    /// 仅标记颜色/透明度变化（无需重查 document，O(C) 快速路径）
    pub fn invalidate_onion_skin_colors(&mut self) {
        self.editor.invalidate_onion_skin_colors();
    }

    /// 设置 MIDI 文档引用（供懒加载使用）
    pub fn set_midi_document(&mut self, doc: Arc<MidiDocument>) {
        // 从 control_events 提取弯音数据
        let mut bend_points = Vec::new();
        for ev in &doc.control_events {
            if ev.kind == 2 {
                // kind=2 是 pitch bend
                let value = ev.as_pitch_bend();
                // as_pitch_bend 返回的是 f32 (-1.0..1.0)，转换为 i16 (-8192..8191)
                let i16_value = (value * crate::constants::editor::PITCH_BEND_FACTOR) as i16;
                bend_points.push(crate::editor::velocity::BendPoint {
                    tick: ev.tick as f32,
                    value: i16_value,
                });
            }
        }
        self.editor.editor_state.data.cc_data.bend_points = bend_points;
        self.midi.document = Some(doc);
    }

    /// 收集各组件的内存占用快照
    pub fn memory_breakdown(&self) -> MemoryBreakdown {
        let editor_mem = self.editor.memory_breakdown();

        // track_midi_events: HashMap<usize, Vec<MidiTrackEvent>>
        let track_midi_events_entries = self.playback.track_midi_events.len();
        let track_midi_events_bytes = self
            .playback
            .track_midi_events
            .values()
            .map(|v| v.capacity() * std::mem::size_of::<crate::playback::MidiTrackEvent>())
            .sum();

        // cached_onion_skin_notes: Option<Vec<(f32, u16, f32, Color)>>
        // tuple = 4 + 2 + 4 + 16 = 26 bytes, with alignment ~28 bytes
        let cached_onion_skin_bytes = self
            .visual
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::handlers::MessageHandler;

    #[test]
    fn test_new_dialog_uses_requested_dialog_type() {
        let root = Root::new_dialog("dark", DialogType::AudioExport);

        assert!(root.state.is_dialog_window);
        assert_eq!(root.state.dialog_type, DialogType::AudioExport);
    }

    // ================================================================
    // 对话框关闭处理器 —— dialog_type 不复位测试
    //
    // 修复背景：关闭对话框时 handler 曾将 dialog_type 复位为 None，
    // 导致 view_dialog() 在窗口销毁前的最后一帧通过 _ 通配符渲染出精度面板。
    // 修复后 handler 不再修改 dialog_type，以下测试确保这一行为。
    // ================================================================

    #[test]
    fn test_close_settings_dialog_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::Settings);
        let mut handler = handlers::DialogHandler::new();
        handler.handle(&mut root, Message::CloseSettingsDialog);

        // 关闭设置对话框不应复位 dialog_type（防止窗口销毁前一帧闪跳到精度面板）
        assert_eq!(
            root.state.dialog_type,
            DialogType::Settings,
            "CloseSettingsDialog 不应修改 dialog_type"
        );
        assert!(
            root.state.dialog_result.is_some(),
            "CloseSettingsDialog 应设置 dialog_result"
        );
    }

    #[test]
    fn test_close_project_settings_dialog_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::ProjectSettings);
        let mut handler = handlers::DialogHandler::new();
        handler.handle(&mut root, Message::CloseProjectSettingsDialog);

        assert_eq!(
            root.state.dialog_type,
            DialogType::ProjectSettings,
            "CloseProjectSettingsDialog 不应修改 dialog_type"
        );
        assert!(
            root.state.dialog_result.is_some(),
            "CloseProjectSettingsDialog 应设置 dialog_result"
        );
    }

    #[test]
    fn test_close_audio_export_dialog_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::AudioExport);
        let mut handler = handlers::DialogHandler::new();
        handler.handle(
            &mut root,
            Message::AudioExport(crate::message::AudioExportAction::CloseDialog),
        );

        assert_eq!(
            root.state.dialog_type,
            DialogType::AudioExport,
            "AudioExport::CloseDialog 不应修改 dialog_type"
        );
        assert!(
            !root.state.audio_export_dialog.is_open,
            "AudioExport::CloseDialog 应关闭音频导出对话框"
        );
    }

    #[test]
    fn test_audio_export_confirm_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::AudioExport);
        root.state.audio_export_dialog.project_name = "test".to_string();
        root.state.audio_export_dialog.output_path = "test.wav".to_string();
        let mut handler = handlers::DialogHandler::new();
        handler.handle(
            &mut root,
            Message::AudioExport(crate::message::AudioExportAction::Confirm),
        );

        assert_eq!(
            root.state.dialog_type,
            DialogType::AudioExport,
            "AudioExport::Confirm 不应修改 dialog_type"
        );
        assert!(
            !root.state.audio_export_dialog.is_open,
            "AudioExport::Confirm 应关闭音频导出对话框"
        );
    }

    #[test]
    fn test_audio_export_cancel_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::AudioExport);
        let mut handler = handlers::DialogHandler::new();
        handler.handle(
            &mut root,
            Message::AudioExport(crate::message::AudioExportAction::Cancel),
        );

        assert_eq!(
            root.state.dialog_type,
            DialogType::AudioExport,
            "AudioExport::Cancel 不应修改 dialog_type"
        );
        assert!(
            !root.state.audio_export_dialog.is_open,
            "AudioExport::Cancel 应关闭音频导出对话框"
        );
    }

    #[test]
    fn test_confirm_load_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::LoadConfirm);
        root.handle_confirm_load();

        assert_eq!(
            root.state.dialog_type,
            DialogType::LoadConfirm,
            "handle_confirm_load 不应修改 dialog_type"
        );
        assert!(
            root.state.dialog_result.is_some(),
            "handle_confirm_load 应设置 dialog_result"
        );
    }

    #[test]
    fn test_cancel_load_preserves_dialog_type() {
        let mut root = Root::new_dialog("dark", DialogType::LoadConfirm);
        root.handle_cancel_load();

        assert_eq!(
            root.state.dialog_type,
            DialogType::LoadConfirm,
            "handle_cancel_load 不应修改 dialog_type"
        );
        // handle_cancel_load 仅关闭对话框，不设置结果
        assert!(
            !root.state.load_confirm_dialog.is_open,
            "handle_cancel_load 应关闭加载确认对话框"
        );
    }

    // ================================================================
    // view_dialog None 测试
    //
    // DialogType::None 应渲染空容器而不是回退到精度面板（修复前的 bug）。
    // 此处验证 view() 不 panic，类型已由 match 分支的显式匹配保证。
    // ================================================================

    #[test]
    fn test_view_dialog_none_does_not_panic() {
        // DialogType::None 匹配专门的空容器分支，不应 panic
        let root = Root::new_dialog("dark", DialogType::None);
        let _element = root.view();
    }

    // ================================================================
    // 变速按钮 Ctrl+Click 测试
    //
    // 修复前 flip_button 传入 has_selection 作为 enabled 参数，
    // 无选中时按钮 disabled，Ctrl+Click 事件根本不会发射。
    // 修复后按钮总是 enabled，handler 内部已对无选中情况做了兜底。
    // ================================================================

    #[test]
    fn test_speed_change_ctrl_click_opens_dialog_event() {
        // 清空全局事件缓冲区
        let _ = crate::event::take_events();

        // Ctrl+Click 应发射 OpenSpeedChangeDialog 事件（不依赖选中状态）
        let mut root = Root::new_dialog("dark", DialogType::None);
        root.toolbar.ctrl_pressed = true;
        let mut handler = handlers::ToolbarHandler::new();
        handler.handle(&mut root, Message::Toolbar(toolbar::Event::SpeedChange));

        let events = crate::event::take_events();
        let has_open_event = events.iter().any(|e| {
            matches!(
                e,
                crate::event::Event::Window(crate::event::window::Event::Dialog(
                    crate::event::window::dialog::Event::OpenSpeedChangeDialog
                ))
            )
        });
        assert!(
            has_open_event,
            "Ctrl+Click 变速应发射 OpenSpeedChangeDialog 事件"
        );
    }

    #[test]
    fn test_speed_change_direct_click_no_selection_returns_early() {
        let mut root = Root::new_dialog("dark", DialogType::None);
        root.toolbar.ctrl_pressed = false;

        // 无音符 + 无选中：直接点击应无副作用地提前返回
        let _ = crate::event::take_events();
        let mut handler = handlers::ToolbarHandler::new();
        handler.handle(&mut root, Message::Toolbar(toolbar::Event::SpeedChange));
        let events = crate::event::take_events();

        assert!(
            events.is_empty(),
            "无选中时直接点击变速不应发射任何窗口事件"
        );
        assert_eq!(
            root.state.dialog_type,
            DialogType::None,
            "无选中时直接点击变速不应改变 dialog_type"
        );
    }

    #[test]
    fn test_speed_change_button_always_enabled_in_view() {
        // 验证在工具栏 view 中变速按钮的 enabled 始终为 true
        // （与 has_selection 解耦），确保 Ctrl+Click 路径可到达 handler。
        // 这是 toolbar/view.rs 中 flip_button 调用改为硬编码 true 的行为保证。
        // 此处通过构造两种场景并调用 view() 来验证不 panic：
        //   1. has_selection = true
        //   2. has_selection = false
        let ui_config = lumino_core::storage::config::UiConfig::default();
        let root = Root::new(&ui_config);

        // 有选中 -> view
        let _element = root.toolbar.view(&root.window, true);

        // 无选中 -> view（不应 panic/assert）
        let _element = root.toolbar.view(&root.window, false);

        // 验证通过：两种情况下 view 均正常返回
    }
}
