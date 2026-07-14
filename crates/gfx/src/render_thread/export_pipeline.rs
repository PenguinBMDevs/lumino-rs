//! GPU→CPU 帧读回导出管线
//!
//! 移植自 nezha 的 StagingRing + ExportPipeline。
//! 通过四重缓冲（4 槽 staging ring）实现 GPU 渲染与 CPU 读回的流水线并行：
//! 提交渲染后立即返回，非阻塞读取已完成帧。
//!
//! 核心流程：
//! 1. `copy_and_submit`：将离屏纹理 copy 到 staging buffer，提交编码器，启动异步 map
//! 2. `try_read` / `wait_read`：非阻塞 / 阻塞读取已就绪的帧数据（去 padding）

use std::ptr;
use std::sync::mpsc;

/// GPU→CPU 读回缓冲区
struct StagingBuffer {
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
    unpadded_bytes_per_row: u32,
    width: u32,
    height: u32,
}

struct StagingSlot {
    buffer: Option<StagingBuffer>,
    /// map_async 完成通知
    rx: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

/// 四重缓冲环，实现 GPU 渲染与 CPU 读回的流水线并行
struct StagingRing {
    slots: [StagingSlot; 4],
    next_write: usize,
    next_read: usize,
    inflight: usize,
    device: wgpu::Device,
}

impl StagingRing {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let make_slot = || StagingSlot {
            buffer: Some(Self::create_staging_buffer(device, width, height)),
            rx: None,
        };
        Self {
            slots: [make_slot(), make_slot(), make_slot(), make_slot()],
            next_write: 0,
            next_read: 0,
            inflight: 0,
            device: device.clone(),
        }
    }

    fn create_staging_buffer(device: &wgpu::Device, width: u32, height: u32) -> StagingBuffer {
        let bytes_per_pixel = 4u32; // BGRA
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as u64;

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_ring"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        StagingBuffer {
            buffer,
            padded_bytes_per_row,
            unpadded_bytes_per_row,
            width,
            height,
        }
    }

    fn ensure_size(&mut self, width: u32, height: u32) {
        let mut changed = false;
        for slot in &mut self.slots {
            if let Some(ref buf) = slot.buffer
                && (buf.width != width || buf.height != height)
            {
                slot.buffer = Some(Self::create_staging_buffer(&self.device, width, height));
                slot.rx = None;
                changed = true;
            }
        }
        if changed {
            self.next_write = 0;
            self.next_read = 0;
            self.inflight = 0;
        }
    }

    fn can_write(&self) -> bool {
        self.inflight < 4
    }

    #[allow(dead_code)]
    fn has_pending(&self) -> bool {
        self.inflight > 0
    }

    fn acquire_write_slot(&mut self) -> usize {
        debug_assert!(self.can_write());
        let idx = self.next_write;
        self.next_write = (self.next_write + 1) % 4;
        idx
    }

    fn write_slot_buffer(&self, slot_idx: usize) -> &StagingBuffer {
        self.slots[slot_idx]
            .buffer
            .as_ref()
            .expect("staging slot 应有 buffer")
    }

    fn map_after_submit(&mut self, slot_idx: usize) {
        let slot = &mut self.slots[slot_idx];
        let buf = slot.buffer.as_ref().expect("staging slot 应有 buffer");
        let slice = buf.buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        slot.rx = Some(rx);
        self.inflight += 1;
    }

    fn try_read(&mut self) -> Option<Vec<u8>> {
        if self.inflight == 0 {
            return None;
        }
        let _ = self.device.poll(wgpu::PollType::Poll);
        let slot = &self.slots[self.next_read];
        if let Some(ref rx) = slot.rx
            && rx.try_recv().is_ok()
        {
            return Some(self.finish_read());
        }
        None
    }

    fn wait_read(&mut self) -> Vec<u8> {
        if self.inflight == 0 {
            return Vec::new();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            {
                let slot = &self.slots[self.next_read];
                if let Some(ref rx) = slot.rx
                    && rx.try_recv().is_ok()
                {
                    return self.finish_read();
                }
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("staging ring wait_read 超时 5s");
                return Vec::new();
            }
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
        }
    }

    fn finish_read(&mut self) -> Vec<u8> {
        let slot = &mut self.slots[self.next_read];
        slot.rx = None;
        let buf = slot.buffer.as_ref().expect("staging slot 应有 buffer");

        let data = buf.buffer.slice(..).get_mapped_range();
        let total_unpadded = (buf.unpadded_bytes_per_row * buf.height) as usize;
        let mut result = Vec::with_capacity(total_unpadded);

        if buf.padded_bytes_per_row == buf.unpadded_bytes_per_row {
            // 无 padding，直接拷贝
            // Safety: result 容量足够，data 切片有效
            unsafe {
                ptr::copy_nonoverlapping(data.as_ptr(), result.as_mut_ptr(), total_unpadded);
                result.set_len(total_unpadded);
            }
        } else {
            // 逐行去 padding，使用 copy_nonoverlapping 避免 extend_from_slice 的边界检查
            // Safety: 每行数据在 data 范围内，result 容量足够
            let unpadded = buf.unpadded_bytes_per_row as usize;
            let padded = buf.padded_bytes_per_row as usize;
            unsafe {
                for row in 0..buf.height as usize {
                    let src = data.as_ptr().add(row * padded);
                    let dst = result.as_mut_ptr().add(row * unpadded);
                    ptr::copy_nonoverlapping(src, dst, unpadded);
                }
                result.set_len(total_unpadded);
            }
        }
        drop(data);
        buf.buffer.unmap();

        self.next_read = (self.next_read + 1) % 4;
        self.inflight -= 1;
        result
    }
}

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
}

impl ExportPipeline {
    /// 创建导出管线
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self {
            ring: StagingRing::new(device, width, height),
            cached_width: width,
            cached_height: height,
        }
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
                buffer: &staging.buffer,
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
        self.ring.try_read()
    }

    /// 阻塞等待最早提交的帧就绪（超时 5s 返回空 Vec）
    pub fn wait_read(&mut self) -> Vec<u8> {
        self.ring.wait_read()
    }
}
