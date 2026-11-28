//! 原生运行时状态：窗口、GPU、gfx 画布与帧间缓存。
//!
//! 字段对 `app` 子树可见（`pub(super)`），因为 `events` / `render` 是同一状态机的
//! 不同阶段；对 crate 外仍然不可见。

use std::sync::Arc;
use std::time::Instant;

use egui::{Event as EguiEvent, Pos2};
use winit::keyboard::ModifiersState;
use winit::window::Window;

/// 原生运行时应用状态。
#[derive(Default)]
pub struct NativeApp {
    pub(super) window: Option<Arc<Window>>,
    pub(super) gpu: Option<Gpu>,
    pub(super) ctx: egui::Context,
    pub(super) icons: lumino_ui_egui::icons::IconCache,
    pub(super) root: Option<lumino_ui::Root>,
    pub(super) canvas: Option<GfxCanvas>,
    pub(super) events: Vec<EguiEvent>,
    pub(super) pointer: Option<Pos2>,
    pub(super) modifiers: ModifiersState,
    pub(super) clicks: u32,
    /// 是否正在用 egui 指针驱动编辑器（按下 → Moved → Released）。
    pub(super) editor_dragging: bool,
    /// 最近一次同步到 gfx 渲染线程的音符世代（`track_notes_gen`）。
    pub(super) note_gen: Option<u64>,
    /// 上一帧 egui 占用区（菜单 / 弹层 / 面板）。
    ///
    /// 探针与生产同口径：事件到达时 egui 还没跑本帧，只能用上一帧的占用数据
    /// 判断这次点击是否该让位。`layer_id_at` 覆盖稳定态，占用表兜住 `Area`
    /// 首帧 sizing pass 的空窗——两者缺一就会「上层点击正常、下层也收到」。
    pub(super) blocked: Vec<egui::Rect>,
    pub(super) start: Option<Instant>,
}

pub(super) struct Gpu {
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) painter: lumino_egui_bridge::Painter,
}

/// 主音轨音符描边编码：低 16 位 = 描边像素，高 16 位 = `track_enc`（`track_id + 1`）。
///
/// cull shader 用 `track_enc` 区分「主音轨 / 洋葱皮可见区间」：只写描边宽度
/// （`track_enc = 0`）会让所有音符被裁掉，画布看起来「什么都没画」。
pub(super) fn note_border_width(track_id: usize) -> u32 {
    1 | (((track_id as u32) + 1) << 16)
}

/// 编辑器画布：`lumino-gfx` 分离渲染线程 + 离屏纹理（供 egui 作为底层采样）。
pub(super) struct GfxCanvas {
    thread: lumino_gfx::WgpuRenderThread,
}

impl GfxCanvas {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let (note_tx, note_rx) = std::sync::mpsc::channel();
        let thread = lumino_gfx::WgpuRenderThread::spawn(
            device.clone(),
            queue.clone(),
            format,
            lumino_gfx::GpuEpoch::INITIAL,
            note_tx,
            note_rx,
        )
        .map_err(|error| format!("启动 gfx 渲染线程失败: {error}"))?;
        Ok(Self { thread })
    }

    /// 全量上传所有轨道的音符实例（`OnionSkinStreamMsg` 会话）。
    pub(super) fn upload_notes(
        &self,
        tracks: Vec<(usize, Vec<lumino_gfx::NoteInstance>)>,
        track_count: usize,
        current_track: usize,
    ) {
        use lumino_gfx::OnionSkinStreamMsg as Msg;

        let total: usize = tracks.iter().map(|(_, instances)| instances.len()).sum();
        tracing::info!(
            "native note upload: tracks={track_count} current={current_track} notes={total}"
        );
        // 注意：`TrackLayout` **不能**夹在全量会话中（渲染线程会按“交错”跳过），
        // 且 `Done` 会立即结束本轮 drain——因此布局补发放在 `Done` 之后，
        // 由渲染线程下一轮 drain 应用（段表在 `Chunk` 流中已按轨构建）。
        let mut ok = self.thread.send_onion_skin_msg(Msg::BeginSession);
        ok &= self.thread.send_onion_skin_msg(Msg::Reserve { total });
        for (track_id, instances) in tracks {
            ok &= self.thread.send_onion_skin_msg(Msg::Chunk {
                track_id,
                instances,
            });
        }
        ok &= self.thread.send_onion_skin_msg(Msg::SetViewState {
            current_track: current_track as u32 + 1,
            muted_tracks: Vec::new(),
            onion_visible: false,
            onion_range: (0, 0),
        });
        ok &= self.thread.send_onion_skin_msg(Msg::Done);
        ok &= self
            .thread
            .send_onion_skin_msg(Msg::TrackLayout { track_count });
        if !ok {
            tracing::warn!("native note upload: 有消息未送达（会话可能残缺）");
        }
    }

    /// 提交一帧并等待离屏纹理就绪，返回可采样的纹理视图。
    pub(super) fn render(
        &self,
        params: lumino_gfx::RenderParams,
    ) -> Option<Arc<wgpu::TextureView>> {
        let frame_id = self.thread.send_params(params);
        self.thread.wait_for_frame(frame_id);
        self.thread.latest_texture_view()
    }
}

impl Gpu {
    pub(super) fn new(window: Arc<Window>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("创建 Surface 失败: {error}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("请求 GPU 适配器失败: {error:?}"))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| format!("创建 wgpu 设备失败: {error:?}"))?;

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "Surface 不支持默认配置".to_owned())?;
        surface.configure(&device, &config);
        let painter = lumino_egui_bridge::Painter::new(&device, config.format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            painter,
        })
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub(super) fn present(&mut self, ctx: &egui::Context, mut output: egui::FullOutput) {
        let Ok(frame) = self.surface.get_current_texture() else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lumino-runtime-encoder"),
            });

        // 清屏由 `Painter` 的 clear 参数完成（单 pass：clear + 图元）。
        let primitives =
            ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        self.painter
            .update_textures(&self.device, &self.queue, &mut output.textures_delta);
        self.painter.render(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            lumino_egui_bridge::Screen {
                size_px: [self.config.width, self.config.height],
                pixels_per_point: output.pixels_per_point,
                clear: Some(wgpu::Color {
                    r: 0.062,
                    g: 0.066,
                    b: 0.078,
                    a: 1.0,
                }),
            },
            &primitives,
        );

        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}
