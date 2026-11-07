//! 实验性 egui 叠加层（`LUMINO_EGUI_OVERLAY=1` 时启用）。
//!
//! 目的：在真实窗口 surface / 真实 `Device` 上验证 `lumino-egui-bridge`，
//! 为 Task 1.2 的生产接线提供可开关的最小闭环。默认关闭，不改变现有画面。
//!
//! 实测（Windows / Vulkan，2560×1540 @1.25）：surface 格式为
//! `Bgra8UnormSrgb`，叠加层在 iced `present` 之后绘制，halt 时不请求重绘。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};
use iced_wgpu::wgpu;
use lumino_egui_bridge::{Painter, Screen};

use crate::host::Host;

/// 叠加层状态（首次启用时创建，跨帧保留纹理与管线）。
pub(crate) struct EguiOverlay {
    /// egui 上下文（默认字体即可，HUD 只用 ASCII）。
    ctx: egui::Context,
    /// wgpu27 绘制桥。
    painter: Painter,
    /// 叠加层已绘制帧数。
    frames: u32,
}

/// 一帧叠加层绘制输入。
struct OverlayFrame<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size_px: [u32; 2],
    pixels_per_point: f32,
    app_frame: u32,
}

impl EguiOverlay {
    /// 用当前 surface 格式创建叠加层。
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        tracing::info!("egui 叠加层已启用：目标格式 {format:?}");
        Self {
            ctx: egui::Context::default(),
            painter: Painter::new(device, format),
            frames: 0,
        }
    }

    /// 渲染一帧 HUD 并提交到目标纹理。
    fn draw(&mut self, frame: OverlayFrame<'_>) {
        self.frames = self.frames.wrapping_add(1);

        let logical = [
            frame.size_px[0] as f32 / frame.pixels_per_point,
            frame.size_px[1] as f32 / frame.pixels_per_point,
        ];
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(logical[0], logical[1]),
            )),
            ..Default::default()
        };

        let overlay_frames = self.frames;
        let output = self.ctx.run_ui(input, |ui| {
            hud(
                ui,
                frame.format,
                frame.size_px,
                overlay_frames,
                frame.app_frame,
            );
        });

        let mut textures = output.textures_delta;
        self.painter
            .update_textures(frame.device, frame.queue, &mut textures);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        let mut encoder = frame
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lumino_egui_overlay_encoder"),
            });
        let drawn = self.painter.render(
            frame.device,
            frame.queue,
            &mut encoder,
            frame.view,
            Screen {
                size_px: frame.size_px,
                pixels_per_point: frame.pixels_per_point,
            },
            &primitives,
        );
        frame.queue.submit(Some(encoder.finish()));
        if drawn == 0 {
            tracing::debug!("egui 叠加层本帧没有可绘制图元");
        }
    }
}

impl Host {
    /// 在 iced `present` 之后叠加 egui 层（默认关闭）。
    pub(super) fn render_egui_overlay(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
    ) {
        if !overlay_enabled() {
            return;
        }

        let format = frame.texture.format();
        let size = self.render_ctx.viewport.physical_size();
        let pixels_per_point = self.render_ctx.viewport.scale_factor();
        let app_frame = self.frame_count;
        // Device/Queue 是 Arc 句柄，克隆避免与 `egui_overlay` 的可变借用冲突。
        let device = self.render_ctx.device.clone();
        let queue = self.render_ctx.queue.clone();

        let overlay = self
            .render_ctx
            .egui_overlay
            .get_or_insert_with(|| EguiOverlay::new(&device, format));
        overlay.draw(OverlayFrame {
            device: &device,
            queue: &queue,
            view,
            format,
            size_px: [size.width, size.height],
            pixels_per_point,
            app_frame,
        });
    }
}

/// 环境变量开关（只读一次；`0` 视为关闭）。
fn overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("LUMINO_EGUI_OVERLAY").is_some_and(|value| value != "0"))
}

/// 右上角 HUD：不依赖主题，直接验证矩形、文字、透明度与 sRGB 混合。
fn hud(
    ui: &mut egui::Ui,
    format: wgpu::TextureFormat,
    size_px: [u32; 2],
    overlay_frames: u32,
    app_frame: u32,
) {
    let screen = ui.ctx().viewport_rect();
    let rect = Rect::from_min_size(
        Pos2::new(screen.right() - 248.0, screen.top() + 44.0),
        Vec2::new(228.0, 96.0),
    );
    let accent = Color32::from_rgb(0x2A, 0xC3, 0xDE);
    let text = Color32::from_rgb(0xB6, 0xC4, 0xFA);
    let painter = ui.painter();

    painter.rect(
        rect,
        CornerRadius::same(8),
        Color32::from_black_alpha(210),
        Stroke::new(1.0, accent),
        StrokeKind::Inside,
    );
    painter.text(
        Pos2::new(rect.left() + 14.0, rect.top() + 14.0),
        Align2::LEFT_TOP,
        "egui overlay · wgpu27",
        FontId::proportional(14.0),
        Color32::WHITE,
    );
    painter.text(
        Pos2::new(rect.left() + 14.0, rect.top() + 40.0),
        Align2::LEFT_TOP,
        format!("{format:?}"),
        FontId::proportional(16.0),
        accent,
    );
    painter.text(
        Pos2::new(rect.left() + 14.0, rect.top() + 68.0),
        Align2::LEFT_TOP,
        format!(
            "{}×{} · overlay {} · app {}",
            size_px[0], size_px[1], overlay_frames, app_frame
        ),
        FontId::monospace(10.0),
        text,
    );
}
