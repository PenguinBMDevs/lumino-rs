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
#[derive(Debug)]
pub enum InitError {
    Storage(std::io::Error),
    Window(String),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::Storage(e) => write!(f, "存储初始化失败: {}", e),
            InitError::Window(e) => write!(f, "窗口初始化失败: {}", e),
        }
    }
}

impl std::error::Error for InitError {}

impl From<std::io::Error> for InitError {
    fn from(e: std::io::Error) -> Self {
        InitError::Storage(e)
    }
}

#[derive(Default)]
pub struct Runner {
    pub(crate) inner: Option<RunnerInner>,
    pub(crate) init_error: Option<InitError>,
}

pub(crate) struct RunnerInner {
    /// 主窗口管理器
    pub(crate) window: WindowManager,
    /// 存储
    pub(crate) storage: storage::Storage,
    /// MIDI 管理器
    pub(crate) midi: MidiManager,
    /// 进度管理器
    pub(crate) progress: ProgressManager,
    /// 进度回调（依赖注入，替代全局 PROGRESS_SENDER）
    pub(crate) progress_cb: lumino_core::midi::loader::ProgressCallback,
    /// 当前加载的 MIDI
    pub(crate) current_midi: Option<ParsedMidi>,
    /// 当前加载的 DMS
    pub(crate) current_dms: Option<Arc<ParsedDms>>,
    /// 对话框管理器
    pub(crate) dialog_manager: DialogManager,
    /// 协作状态
    pub(crate) collaboration_status: CollaborationStatus,
    /// 待处理的加入房间邀请码
    pub(crate) pending_invite_code: Option<String>,
    /// 文件处理器
    pub(crate) file_handler: FileHandler,
    /// MIDI 处理器
    pub(crate) midi_handler: MidiHandler,
    /// 文件服务
    pub(crate) file_service: FileService,
    /// 协作服务
    pub(crate) collaboration_service: CollaborationService,
    /// 是否需要重启窗口（标题栏设置变更）
    pub(crate) needs_window_restart: bool,
    /// 上次协作同步时间（用于定时发送鼠标位置）
    pub(crate) last_collab_sync: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum CollaborationStatus {
    #[default]
    Disconnected,
    Connecting,
}

impl Runner {
    pub(crate) fn init_inner(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<RunnerInner, InitError> {
        let storage = storage::Storage::new()?;

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        // 创建主窗口管理器
        let window = WindowManager::new(event_loop, ui_state, &config.ui)
            .map_err(|e| InitError::Window(e.to_string()))?;

        // 创建进度管理器
        let (progress, progress_tx) = ProgressManager::new();
        let progress_cb = lumino_core::midi::loader::progress_from_sender(progress_tx);

        // 创建 MIDI 管理器
        let midi = MidiManager::from_config(&config.ui);

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
            window,
            storage,
            midi,
            progress,
            progress_cb,
            current_midi: None,
            current_dms: None,
            dialog_manager,
            collaboration_status,
            pending_invite_code: None,
            file_handler,
            midi_handler,
            file_service,
            collaboration_service,
            needs_window_restart: false,
            last_collab_sync: None,
        };

        // Debug 模式下自动连接本地服务器
        // #[cfg(debug_assertions)]
        // {
        //     // 生成时间戳格式的用户名
        //     let username = format!(
        //         "debug_{}",
        //         std::time::SystemTime::now()
        //             .duration_since(std::time::UNIX_EPOCH)
        //             .unwrap_or_default()
        //             .as_millis()
        //     );

        //     tracing::info!(
        //         "Debug 模式：自动连接协作服务器 127.0.0.1:3000，用户名: {}",
        //         username
        //     );

        //     // 打开协作对话框并设置 UI 状态为正在连接
        //     runner
        //         .window
        //         .ui_mut()
        //         .open_collaboration_dialog_with_state(
        //             "127.0.0.1".to_string(),
        //             3000,
        //             username.clone(),
        //         );

        //     // 通过正常流程处理连接（这会更新 Runner 的 collaboration_status）
        //     let host = "127.0.0.1".to_string();
        //     let port = 3000u16;
        //     runner.handle_collaboration_connect(
        //         host,
        //         port,
        //         username,
        //         Some("Lumino 房间".to_string()),
        //         None,
        //     );
        // }

        Ok(runner)
    }

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
        }
    }

    /// 检查是否有初始化错误
    pub fn init_error(&self) -> Option<&InitError> {
        self.init_error.as_ref()
    }
}

