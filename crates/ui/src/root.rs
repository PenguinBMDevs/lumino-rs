//! Root 模块 - 应用程序根组件
//!
//! 该模块已拆分为以下子模块：
//! - `handlers`: 消息处理器主入口
//! - `collaboration`: 协作功能处理器
//! - `view`: 视图渲染
//! - `editor_ops`: 编辑器操作

use crate::Theme;
use crate::state::root_state::{DialogType, RootState};
use crate::{editor, settings, sidebar, statusbar, titlebar, toolbar, window};
use lumino_core::storage::config::UiConfig;
use lumino_midi_loader::MidiDocument;
use std::sync::Arc;

pub use lumino_ui_core::visual_state::VisualState;

/// 根组件各组件的内存占用快照（字节和计数）
#[derive(Debug, Clone, Default)]
pub struct MemoryBreakdown {
    /// 编辑器内各组件的细分
    pub editor: editor::EditorMemory,
    /// track_midi_events HashMap 中的总条目数和估算字节
    pub track_midi_events_entries: usize,
    pub track_midi_events_bytes: usize,
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
    pub playback: lumino_ui_core::playback_state::PlaybackState,
    /// 视觉/渲染状态（洋葱皮缓存、力度面板等）
    pub visual: lumino_ui_core::visual_state::VisualState,
    /// MIDI 连接状态（文档引用、输入连接、缓冲区、API）
    pub midi: lumino_ui_core::midi_state::MidiConnectionState,
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
        // 使用 UI 内存标签包裹 Root 各子组件初始化，便于内存监控归因
        lumino_memtrace::with_tag(lumino_memtrace::AllocTag::Ui, || {
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
                titlebar: titlebar::Titlebar::new(),
                statusbar: statusbar::StatusBar::new(),
                toolbar,
                editor,
                arrangement_view: editor::arrangement::ArrangementView::new(),
                window: window::Window::new(&params.theme),
                settings: settings::SettingsPanel::new(&params.ui_config),
                progress: None,
                is_progress_window: params.is_progress_window,
                state,
                playback: lumino_ui_core::playback_state::PlaybackState::new(),
                visual: lumino_ui_core::visual_state::VisualState::new(
                    params.ui_config.velocity_filter_threshold,
                    crate::editor::velocity::VELOCITY_PANEL_HEIGHT,
                ),
                midi: lumino_ui_core::midi_state::MidiConnectionState::new(),
                recording: editor::recording::RecordingState::new(),
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
            Some(manager.current_tick())
        } else {
            None
        }
    }

    /// 获取工程走带视图的最大 tick 终点（缓存，按 track_notes_gen 失效）
    ///
    /// 播放时每帧需要计算最大滚动范围，全量扫描 track_notes 在大型 MIDI 下会导致主线程卡顿。
    /// 使用 EditorData::track_notes_gen 作为缓存版本号，只在音符数据变化时重新计算。
    pub fn arrangement_max_tick_end(&mut self) -> f32 {
        let data = &self.editor.editor_state.data;
        let vp = &mut self.arrangement_view.viewport;
        let current_gen = data.track_notes_gen;
        if vp.cached_track_notes_gen != current_gen {
            vp.cached_max_tick_end = data
                .track_notes
                .values()
                .flat_map(|notes| notes.iter().map(|n| n.tick + n.length))
                .fold(0.0_f32, f32::max);
            vp.cached_track_notes_gen = current_gen;
        }
        vp.cached_max_tick_end
            .max(crate::constants::editor::DEFAULT_MIN_TICKS)
    }

    /// 更新工程走带视图的自动滚动（基于编辑器自动滚动配置）
    /// 使演奏指示线的滚动模式在工程走带界面同样适用
    pub fn update_arrangement_auto_scroll(&mut self, playback_tick: f32) {
        let asc = *self.editor.auto_scroll_config();
        if asc.mode == lumino_core::storage::config::AutoScrollMode::Off {
            return;
        }

        // 先计算缓存的最大 tick（可能扫描 track_notes），再借用 viewport
        let max_tick = self.arrangement_max_tick_end();

        let vp = &mut self.arrangement_view.viewport;
        let viewport_width = vp.canvas_size.x.max(1.0);
        let ppu = vp.zoom_x.max(0.001);

        // 计算最大滚动值（使用视口尺寸和总宽度）
        let canvas_w = vp.canvas_size.x.max(1.0);
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

    /// 设置 MIDI 文档引用（供懒加载使用）
    ///
    /// 将控制事件（CC / PitchBend）按音轨导入 automation_lanes，与 Yinhe 的自动化数据模型对齐。
    pub fn set_midi_document(&mut self, doc: Arc<MidiDocument>) {
        use lumino_core::{AutomationEdit, AutomationTarget, SegmentShape};

        // 每次加载新文档时重建自动化 lane，避免旧数据残留。
        self.editor.editor_state.data.automation_lanes.clear();

        for ev in &doc.control_events {
            match ev.kind {
                // CC: param 高 8 位为控制器编号，低 8 位为值。
                0 => {
                    let controller = (ev.param >> 8) as u8;
                    let value = ev.param & 0xFF;
                    let edit = AutomationEdit::Add {
                        track_idx: ev.track,
                        target: AutomationTarget::CC { controller },
                        tick: ev.tick,
                        value,
                        shape: SegmentShape::Step,
                    };
                    self.editor.editor_state.data.apply_automation_edit(edit);
                }
                // PitchBend: param 为 14-bit 值（0–16383）。
                2 => {
                    let edit = AutomationEdit::Add {
                        track_idx: ev.track,
                        target: AutomationTarget::PitchBend,
                        tick: ev.tick,
                        value: ev.param,
                        shape: SegmentShape::Step,
                    };
                    self.editor.editor_state.data.apply_automation_edit(edit);
                }
                _ => {}
            }
        }

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

        MemoryBreakdown {
            editor: editor_mem,
            track_midi_events_entries,
            track_midi_events_bytes,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod root_tests;
