use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use winit::event_loop::ControlFlow;

use super::dialog_manager::DialogManager;
use super::file_handler::FileHandler;
use super::midi_handler::MidiHandler;
use super::midi_manager::MidiManager;
use super::progress_manager::ProgressManager;
use super::window_manager::WindowManager;
use crate::services::collaboration_service::CollaborationService;
use crate::services::file_service::FileService;
use crate::storage;

pub use lumino_midi_loader::ParsedMidi;

// ── 子模块 ──────────────────────────────────────────────────────────────

mod collab;
mod file;
mod midi;
mod persist;
mod session;
mod test;
mod window;

pub(crate) use collab::CollaborationStatus;
pub(crate) use session::SessionTracker;
pub(crate) use test::TestModeState;

// ── 错误类型 ────────────────────────────────────────────────────────────

/// Runner 初始化错误
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("存储初始化失败: {0}")]
    Storage(#[from] std::io::Error),
    #[error("窗口初始化失败: {0}")]
    Window(String),
}

// ── Runner 顶层结构 ─────────────────────────────────────────────────────

#[derive(Default)]
pub struct Runner {
    pub(crate) inner: Option<RunnerInner>,
    pub(crate) init_error: Option<InitError>,
    pub(crate) test_config: Option<crate::cli::TestConfig>,
    pub(crate) log_memory_usage: bool,
}

// ── 领域状态 ────────────────────────────────────────────────────────────

/// 窗口与 UI 状态
pub(crate) struct WindowState {
    pub(crate) window: WindowManager,
    pub(crate) storage: storage::Storage,
    pub(crate) needs_window_restart: bool,
    pub(crate) dialog_manager: DialogManager,
    pub(crate) progress: ProgressManager,
    pub(crate) progress_cb: lumino_midi_loader::loader::ProgressCallback,
    /// 视频导出进度接收器（(message, progress, total_frames, render_fps, elapsed_secs)）
    #[allow(clippy::type_complexity)]
    pub(crate) export_progress_rx:
        Option<tokio::sync::mpsc::UnboundedReceiver<(String, f64, u64, f64, f64)>>,
    /// 视频导出预览帧接收器（RGBA 像素数据）
    pub(crate) video_preview_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(Vec<u8>, u32, u32)>>,
    /// 视频导出取消标志（后台线程通过此标志检测用户取消）
    pub(crate) video_export_cancel: Arc<AtomicBool>,
}

/// MIDI 相关状态
pub(crate) struct MidiState {
    pub(crate) midi: MidiManager,
    pub(crate) current_midi: Option<Arc<ParsedMidi>>,
    pub(crate) current_midi_source: Option<std::path::PathBuf>,
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
    pub(crate) collaboration_service: CollaborationService,
    pub(crate) last_collab_sync: Option<std::time::Instant>,
}

/// 测试与调试状态
pub(crate) struct TestState {
    pub(crate) test_mode_state: Option<TestModeState>,
    pub(crate) log_memory_usage: bool,
    pub(crate) last_memory_log: Option<std::time::Instant>,
}

// ── RunnerInner：聚合所有子状态 ─────────────────────────────────────────

pub(crate) struct RunnerInner {
    pub(crate) window_state: WindowState,
    pub(crate) midi_state: MidiState,
    pub(crate) file_state: FileState,
    pub(crate) collab_state: CollabState,
    pub(crate) test_state: TestState,
    pub(crate) session_tracker: SessionTracker,
}

// ── impl Runner ─────────────────────────────────────────────────────────

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
        let progress_cb = lumino_midi_loader::loader::progress_from_sender(progress_tx);

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
        if let Err(e) = crate::platform::macos::init(config.ui.language) {
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
                export_progress_rx: None,
                video_preview_rx: None,
                video_export_cancel: Arc::new(AtomicBool::new(false)),
            },
            midi_state: MidiState {
                midi,
                current_midi: None,
                current_midi_source: None,
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
                last_collab_sync: None,
            },
            test_state: TestState {
                test_mode_state: None,
                log_memory_usage: self.log_memory_usage,
                last_memory_log: None,
            },
            session_tracker: SessionTracker::new(),
        };

        Ok(runner)
    }
}