impl RunnerInner {
    /// 保存存储
    pub(crate) fn save_storage(&mut self) {
        // 获取当前 UI 中的设置
        let new_preferred_backend = self.window.ui().settings().synth_backend;
        let new_soundfont_path = self.window.ui().settings().soundfont_path.clone();
        let new_use_native_titlebar = self.window.ui().settings().use_native_titlebar;
        let new_program_font_name = self.window.ui().settings().program_font_name.clone();
        let new_program_font_path = self.window.ui().settings().program_font_path.clone();

        // 获取当前存储的配置
        let old_config = self.storage.config.get();
        let old_preferred_backend = old_config.ui.preferred_backend;
        let old_soundfont_path = &old_config.ui.soundfont_path;
        let old_use_native_titlebar = old_config.ui.use_native_titlebar;
        let old_program_font_name = &old_config.ui.program_font_name;
        let old_program_font_path = &old_config.ui.program_font_path;

        // 检查合成器相关设置是否改变
        let backend_changed = new_preferred_backend != old_preferred_backend;
        let soundfont_changed = new_soundfont_path != *old_soundfont_path;
        let titlebar_changed = new_use_native_titlebar != old_use_native_titlebar;
        let font_changed = new_program_font_name != *old_program_font_name
            || new_program_font_path != *old_program_font_path;

        if backend_changed || soundfont_changed {
            tracing::info!(
                "合成器设置已改变: backend {} -> {}, soundfont {} -> {}",
                old_preferred_backend,
                new_preferred_backend,
                if old_soundfont_path.is_empty() {
                    "(空)"
                } else {
                    old_soundfont_path
                },
                if new_soundfont_path.is_empty() {
                    "(空)"
                } else {
                    &new_soundfont_path
                }
            );
            // 标记需要重新初始化 MIDI
            self.midi.mark_for_reinit();
        }

        if titlebar_changed {
            tracing::info!(
                "标题栏设置已改变: native_titlebar {} -> {}",
                old_use_native_titlebar,
                new_use_native_titlebar
            );
            // 标记需要重启窗口
            self.needs_window_restart = true;
        }

        if font_changed {
            tracing::info!(
                "字体设置已改变: font_name {} -> {}, font_path {} -> {}",
                if old_program_font_name.is_empty() {
                    "(空)"
                } else {
                    old_program_font_name
                },
                if new_program_font_name.is_empty() {
                    "(空)"
                } else {
                    &new_program_font_name
                },
                if old_program_font_path.is_empty() {
                    "(空)"
                } else {
                    old_program_font_path
                },
                if new_program_font_path.is_empty() {
                    "(空)"
                } else {
                    &new_program_font_path
                }
            );
            // 标记需要重启窗口以应用字体设置
            self.needs_window_restart = true;
        }

        // 保存配置
        self.storage.config.patch(|config| {
            config.ui.preferred_backend = new_preferred_backend;
            config.ui.soundfont_path = new_soundfont_path;
            config.ui.use_native_titlebar = new_use_native_titlebar;
            config.ui.program_font_name = new_program_font_name;
            config.ui.program_font_path = new_program_font_path;
        });

        if let Err(e) = self.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }

    /// 重启窗口（标题栏设置变更后）
    pub(crate) fn restart_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        tracing::info!("正在重启窗口以应用标题栏设置...");

        // 保存当前窗口状态
        let is_maximized = self.window.window().is_maximized();

        // 销毁当前窗口并创建新窗口
        let ui_state = self.storage.ui_state.get();
        let config = self.storage.config.get();

        // 创建新的窗口管理器
        match WindowManager::new(event_loop, ui_state, &config.ui) {
            Ok(new_window) => {
                // 替换窗口管理器
                self.window = new_window;

                // 恢复窗口最大化状态
                if is_maximized {
                    self.window.window().set_maximized(true);
                }

                tracing::info!("窗口重启完成");
            }
            Err(e) => {
                tracing::error!("重启窗口失败: {}", e);
            }
        }
    }
}
