use std::sync::Arc;

use tokio::sync::mpsc;
use winit::{dpi, event_loop::ControlFlow, keyboard::ModifiersState, window::WindowAttributes};

use super::storage;
use lumino_core::storage::config::{SynthBackend, UiConfig};

mod audio;
mod menu;
mod progress;
mod window;

pub use lumino_core::ParsedDms;
pub use lumino_core::ParsedMidi;

const MIN_WINDOW_WIDTH: u32 = 800;
const MIN_WINDOW_HEIGHT: u32 = 600;

#[derive(Default)]
pub struct Runner {
    inner: Option<RunnerInner>,
}

struct RunnerInner {
    gfx: lumino_gfx::Context,
    ui: lumino_ui::Host,
    storage: storage::Storage,
    window: Arc<winit::window::Window>,
    modifiers: ModifiersState,
    resized: bool,
    current_midi: Option<ParsedMidi>,
    current_dms: Option<Arc<ParsedDms>>,
    progress: Option<(String, f64)>,
    progress_rx: mpsc::UnboundedReceiver<(String, f64)>,
    progress_window: Option<Arc<winit::window::Window>>,
    progress_gfx: Option<lumino_gfx::Context>,
    progress_ui: Option<lumino_ui::Host>,
    progress_modifiers: ModifiersState,
    /// 保存 API 实例（用于保持 RealtimeSynth 等存活）
    midi_api: Option<Box<dyn lumino_midi::Api>>,
    midi_output: Option<Box<dyn lumino_midi::OutputConnection>>,
    /// 实际启用的合成器后端（可能与用户设置不同，如果发生回退）
    active_synth_backend: SynthBackend,
    /// MIDI 输出是否需要重新初始化（设置改变时）
    midi_needs_reinit: bool,
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
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        if this.is_progress_window(window_id) {
            this.handle_progress_window_event(event);
            return;
        }

        this.handle_main_window_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        this.process_progress_messages();
        this.update_progress_window(event_loop);
        this.handle_window_actions(event_loop);
        this.process_audio_actions();
        this.process_core_events(event_loop);
        this.save_storage();
        this.reinit_midi_if_needed();
    }
}

