//! 每帧渲染（上传数据 + dispatch compute shader）

use super::{WaterfallNoteGpu, WaterfallRenderer, WaterfallUniformGpu};

impl WaterfallRenderer {
    /// 渲染瀑布流帧。
    ///
    /// # 参数
    /// - `device` — wgpu 设备
    /// - `queue` — wgpu 队列
    /// - `encoder` — 命令编码器（compute pass 将追加到此 encoder）
    /// - `params` — 瀑布流 uniform 参数
    /// - `notes` — 音符数据切片（按 (key, start_tick) 升序排列）
    /// - `key_offsets` — 分桶偏移表（len = key_count + 1），桶 k 区间为
    ///   `[key_offsets[k], key_offsets[k+1])`。动态分桶：支持任意 key 数量。
    ///   为空时回退为单桶（全部音符），shader 仍可工作。
    /// - `active_key_colors` — 活跃键颜色数组（128 个 u32，packed RGBA `0xRRGGBBAA`，0 表示无高亮）
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        params: &WaterfallUniformGpu,
        notes: &[WaterfallNoteGpu],
        key_offsets: &[u32],
        active_key_colors: &[u32; 128],
    ) {
        let width = params.frame_width;
        let height = params.frame_height;
        if width == 0 || height == 0 {
            return;
        }

        // 确保资源已创建
        self.ensure_output_texture(device, width, height);
        self.ensure_note_buffer(device, notes.len());
        self.ensure_active_key_colors_buffer(device);
        self.ensure_key_offsets_buffer(device, key_offsets.len().saturating_sub(1));

        // 重建 bind group（如果资源发生了变化）
        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        // 上传 uniform
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[*params]),
        );

        // 上传音符数据
        if let Some(ref buf) = self.note_buffer {
            let note_bytes = bytemuck::cast_slice(notes);
            queue.write_buffer(buf.inner(), 0, note_bytes);
        }

        // 上传分桶偏移表（空时回退单桶：全部音符归入 key 0 桶。
        // 注意必须上传完整 key_count+1 长度，shader 会访问 key_offsets[key_count]）
        if let Some(ref buf) = self.key_offsets_buffer {
            if key_offsets.is_empty() {
                // 单桶回退：key 0 桶 = [0, len]，其余 key 桶为空
                let mut offsets = vec![notes.len() as u32; params.key_count as usize + 1];
                offsets[0] = 0;
                queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&offsets));
            } else {
                queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(key_offsets));
            }
        }

        // 上传活跃键颜色
        if let Some(ref buf) = self.active_key_colors_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(active_key_colors));
        }

        // 计算 dispatch 参数
        let workgroup_size: u32 = 16;
        let dispatch_x = width.div_ceil(workgroup_size);
        let dispatch_y = height.div_ceil(workgroup_size);

        // dispatch compute shader
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("waterfall_compute_pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.compute_pipeline);
            if let Some(ref bg) = self.bind_group {
                compute_pass.set_bind_group(0, bg, &[]);
            }
            compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
    }

    /// 获取输出纹理的引用（用于 export pipeline 读回）。
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref().map(|t| t.inner())
    }
}
