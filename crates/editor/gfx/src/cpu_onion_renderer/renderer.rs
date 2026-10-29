//! CPU 洋葱皮叠加层渲染器：`prepare`（纹理 / uniform 上传）+ `draw`（四边形绘制）。

use crate::gpu_resource_tracker::TrackedBuffer;

use super::{
    frame::CpuOnionFrame,
    pipeline::{self, CpuOnionUniform},
    texture::CpuOnionTexture,
    upload::{UploadPlan, UploadState},
};

/// 单个 quad 的顶点数（2 个三角形，顶点由 `vertex_index` 生成）。
const QUAD_VERTICES: u32 = 6;

/// CPU 洋葱皮透明叠加层渲染器。
///
/// 生命周期与 [`crate::Renderers`] 一致；默认不参与任何绘制——只有
/// `RenderParams::cpu_onion_frame` 为 `Some` 时渲染线程才会调用
/// [`CpuOnionRenderer::prepare`] / [`CpuOnionRenderer::draw`]。
pub struct CpuOnionRenderer {
    /// 设备句柄：`prepare` 只拿得到 `queue`，尺寸变化重建纹理时仍需 device
    device: wgpu::Device,
    /// 叠加层渲染管线（alpha 混合 + 只读 depth-stencil 状态）
    pipeline: wgpu::RenderPipeline,
    /// bind group layout（uniform + 纹理 + 采样器）
    bind_group_layout: wgpu::BindGroupLayout,
    /// 最近邻采样器（1:1 贴合，不做插值）
    sampler: wgpu::Sampler,
    /// 物理视口 / 物理矩形 uniform
    uniform_buffer: TrackedBuffer,
    /// 上次写入 GPU 的 uniform 内容（相同则跳过写入）
    uniform: Option<CpuOnionUniform>,
    /// 驻留纹理（尺寸变化时重建）
    texture: Option<CpuOnionTexture>,
    /// 上传决策状态（尺寸 + 内容版本）
    upload: UploadState,
    /// 目标物理视口尺寸（由渲染线程在绘制前设置）
    viewport: (u32, u32),
}

impl CpuOnionRenderer {
    /// 创建渲染器：采样器 / bind group layout / 管线 / uniform 缓冲。
    ///
    /// `format` 为主渲染目标格式（sRGB 或线性均可；纹理本身恒为 sRGB）。
    /// 管线携带与 [`crate::constants::rendering::DEPTH_FORMAT`] 匹配的只读
    /// depth-stencil 状态，供带 depth attachment 的 UI 通道使用。
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cpu_onion_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group_layout = pipeline::create_bind_group_layout(device);
        let pipeline = pipeline::create_pipeline(device, &bind_group_layout, format);
        let uniform_buffer = pipeline::create_uniform_buffer(device);
        // 占位内容：全零 uniform（viewport = 0 时 draw 侧直接跳过，不会被采样）
        queue.write_buffer(
            uniform_buffer.inner(),
            0,
            bytemuck::bytes_of(&CpuOnionUniform::zeroed()),
        );

        Self {
            device: device.clone(),
            pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer,
            uniform: None,
            texture: None,
            upload: UploadState::default(),
            viewport: (0, 0),
        }
    }

    /// 设置目标物理视口尺寸（像素）。
    ///
    /// 渲染线程在绘制前按离屏纹理尺寸设置；`(0, 0)` 时 `draw` 直接跳过。
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport = (width, height);
    }

    /// 准备一帧：尺寸变化时重建纹理并全量上传，`version` 变化时全量上传。
    ///
    /// - 帧无效（尺寸为 0 / 像素长度不匹配）→ 不上传、不 panic，`draw` 亦跳过；
    /// - 尺寸与 `version` 都未变化 → 不上传（直接复用驻留纹理）。
    pub fn prepare(&mut self, queue: &wgpu::Queue, frame: &CpuOnionFrame) {
        let plan = self.upload.plan(frame);
        if matches!(plan, UploadPlan::Skip) {
            return;
        }

        match plan {
            UploadPlan::RecreateAndUpload => {
                let texture = CpuOnionTexture::create(
                    &self.device,
                    &self.bind_group_layout,
                    &self.sampler,
                    self.uniform_buffer.inner(),
                    frame.width,
                    frame.height,
                );
                texture.upload(queue, frame);
                self.texture = Some(texture);
                self.upload.record(frame);
            }
            UploadPlan::Upload => {
                let Some(texture) = self.texture.as_ref() else {
                    // 状态自相矛盾（记录有尺寸但资源缺失）：复位，等下一帧重建
                    self.upload.reset();
                    return;
                };
                texture.upload(queue, frame);
                self.upload.record(frame);
            }
            UploadPlan::Reuse | UploadPlan::Skip => {}
        }

        self.write_uniform(queue, frame);
    }

    /// 绘制叠加层（6 顶点四边形，最近邻采样，alpha 混合）。
    ///
    /// 以下情况直接跳过（不 panic、不产生 wgpu 校验错误）：
    /// 视口为 0、帧无效、驻留纹理缺失、纹理尺寸或内容版本与当前帧不一致
    ///（即 `prepare` 未成功执行）。
    pub fn draw(&self, render_pass: &mut wgpu::RenderPass, frame: &CpuOnionFrame) {
        if self.viewport.0 == 0 || self.viewport.1 == 0 {
            return;
        }
        if !frame.validate() {
            return;
        }
        let Some(texture) = self.texture.as_ref() else {
            return;
        };
        if texture.size() != frame.size() || self.uploaded_version() != Some(frame.version) {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, texture.bind_group(), &[]);
        render_pass.draw(0..QUAD_VERTICES, 0..1);
    }

    /// 已上传到 GPU 的内容版本（`None` 表示当前没有驻留纹理）。
    ///
    /// 供诊断与测试断言「相同 version 不重复上传」。
    #[must_use]
    pub(crate) fn uploaded_version(&self) -> Option<u64> {
        self.upload.version()
    }

    /// 写入 uniform（内容未变化时跳过）。
    fn write_uniform(&mut self, queue: &wgpu::Queue, frame: &CpuOnionFrame) {
        let uniform = CpuOnionUniform::new(self.viewport, frame);
        if self.uniform == Some(uniform) {
            return;
        }
        queue.write_buffer(self.uniform_buffer.inner(), 0, bytemuck::bytes_of(&uniform));
        self.uniform = Some(uniform);
    }
}
