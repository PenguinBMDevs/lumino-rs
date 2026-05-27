use std::sync::Arc;
use winit::event_loop::ControlFlow;

use super::dialog_manager::{DialogManager, DialogResult};
use super::file_handler::FileHandler;
use super::midi_handler::MidiHandler;
use super::midi_manager::{MidiManager, handle_audio_action};
use super::progress_manager::ProgressManager;
use super::window_manager::WindowManager;
use crate::services::collaboration_service::CollaborationService;
use crate::services::file_service::FileService;
use crate::storage;

pub use lumino_core::ParsedDms;
pub use lumino_core::ParsedMidi;

/// Runner 初始化错误
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("存储初始化失败: {0}")]
    Storage(#[from] std::io::Error),
    #[error("窗口初始化失败: {0}")]
    Window(String),
}

#[derive(Default)]
pub struct Runner {
    pub(crate) inner: Option<RunnerInner>,
    pub(crate) init_error: Option<InitError>,
    pub(crate) test_config: Option<crate::cli::TestConfig>,
    pub(crate) log_memory_usage: bool,
}

/// 窗口与 UI 状态
pub(crate) struct WindowState {
    pub(crate) window: WindowManager,
    pub(crate) storage: storage::Storage,
    pub(crate) needs_window_restart: bool,
    pub(crate) dialog_manager: DialogManager,
    pub(crate) progress: ProgressManager,
    pub(crate) progress_cb: lumino_core::midi::loader::ProgressCallback,
}

/// MIDI 相关状态
pub(crate) struct MidiState {
    pub(crate) midi: MidiManager,
    pub(crate) current_midi: Option<Arc<ParsedMidi>>,
    pub(crate) current_midi_source: Option<std::path::PathBuf>,
    pub(crate) current_dms: Option<Arc<ParsedDms>>,
    pub(crate) midi_handler: MidiHandler,
}

/// 文件操作状态
pub(crate) struct FileState {
    pub(crate) file_handler: FileHandler,
    pub(crate) file_service: FileService,
    pub(crate) pending_load_path: Option<std::path::PathBuf>,
}

/// 协作功能状态
pub(crate) struct CollabState {
    pub(crate) collaboration_status: CollaborationStatus,
    pub(crate) pending_invite_code: Option<String>,
    pub(crate) collaboration_service: CollaborationService,
    pub(crate) last_collab_sync: Option<std::time::Instant>,
}

/// 测试与调试状态
pub(crate) struct TestState {
    pub(crate) test_mode_state: Option<TestModeState>,
    pub(crate) log_memory_usage: bool,
    pub(crate) last_memory_log: Option<std::time::Instant>,
}

pub(crate) struct RunnerInner {
    pub(crate) window_state: WindowState,
    pub(crate) midi_state: MidiState,
    pub(crate) file_state: FileState,
    pub(crate) collab_state: CollabState,
    pub(crate) test_state: TestState,
}

pub(crate) struct TestModeState {
    pub active: bool,
    pub start_time: Option<std::time::Instant>,
    pub duration: Option<u64>,
    pub fps_samples: Vec<f32>,
    pub last_fps_update: Option<std::time::Instant>,
    pub frame_count: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum CollaborationStatus {
    #[default]
    Disconnected,
    Connecting,
}

impl Runner {
    /// 设置测试配置
    pub fn set_test_config(&mut self, config: crate::cli::TestConfig) {
        self.test_config = Some(config);
    }

    /// 设置是否启用 memory-usage 日志
    pub fn set_log_memory_usage(&mut self, enabled: bool) {
        self.log_memory_usage = enabled;
    }

    pub(crate) fn init_inner(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<RunnerInner, InitError> {
        let storage = storage::Storage::new()?;

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        // 创建主窗口管理器
        let mut window = WindowManager::new(event_loop, ui_state, &config.ui)
            .map_err(|e| InitError::Window(e.to_string()))?;

        // 创建进度管理器
        let (progress, progress_tx) = ProgressManager::new();
        let progress_cb = lumino_core::midi::loader::progress_from_sender(progress_tx);

        // 创建 MIDI 管理器
        let mut midi = MidiManager::from_config(&config.ui);

        // 为播放引擎创建独立的 MIDI 输出连接（用于新项目的播放功能）
        // 这样用户自绘音符在点击播放按钮时能正常发声
        if let Some(output) = midi.create_additional_output() {
            window.ui_mut().set_playback_midi_output(output);
            tracing::info!("Runner: 播放引擎 MIDI 输出连接已就绪");
        } else {
            tracing::error!("Runner: 无法创建播放引擎 MIDI 输出，播放将无声");
        }

        // 为录制功能创建独立的 MIDI 输入 API
        if let Some(input_api) = midi.create_input_api() {
            window.ui_mut().set_midi_api(input_api);
            tracing::info!("Runner: MIDI 输入 API 已就绪，录制功能可用");
        } else {
            tracing::warn!("Runner: 无法创建 MIDI 输入 API，录制功能不可用");
        }

        // 创建对话框管理器
        let dialog_manager = DialogManager::new();

        let collaboration_status = CollaborationStatus::Disconnected;

        // 创建新的处理器和服务
        let file_handler = FileHandler::new();
        let midi_handler = MidiHandler::new();
        let file_service = FileService::new(progress_cb.clone());
        let collaboration_service = CollaborationService::new();

        event_loop.set_control_flow(ControlFlow::Wait);

        #[cfg(target_os = "macos")]
        if let Err(e) = crate::platform::macos::init() {
            tracing::error!("Failed to init macOS menu: {:?}", e);
        }

        let runner = RunnerInner {
            window_state: WindowState {
                window,
                storage,
                dialog_manager,
                progress,
                progress_cb,
                needs_window_restart: false,
            },
            midi_state: MidiState {
                midi,
                current_midi: None,
                current_midi_source: None,
                current_dms: None,
                midi_handler,
            },
            file_state: FileState {
                file_handler,
                file_service,
                pending_load_path: None,
            },
            collab_state: CollabState {
                collaboration_status,
                collaboration_service,
                pending_invite_code: None,
                last_collab_sync: None,
            },
            test_state: TestState {
                test_mode_state: None,
                log_memory_usage: self.log_memory_usage,
                last_memory_log: None,
            },
        };

        Ok(runner)
    }
}

impl RunnerInner {
    pub(crate) fn process_audio_actions(window: &mut WindowManager, midi: &mut MidiManager) {
        let actions = window.ui_mut().take_audio_actions();

        if !actions.is_empty() {
            tracing::debug!("Runner: 处理 {} 个音频动作", actions.len());
        }

        for action in actions {
            if let Some(output) = midi.output_mut() {
                handle_audio_action(output, action);
            }
        }
    }

