use std::sync::Arc;

use tokio::sync::mpsc;
use winit::{dpi, event_loop::ControlFlow, keyboard::ModifiersState, window::WindowAttributes};

use super::storage;

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
        this.process_core_events(event_loop);
        this.save_storage();
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
