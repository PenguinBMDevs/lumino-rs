use std::sync::Arc;
use winit::event_loop::ControlFlow;

use super::storage;

mod menu;
mod midi_manager;
mod progress_manager;
mod window_manager;

pub use lumino_core::ParsedDms;
pub use lumino_core::ParsedMidi;

use midi_manager::{MidiManager, handle_audio_action};
use progress_manager::ProgressManager;
use window_manager::WindowManager;

#[derive(Default)]
pub struct Runner {
    inner: Option<RunnerInner>,
}

struct RunnerInner {
    /// 主窗口管理器
    window: WindowManager,
    /// 存储
    storage: storage::Storage,
    /// MIDI 管理器
    midi: MidiManager,
    /// 进度管理器
    progress: ProgressManager,
    /// 当前加载的 MIDI
    current_midi: Option<ParsedMidi>,
    /// 当前加载的 DMS
    current_dms: Option<Arc<ParsedDms>>,
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

        if this.progress.is_progress_window(window_id) {
            this.progress.handle_event(event);
            return;
        }

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

        // 处理核心事件
        this.process_core_events(event_loop);

        // 保存存储
        this.save_storage();

        // 重新初始化 MIDI 如果需要
        if this.midi.needs_reinit() {
            let ui_config = this.storage.config.get().ui.clone();
            this.midi.reinit_if_needed(&ui_config);
        }

        // 检查 XSynth 异步初始化是否完成
        this.midi.check_async_init_complete();
    }
}

impl Runner {
    fn init_inner(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> RunnerInner {
        let storage = storage::Storage::new().expect("初始化存储失败");

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        // 创建主窗口管理器
        let window =
            WindowManager::new(event_loop, ui_state, &config.ui).expect("初始化窗口管理器失败");

        // 创建进度管理器
        let (progress, progress_tx) = ProgressManager::new();
        lumino_core::midi::loader::set_progress_sender(progress_tx);

        // 创建 MIDI 管理器
        let midi = MidiManager::from_config(&config.ui);

        event_loop.set_control_flow(ControlFlow::Wait);

        #[cfg(target_os = "macos")]
        crate::platform::macos::init();

        RunnerInner {
            window,
            storage,
            midi,
            progress,
            current_midi: None,
            current_dms: None,
        }
    }

    fn process_audio_actions(window: &mut WindowManager, midi: &mut MidiManager) {
        let actions = window.ui_mut().take_audio_actions();

        if !actions.is_empty() {
            tracing::info!("Runner: 处理 {} 个音频动作", actions.len());
        }

        for action in actions {
            if let Some(output) = midi.output_mut() {
                handle_audio_action(output, action);
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

        // 获取当前存储的配置
        let old_config = self.storage.config.get();
        let old_preferred_backend = old_config.ui.preferred_backend;
        let old_soundfont_path = &old_config.ui.soundfont_path;

        // 检查合成器相关设置是否改变
        let backend_changed = new_preferred_backend != old_preferred_backend;
        let soundfont_changed = new_soundfont_path != *old_soundfont_path;

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

        // 保存配置
        self.storage.config.patch(|config| {
            config.ui.preferred_backend = new_preferred_backend;
            config.ui.soundfont_path = new_soundfont_path;
        });

        if let Err(e) = self.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }
}
