//! GPU→CPU 帧读回导出管线
//!
//! 移植自 nezha 的 StagingRing + ExportPipeline。
//! 通过四重缓冲（4 槽 staging ring）实现 GPU 渲染与 CPU 读回的流水线并行：
//! 提交渲染后立即返回，非阻塞读取已完成帧。
//!
//! 核心流程：
//! 1. `copy_and_submit`：将离屏纹理 copy 到 staging buffer，提交编码器，启动异步 map
//! 2. `try_read` / `wait_read`：非阻塞 / 阻塞读取已就绪的帧数据（去 padding）

use std::sync::mpsc;

mod staging;

use staging::StagingRing;

/// GPU 帧读回导出管线
///
/// 通过四重缓冲（4 槽 staging ring）实现渲染与 CPU 读回的流水线并行。
/// 视频导出时，渲染线程将离屏纹理拷贝到 staging buffer 并异步映射，
/// 然后通过 `try_read` / `wait_read` 读取 BGRA 像素数据。
pub struct ExportPipeline {
    ring: StagingRing,
    /// 上次 ensure_size 的尺寸，尺寸不变时跳过迭代检查
    cached_width: u32,
    cached_height: u32,
    /// 帧缓冲区回收通道：ffmpeg 写入线程归还已用 Vec<`u8`>
    recycle_rx: Option<mpsc::Receiver<Vec<u8>>>,
}

impl ExportPipeline {
    /// 创建导出管线
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self {
            ring: StagingRing::new(device, width, height),
            cached_width: width,
            cached_height: height,
            recycle_rx: None,
        }
    }

    /// 设置帧缓冲区回收通道
    pub fn set_recycle_receiver(&mut self, rx: mpsc::Receiver<Vec<u8>>) {
        self.recycle_rx = Some(rx);
    }

    /// 当输出尺寸变化时重建 staging buffer
    ///
    /// 视频导出期间尺寸通常不变，缓存尺寸避免每帧迭代 4 个槽位。
    pub fn ensure_size(&mut self, width: u32, height: u32) {
        if width == self.cached_width && height == self.cached_height {
            return;
        }
        self.cached_width = width;
        self.cached_height = height;
        self.ring.ensure_size(width, height);
    }

    /// 四重缓冲是否还有空闲槽位
    pub fn can_write(&self) -> bool {
        self.ring.can_write()
    }

    /// 将离屏纹理拷贝到 staging ring、提交编码器、启动异步 GPU 读回映射
    ///
    /// 所有权型编码器：此方法消费 `encoder`（追加 copy 命令后 finish + submit）
    pub fn copy_and_submit(
        &mut self,
        mut encoder: wgpu::CommandEncoder,
        source: &wgpu::Texture,
        queue: &wgpu::Queue,
    ) {
        let slot_idx = self.ring.acquire_write_slot();
        let staging = self.ring.write_slot_buffer(slot_idx);

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: staging.buffer.inner(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(staging.padded_bytes_per_row),
                    rows_per_image: Some(staging.height),
                },
            },
            wgpu::Extent3d {
                width: staging.width,
                height: staging.height,
                depth_or_array_layers: 1,
            },
        );

        let cmd_buf = encoder.finish();
        queue.submit(std::iter::once(cmd_buf));
        self.ring.map_after_submit(slot_idx);
    }

    /// 非阻塞尝试读回最早提交的帧
    pub fn try_read(&mut self) -> Option<Vec<u8>> {
        if let Some(ref rx) = self.recycle_rx {
            self.ring.try_recycle(rx);
        }
        self.ring.try_read()
    }

    /// 阻塞等待最早提交的帧就绪（超时 5s 返回空 Vec）
    pub fn wait_read(&mut self) -> Vec<u8> {
        if let Some(ref rx) = self.recycle_rx {
            self.ring.try_recycle(rx);
        }
        self.ring.wait_read()
    }
}
