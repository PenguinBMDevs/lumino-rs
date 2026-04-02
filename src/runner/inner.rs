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

#[derive(Default)]
pub struct Runner {
    inner: Option<RunnerInner>,
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

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() {
            return;
        }

        let inner = self.init_inner(event_loop);
        self.inner = Some(inner);
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 首先检查是否是进度窗口
        if this.progress.is_progress_window(window_id) {
            this.progress.handle_event(event);
            return;
        }

        // 检查是否是对话框窗口
        if this.dialog_manager.is_dialog_window(window_id) {
            let mut dialog_result = None;
            let mut should_close = false;

            if let Some(dialog) = this.dialog_manager.get_dialog_mut(window_id) {
                dialog.handle_event(event);

                // 检查对话框是否应该关闭
                should_close = dialog.should_close();

                // 检查对话框结果
                if let Some(result) = dialog.check_result() {
                    dialog_result = Some(result);
                    should_close = true;
                }
            }

            // 如果应该关闭，关闭对话框
            if should_close {
                this.dialog_manager.close_dialog(window_id);
            } else {
                // 请求重绘对话框
                if let Some(dialog) = this.dialog_manager.get_dialog_mut(window_id) {
                    dialog.redraw();
                }
            }

            // 处理对话框返回的结果
            if let Some(result) = dialog_result {
                let main_ui = this.window.ui_mut();
                Self::apply_dialog_result_to_ui(main_ui, result);
            }
            return;
        }

        // 主窗口事件
        this.window.handle_event(event, &mut this.storage);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 处理进度消息
        let main_window = this.window.window().clone();
        let main_ui = this.window.ui_mut();
        this.progress.process_messages(main_ui, &main_window);

        // 更新进度窗口
        let ui_config = this.storage.config.get().ui.clone();
        this.progress.update(event_loop, &ui_config);

        // 处理窗口动作
        this.window.handle_window_actions(event_loop);

        // 处理音频动作
        Self::process_audio_actions(&mut this.window, &mut this.midi);

        // 处理核心事件（包括打开对话框）
        this.process_core_events(event_loop);

        // 初始化新创建的对话框（同步主窗口的协作状态）
        {
            let main_ui = this.window.ui();
            this.dialog_manager
                .initialize_pending_with_collaboration_state(
                    event_loop,
                    this.window.window(),
                    &this.storage.config.get().ui,
                    main_ui,
                );
        }

        // 更新对话框
        this.dialog_manager.update();

        // 保存存储
        this.save_storage();

        // 重新初始化 MIDI 如果需要
        if this.midi.needs_reinit() {
            let ui_config = this.storage.config.get().ui.clone();
            this.midi.reinit_if_needed(&ui_config);
        }

        // 检查 XSynth 异步初始化是否完成
        this.midi.check_async_init_complete();

        // 检查是否需要重启窗口（标题栏设置变更）
        if this.needs_window_restart {
            this.needs_window_restart = false;
            this.restart_window(event_loop);
        }

        // 检查播放状态：播放时使用 Poll 模式确保持续重绘，暂停时使用 Wait 模式节省资源
        let is_playing = this.window.ui().is_playing();
        if is_playing {
            event_loop.set_control_flow(ControlFlow::Poll);
            this.window.request_redraw();
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

impl Runner {
    fn init_inner(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> RunnerInner {
        let storage = match storage::Storage::new() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("初始化存储失败: {}", e);
                std::process::exit(1);
            }
        };

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        // 创建主窗口管理器
        let window = match WindowManager::new(event_loop, ui_state, &config.ui) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("初始化窗口管理器失败: {}", e);
                std::process::exit(1);
            }
        };

        // 创建进度管理器
        let (progress, progress_tx) = ProgressManager::new();
        lumino_core::midi::loader::set_progress_sender(progress_tx);

        // 创建 MIDI 管理器
        let midi = MidiManager::from_config(&config.ui);

        // 创建对话框管理器
        let dialog_manager = DialogManager::new();

        let collaboration_status = CollaborationStatus::Disconnected;

        // 创建新的处理器和服务
        let file_handler = FileHandler::new();
        let midi_handler = MidiHandler::new();
        let file_service = FileService::new();
        let collaboration_service = CollaborationService::new();

        event_loop.set_control_flow(ControlFlow::Wait);

        #[cfg(target_os = "macos")]
        crate::platform::macos::init();

        let runner = RunnerInner {
            window,
            storage,
            midi,
            progress,
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

        runner
    }

    fn process_audio_actions(window: &mut WindowManager, midi: &mut MidiManager) {
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

    fn apply_dialog_result_to_ui(ui: &mut lumino_ui::Host, result: DialogResult) {
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
}

impl RunnerInner {
    /// 保存存储
    fn save_storage(&mut self) {
        // 获取当前 UI 中的设置
        let new_preferred_backend = self.window.ui().settings().synth_backend;
        let new_soundfont_path = self.window.ui().settings().soundfont_path.clone();
        let new_use_native_titlebar = self.window.ui().settings().use_native_titlebar;

        // 获取当前存储的配置
        let old_config = self.storage.config.get();
        let old_preferred_backend = old_config.ui.preferred_backend;
        let old_soundfont_path = &old_config.ui.soundfont_path;
        let old_use_native_titlebar = old_config.ui.use_native_titlebar;

        // 检查合成器相关设置是否改变
        let backend_changed = new_preferred_backend != old_preferred_backend;
        let soundfont_changed = new_soundfont_path != *old_soundfont_path;
        let titlebar_changed = new_use_native_titlebar != old_use_native_titlebar;

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

        // 保存配置
        self.storage.config.patch(|config| {
            config.ui.preferred_backend = new_preferred_backend;
            config.ui.soundfont_path = new_soundfont_path;
            config.ui.use_native_titlebar = new_use_native_titlebar;
        });

        if let Err(e) = self.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }

    /// 重启窗口（标题栏设置变更后）
    fn restart_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
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