    pub(crate) fn apply_dialog_result_to_ui(ui: &mut lumino_ui::Host, result: DialogResult) {
        match result {
            DialogResult::CustomPrecision {
                numerator,
                denominator,
            } => {
                tracing::info!("应用自定义精度: {}/{}", numerator, denominator);

                // 应用到主窗口的编辑器
                if let (Ok(num), Ok(den)) = (numerator.parse::<f32>(), denominator.parse::<f32>()) {
                    // 从编辑器状态获取实际的 PPQ 值
                    let ppq = ui.ppq();
                    let ticks = (ppq as f32) * 4.0 * num / den;

                    ui.set_custom_precision(ticks);
                    tracing::info!("自定义精度已应用: {} ticks (PPQ={})", ticks, ppq);
                }
            }
            DialogResult::LoadConfirm => {
                // LoadConfirm 由 lifecycle.rs 处理，这里不应到达
                tracing::warn!("LoadConfirm 结果不应通过 apply_dialog_result_to_ui 处理");
            }
        }
    }
    /// 保存存储
    /// 简化空字符串显示（避免重复的三元表达式）
    fn display_or_empty(s: &str) -> &str {
        if s.is_empty() { "(空)" } else { s }
    }

    pub(crate) fn save_storage(&mut self) {
        // 获取新旧配置
        let new = self.window_state.window.ui().settings();
        let old = &self.window_state.storage.config.get().ui;

        // 逐一检查各项设置变更
        if new.synth_backend != old.preferred_backend || new.soundfont_path != old.soundfont_path {
            tracing::info!(
                "合成器设置已改变: backend {} -> {}, soundfont {} -> {}",
                old.preferred_backend,
                new.synth_backend,
                Self::display_or_empty(&old.soundfont_path),
                Self::display_or_empty(&new.soundfont_path),
            );
            self.midi_state.midi.mark_for_reinit();
        }

        if new.use_native_titlebar != old.use_native_titlebar {
            tracing::info!(
                "标题栏设置已改变: native_titlebar {} -> {}",
                old.use_native_titlebar,
                new.use_native_titlebar
            );
            self.window_state.needs_window_restart = true;
        }

        if new.program_font_name != old.program_font_name
            || new.program_font_path != old.program_font_path
        {
            tracing::info!(
                "字体设置已改变: font_name {} -> {}, font_path {} -> {}",
                Self::display_or_empty(&old.program_font_name),
                Self::display_or_empty(&new.program_font_name),
                Self::display_or_empty(&old.program_font_path),
                Self::display_or_empty(&new.program_font_path),
            );
            self.window_state.needs_window_restart = true;
        }

        // 保存配置
        self.window_state.storage.config.patch(|config| {
            config.ui.preferred_backend = new.synth_backend;
            config.ui.soundfont_path = new.soundfont_path.clone();
            config.ui.use_native_titlebar = new.use_native_titlebar;
            config.ui.program_font_name = new.program_font_name.clone();
            config.ui.program_font_path = new.program_font_path.clone();
        });

        if let Err(e) = self.window_state.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.window_state.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }

    /// 重启窗口（标题栏设置变更后）
    pub(crate) fn restart_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        tracing::info!("正在重启窗口以应用标题栏设置...");

        // 保存当前窗口状态
        let is_maximized = self.window_state.window.window().is_maximized();

        // 销毁当前窗口并创建新窗口
        let ui_state = self.window_state.storage.ui_state.get();
        let config = self.window_state.storage.config.get();

        // 创建新的窗口管理器
        match WindowManager::new(event_loop, ui_state, &config.ui) {
            Ok(new_window) => {
                // 替换窗口管理器
                self.window_state.window = new_window;

                // 恢复窗口最大化状态
                if is_maximized {
                    self.window_state.window.window().set_maximized(true);
                }

                tracing::info!("窗口重启完成");
            }
            Err(e) => {
                tracing::error!("重启窗口失败: {}", e);
            }
        }
    }
}
