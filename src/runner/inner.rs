use std::sync::Arc;
use std::sync::Mutex;
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
    #[error("云存储初始化失败: {0}")]
    Cloud(#[from] lumino_cloud::CloudError),
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
    /// 云传输（上传/下载）进度悬浮窗口（覆盖在云浏览对话框上）
    pub(crate) cloud_progress: super::cloud_progress::CloudProgressManager,
    /// 云传输进度发送端（后台线程推送阶段进度）
    pub(crate) cloud_progress_tx: tokio::sync::mpsc::UnboundedSender<(String, f64)>,
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
    /// 云端来源（从云面板下载的 .lmpj 工程：连接 id + 远程路径）。
    /// 覆盖保存后自动上传回原远程路径；打开新文件/关闭工程时清空。
    pub(crate) cloud_source: Option<CloudSource>,
}

/// 云端来源记录：定位"文件从哪里下载的"，供保存后自动回传
#[derive(Debug, Clone)]
pub(crate) struct CloudSource {
    /// 云连接 ID
    pub(crate) conn_id: String,
    /// 云端原始路径（含文件名）
    pub(crate) remote_path: String,
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
    /// 上一次实际发送的鼠标状态（含内容坐标、滚动、缩放），用于变更检测以抑制
    /// 无变化的重复发送（避免热路径日志洪泛与无谓带宽占用）
    pub(crate) last_sent_mouse: Option<LastSentMouse>,
}

/// 上一次发送的鼠标状态快照（用于变更检测）
#[derive(Debug, Clone, Copy)]
pub(crate) struct LastSentMouse {
    /// 内容空间坐标 X
    pub(crate) x: f32,
    /// 内容空间坐标 Y
    pub(crate) y: f32,
    /// 滚动偏移 X
    pub(crate) scroll_x: f32,
    /// 滚动偏移 Y
    pub(crate) scroll_y: f32,
    /// 缩放 X
    pub(crate) zoom_x: f32,
    /// 缩放 Y
    pub(crate) zoom_y: f32,
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
    /// 云存储管理器（后台线程锁内执行耗时操作）
    pub(crate) cloud: std::sync::Arc<std::sync::Mutex<lumino_cloud::CloudManager>>,
    /// 云入口意图（记录用户从哪里进入，连接成功后按意图打开对应面板）
    pub(crate) cloud_intent: Option<crate::runner::cloud::CloudIntent>,
    /// 断连提醒面板是否已弹出（每次会话只弹一次）
    pub(crate) cloud_alert_shown: bool,
    /// 找回删除音轨对话框的待填充条目列表
    ///
    /// 用户请求打开对话框时，Runner 先扫描缓存目录得到条目列表存于此字段，
    /// 在 `about_to_wait` 中检测对话框 UI 就绪后填充并清空。对话框分帧
    /// 初始化（窗口 → GFX → UI）导致打开请求与 UI 就绪之间有数帧延迟，
    /// 此字段作为缓冲。
    pub(crate) pending_recover_track_entries:
        Option<Vec<lumino_ui::event::window::track::RecoverTrackEntryPayload>>,
    /// 本地保存进行中标志（异步保存任务开始时置 true，结束置 false）。
    /// 保存期间禁止关闭软件，关闭请求转为 `pending_close` 延迟处理。
    pub(crate) saving: std::sync::Arc<AtomicBool>,
    /// 云端上传进行中标志（保存到云/自动回传期间置 true）
    pub(crate) cloud_saving: std::sync::Arc<AtomicBool>,
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

        // 在存储初始化后启动文件日志
        {
            let config_dir = storage::config_dir();
            let log_dir = config_dir.join("logs");
            let retention = storage.config.get().ui.log_retention_count;
            crate::logging::start_file_logging(log_dir, retention);
        }

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        // 保存进行中标志：WindowManager（关闭拦截）与 RunnerInner（保存任务）共享
        let saving = Arc::new(AtomicBool::new(false));
        let cloud_saving = Arc::new(AtomicBool::new(false));

        // 创建主窗口管理器
        let mut window = WindowManager::new(
            event_loop,
            ui_state,
            &config.ui,
            Arc::clone(&saving),
            Arc::clone(&cloud_saving),
        )
        .map_err(|e| InitError::Window(e.to_string()))?;

        // 创建进度管理器
        let (progress, progress_tx) = ProgressManager::new();
        let progress_cb = lumino_midi_loader::loader::progress_from_sender(progress_tx);

        // 创建云传输进度悬浮窗口管理器
        let (cloud_progress, cloud_progress_tx) =
            super::cloud_progress::CloudProgressManager::new();

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
        let file_service = FileService::new(Arc::clone(&progress_cb));
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
                cloud_progress,
                cloud_progress_tx,
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
                cloud_source: None,
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
                last_sent_mouse: None,
            },
            test_state: TestState {
                test_mode_state: None,
                log_memory_usage: self.log_memory_usage,
                last_memory_log: None,
            },
            session_tracker: SessionTracker::new(),
            pending_recover_track_entries: None,
            // 云存储管理器：配置与 config.json 同目录（cloud.json）
            // 配置解析失败已在 CloudConfigStore 内回退默认，此处失败仅剩
            // tokio Runtime 创建失败（内存不足等不可恢复错误）——传播为 InitError，
            // 由上层以启动失败而非运行时 panic 处理。
            cloud: Arc::new(Mutex::new(lumino_cloud::CloudManager::new(
                crate::storage::config_dir().join("cloud.json"),
            )?)),
            cloud_intent: None,
            cloud_alert_shown: false,
            saving,
            cloud_saving,
        };

        Ok(runner)
    }
}

impl RunnerInner {
    /// 本地保存是否进行中
    pub(crate) fn is_saving(&self) -> bool {
        self.saving.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 云端上传是否进行中
    pub(crate) fn is_cloud_saving(&self) -> bool {
        self.cloud_saving.load(std::sync::atomic::Ordering::SeqCst)
    }
}
