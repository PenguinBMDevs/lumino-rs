//! 呈现/HUD 桥接：只把 CPU 帧交给 `Presenter`，并画选择高亮。

use super::App;
use crate::{camera::Camera, edit_overlay, render::chrome};
use winit::event_loop::ActiveEventLoop;

impl App {
    /// 总览增量覆盖层：键盘/标尺只在相机变化时重画，帧率 HUD 每帧更新。
    pub(super) fn draw_overview_overlays(
        &mut self,
        camera: &Camera,
        width: usize,
        height: usize,
        hud: &str,
    ) {
        let keyboard_dirty = (camera.zoom_y - self.last_zoom_y).abs() > 1e-6
            || (camera.scroll_y - self.last_scroll_y).abs() > 0.01;
        let ruler_dirty = keyboard_dirty
            || (camera.zoom_x - self.last_zoom_x).abs() > 1e-6
            || (camera.scroll_x - self.last_scroll_x).abs() > 0.01;
        let frame = self.renderer.frame_mut();
        if frame.len() != width * height {
            return;
        }
        if keyboard_dirty {
            chrome::draw_keyboard(frame, width, camera, self.font.as_ref());
        }
        if ruler_dirty {
            chrome::draw_ruler(frame, width, camera, self.font.as_ref());
        }
        chrome::draw_status(frame, width, camera, hud, self.font.as_ref());
    }

    pub(super) fn hud_text(&self, mode: &str, visible: u64) -> String {
        format!(
            "{} | {} | 选中 {} | 可见 {} | {:.1} FPS | {:.2} ms",
            mode,
            self.editor.tool_name(),
            self.editor.selection_count(),
            visible,
            self.fps_ema,
            self.last_work_ms
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_edit_selection(&mut self, camera: &Camera, width: usize) {
        if self.editor.selection.is_empty() {
            return;
        }
        let frame = self.renderer.frame_mut();
        edit_overlay::draw_selection(
            frame,
            width,
            camera,
            &self.document,
            self.current_track,
            &self.editor.selection,
        );
    }

    pub(super) fn present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        self.presenter.present(&window, self.renderer.frame());
    }

    pub(super) fn present_canvas(&mut self, camera: &Camera) {
        let Some(window) = self.window.clone() else {
            return;
        };
        self.presenter
            .present_canvas(&window, self.renderer.frame(), camera);
    }

    pub(super) fn draw_overlays(
        &mut self,
        camera: &Camera,
        width: usize,
        height: usize,
        hud: &str,
    ) {
        let frame = self.renderer.frame_mut();
        if frame.len() != width * height {
            return;
        }
        chrome::draw_keyboard(frame, width, camera, self.font.as_ref());
        chrome::draw_ruler(frame, width, camera, self.font.as_ref());
        chrome::draw_status(frame, width, camera, hud, self.font.as_ref());
    }

    pub(super) fn finish_bench(&mut self, event_loop: &ActiveEventLoop) {
        if self.bench_reported {
            return;
        }
        self.bench_reported = true;
        let label = if self.overview {
            "overview-heatmap"
        } else {
            "scroll-strip"
        };
        let pass = self.stats.print_bench(label, self.overview);
        self.bench_pass = Some(pass);
        event_loop.exit();
    }
}
