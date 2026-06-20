//! 后台生成、纹理上传、生命周期管理

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;

use tracing::{debug, info};

use crate::generate::{TEXTURE_WIDTH, generate_pixels};
use crate::renderer::OnionSkinRenderer;
use crate::types::{GenerateProgress, GenerateResult, OnionSkinNote};
use lumino_core::view_state::DEFAULT_PPQ;

impl OnionSkinRenderer {
    /// 启动后台生成洋葱皮贴图
    pub fn generate(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        notes: Vec<Vec<OnionSkinNote>>,
        duration_ms: u32,
        tempo_table: Option<Vec<(u32, f32)>>,
    ) {
        self.cancel_previous_generation();

        self.duration_ms = duration_ms;
        self.ready.store(false, Ordering::SeqCst);

        let height = self.key_mode.height();
        let total_tracks = notes.len();
        let cancel_flag = Arc::clone(&self.cancel_flag);

        let (progress_tx, progress_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();

        self.progress_rx = Some(progress_rx);
        self.result_rx = Some(result_rx);

        let ppq = DEFAULT_PPQ as u32;

        let handle = thread::Builder::new()
            .name("onion-skin-generator".into())
            .spawn(move || {
                let result = generate_pixels(
                    &notes,
                    duration_ms,
                    height,
                    ppq,
                    tempo_table.as_deref(),
                    &progress_tx,
                    &cancel_flag,
                );

                if cancel_flag.load(Ordering::SeqCst) {
                    debug!("洋葱皮生成被取消");
                    return;
                }

                if let Some(result) = result {
                    let _ = result_tx.send(result);
                }
            })
            .expect("无法创建洋葱皮生成线程");

        self.generate_thread = Some(handle);

        info!(
            "洋葱皮生成已启动: {} 音轨, {} 毫秒, {}x{}",
            total_tracks, duration_ms, TEXTURE_WIDTH, height
        );
    }

    /// 取消之前的生成任务并等待线程结束
    fn cancel_previous_generation(&mut self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.generate_thread.take() {
            let _ = handle.join();
        }
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    /// 轮询生成进度
    pub fn poll_progress(&self) -> Option<GenerateProgress> {
        let mut latest = None;
        if let Some(ref rx) = self.progress_rx {
            while let Ok(progress) = rx.try_recv() {
                latest = Some(progress);
            }
        }
        latest
    }

    /// 检查生成是否完成且贴图已上传
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// 检查后台线程是否仍在运行
    pub fn is_generating(&self) -> bool {
        self.generate_thread.is_some()
            || self
                .progress_rx
                .as_ref()
                .is_some_and(|rx| rx.try_recv().is_ok())
    }

    /// 检查是否有可用的生成结果需要 upload
    ///
    /// 如果生成完成且有新结果，执行 upload 并返回 true。
    /// 应在每帧调用。
    pub fn check_and_upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let result = self.result_rx.as_ref().and_then(|rx| rx.try_recv().ok());

        if let Some(result) = result {
            self.upload_texture(device, queue, result);
            true
        } else {
            false
        }
    }

    /// 将后台生成的结果 upload 到 GPU 纹理
    fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        result: GenerateResult,
    ) {
        let width = TEXTURE_WIDTH;
        let height = result.height;

        self.texture = None;
        self.texture_view = None;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("onion_skin_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("onion_skin_texture_view"),
            ..Default::default()
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &result.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("onion_skin_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.texture = Some(texture);
        self.texture_view = Some(texture_view);
        self.ready.store(true, Ordering::SeqCst);

        info!("洋葱皮贴图已上传: {}x{}", width, height);
    }

    /// 释放所有 GPU 资源
    pub fn dispose(&mut self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.generate_thread.take() {
            let _ = handle.join();
        }
        self.cancel_flag.store(false, Ordering::SeqCst);

        self.progress_rx = None;
        self.result_rx = None;
        self.texture = None;
        self.texture_view = None;
        self.ready.store(false, Ordering::SeqCst);
        self.duration_ms = 0;

        debug!("洋葱皮渲染器已释放");
    }
}
