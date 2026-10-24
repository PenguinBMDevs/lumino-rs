//! `NoteRenderer` 的 splat 入口：与主渲染器状态交互，保持在独立文件以
//! 控制单文件长度。

use super::{SPLAT_MIN_INSTANCES, SplatBuffers, SplatFrame};
use crate::note_renderer::NoteRenderer;
use crate::note_renderer::types::CameraUniform;

impl NoteRenderer {
    /// 准备 splat（仅横向洋葱皮、带 depth、实例数达阈值时）。
    ///
    /// 返回 `false` 时调用方回退点图元/quad 路径。
    pub(crate) fn prepare_splat(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: CameraUniform,
        physical: (u32, u32),
    ) -> bool {
        self.prepare_splat_inner(
            encoder,
            device,
            queue,
            camera,
            physical,
            SPLAT_MIN_INSTANCES,
        )
    }

    /// 测试用：跳过「实例数阈值」闸门（小场景也要走 splat 做像素等价）。
    #[cfg(test)]
    pub(crate) fn prepare_splat_for_test(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: CameraUniform,
        physical: (u32, u32),
    ) -> bool {
        self.prepare_splat_inner(encoder, device, queue, camera, physical, 0)
    }

    fn prepare_splat_inner(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: CameraUniform,
        physical: (u32, u32),
        min_instances: usize,
    ) -> bool {
        if (self.last_upload_count as usize) < min_instances {
            return false;
        }
        // packed = 区域位(31) | (0x7FFFFFFF - 源索引)；索引必须留出 1 个非零值
        if self.last_upload_count > 0x7FFF_FFFE {
            return false;
        }
        let Some(splat) = self.splat.as_mut() else {
            return false;
        };
        let logical = camera.viewport_size;
        if logical[0] <= 0.0 || logical[1] <= 0.0 {
            return false;
        }
        let scale_x = physical.0 as f32 / logical[0];
        let scale_y = physical.1 as f32 / logical[1];
        // 先写相机：compute pass 在当前 encoder 内执行时读到的是新相机
        queue.write_buffer(
            self.viewport_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );
        let source = self.gpu_note_buffer.buffer().clone();
        let buffers = SplatBuffers {
            source: &source,
            viewport: self.viewport_buffer.inner(),
            view_state: self.view_state_buffer.inner(),
            chunk_layout: &self.chunk_layout,
        };
        let frame = SplatFrame {
            physical,
            scale: (scale_x, scale_y),
            instance_count: self.last_upload_count as usize,
        };
        splat.prepare(encoder, device, queue, frame, buffers)
    }

    /// 绘制 splat 合成层；返回 `false` 时调用方回退点图元路径。
    pub(crate) fn draw_splat<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) -> bool {
        match self.splat.as_ref() {
            Some(splat) => splat.draw(pass, scissor_rect),
            None => false,
        }
    }
}