impl Runner {
    fn init_inner(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> RunnerInner {
        let storage = storage::Storage::new().expect("初始化存储失败");

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        let attributes = Self::build_window_attributes(ui_state);

        let window = Arc::new(event_loop.create_window(attributes).expect("创建窗口失败"));

        let physical_size = window.inner_size();

        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
        ))
        .expect("初始化图形上下文失败");

        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            &config.ui,
            &gfx,
            false,
        );

        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        lumino_core::midi::loader::set_progress_sender(progress_tx);

        event_loop.set_control_flow(ControlFlow::Wait);
        window.set_visible(true);

        #[cfg(target_os = "macos")]
        crate::platform::macos::init();

        // 初始化 MIDI 输出
        let (api, midi_output, active_backend) = Self::init_midi_output(&config.ui);

        RunnerInner {
            gfx,
            ui,
            storage,
            window,
            modifiers: ModifiersState::default(),
            resized: false,
            current_midi: None,
            current_dms: None,
            progress: None,
            progress_rx,
            progress_window: None,
            progress_gfx: None,
            progress_ui: None,
            progress_modifiers: ModifiersState::default(),
            midi_api: api,
            midi_output,
            active_synth_backend: active_backend.unwrap_or(SynthBackend::System),
            midi_needs_reinit: false,
        }
    }

    fn init_midi_output(
        ui_config: &UiConfig,
    ) -> (
        Option<Box<dyn lumino_midi::Api>>,
        Option<Box<dyn lumino_midi::OutputConnection>>,
        Option<SynthBackend>,
    ) {
        use lumino_midi::ApiKind;
        use std::path::PathBuf;

        // 优先级顺序：XSynth -> Kdmapi -> System
        let mut chosen_backend = None;

        // 1. 尝试 XSynth
        if let SynthBackend::XSynth = ui_config.preferred_backend {
            if !ui_config.soundfont_path.is_empty() {
                let path = PathBuf::from(&ui_config.soundfont_path);
                if path.exists() {
                    chosen_backend = Some((
                        ApiKind::XSynth {
                            soundfont_path: path,
                        },
                        SynthBackend::XSynth,
                    ));
                } else {
                    tracing::warn!("XSynth 音色库文件不存在: {:?}", path);
                }
            } else {
                tracing::warn!("XSynth 音色库路径未设置");
            }
        }

        // 2. 如果 XSynth 失败或原选择是 Kdmapi，尝试 Kdmapi
        if chosen_backend.is_none() {
            if let SynthBackend::Kdmapi = ui_config.preferred_backend {
                let kdmapi_path = PathBuf::from("C:\\Windows\\System32\\OmniMIDI\\OmniMIDI.dll");
                if kdmapi_path.exists() {
                    chosen_backend =
                        Some((ApiKind::Kdmapi { path: kdmapi_path }, SynthBackend::Kdmapi));
                } else {
                    tracing::warn!("未找到 OmniMIDI");
                }
            } else if let SynthBackend::XSynth = ui_config.preferred_backend {
                // XSynth 失败后尝试 Kdmapi（即使原选择是 XSynth）
                let kdmapi_path = PathBuf::from("C:\\Windows\\System32\\OmniMIDI\\OmniMIDI.dll");
                if kdmapi_path.exists() {
                    tracing::info!("XSynth 不可用，回退到 KDMAPI");
                    chosen_backend =
                        Some((ApiKind::Kdmapi { path: kdmapi_path }, SynthBackend::Kdmapi));
                }
            }
        }

        // 3. 如果都失败，使用 System
        let (api_kind, actual_backend) =
            chosen_backend.unwrap_or((ApiKind::System, SynthBackend::System));

        // 初始化 MIDI API
        let api = match lumino_midi::new_api(&api_kind) {
            Ok(api) => api,
            Err(e) => {
                tracing::warn!("初始化 MIDI API 失败: {:?}", e);
                return (None, None, Some(actual_backend));
            }
        };

        if let Some(version) = api.version() {
            tracing::info!("MIDI API 版本: {}", version);
        }

        // 获取第一个可用的输出设备
        let outputs = match api.outputs() {
            Ok(outputs) => outputs,
            Err(e) => {
                tracing::warn!("获取 MIDI 输出设备失败: {:?}", e);
                return (Some(api), None, Some(actual_backend));
            }
        };
        if let Some(output) = outputs.first() {
            tracing::info!("使用 MIDI 输出设备: {}", output.name);
            match api.open_output(output.id) {
                Ok(conn) => {
                    tracing::info!("MIDI 输出连接已打开");
                    (Some(api), Some(conn), Some(actual_backend))
                }
                Err(e) => {
                    tracing::warn!("打开 MIDI 输出连接失败: {:?}", e);
                    (Some(api), None, Some(actual_backend))
                }
            }
        } else {
            tracing::warn!("未找到可用的 MIDI 输出设备");
            (Some(api), None, Some(actual_backend))
        }
    }

    fn build_window_attributes(
        ui_state: &lumino_core::storage::ui_state::UiState,
    ) -> WindowAttributes {
        let mut attributes = WindowAttributes::default()
            .with_min_inner_size(dpi::LogicalSize {
                width: MIN_WINDOW_WIDTH,
                height: MIN_WINDOW_HEIGHT,
            })
            .with_inner_size(dpi::LogicalSize {
                width: ui_state.w,
                height: ui_state.h,
            })
            .with_maximized(ui_state.is_maximized)
            .with_title("Lumino")
            .with_visible(false);

        if let (Some(x), Some(y)) = (ui_state.x, ui_state.y)
            && !ui_state.is_maximized
        {
            attributes = attributes.with_position(dpi::LogicalPosition { x, y });
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes
                .with_decorations(false)
                .with_undecorated_shadow(true);
        }

        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attributes = attributes
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
        }

        attributes
    }
}
