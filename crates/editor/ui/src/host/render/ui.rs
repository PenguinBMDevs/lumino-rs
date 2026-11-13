use crate::host::Host;
use iced_core::Color;
use iced_wgpu::wgpu;

/// 主题色（egui 色板 `base_bg`）→ wgpu 清屏色（sRGB）。
fn clear_color(palette: lumino_ui_egui::theme::Palette) -> wgpu::Color {
    let base = palette.base_bg;
    let srgb = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    wgpu::Color {
        r: srgb(base.r()),
        g: srgb(base.g()),
        b: srgb(base.b()),
        a: 1.0,
    }
}

impl Host {
    /// 渲染窗口画面：egui 独占整窗（唯一生产路径）。
    ///
    /// iced widget 树**不再构建、不再 present**——外壳 / 对话框 / 面板 / 编辑器
    /// 骨架全部由 egui 绘制，画布底层由 egui 采样 gfx 离屏纹理。
    /// `background` 为 `Some(color)` 时以该色清屏（无编辑器画布的窗口模式），
    /// 为 `None` 时用当前主题的 `base_bg` 清屏。
    pub(super) fn render_iced_ui(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        texture_view: &wgpu::TextureView,
        background: Option<Color>,
    ) {
        puffin::profile_function!();

        let clear = match background {
            Some(color) => wgpu::Color {
                r: f64::from(color.r),
                g: f64::from(color.g),
                b: f64::from(color.b),
                a: 1.0,
            },
            None => clear_color(self.root.egui_palette()),
        };
        self.render_egui_overlay(frame, texture_view, Some(clear));
    }
}
