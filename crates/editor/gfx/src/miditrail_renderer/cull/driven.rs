use super::super::MiditrailRenderer;
use super::super::instances::{
    build_driven_params, build_key_instances, emit_aura_instances, update_key_positions,
};
use super::super::math::build_camera_uniform;
use super::super::types::MiditrailUniformGpu;
use super::*;

impl MiditrailRenderer {
    /// Normal driven 渲染（零音符回读）：compact 直绑顶点实例缓冲，绘制全窗口。
    ///
    /// CPU 每帧只做：活跃键解码 + 128 键实例/光晕构建 + 相机/参数上传；
    /// 音符换算/排序/gather/上传全部消失（vertex 推导 + 深度测试排序）。
    /// `active` 为 `cull_prepare` 回读的 1KB 聚合（布局见 `CullPrepared`）。
    /// 仅 Normal 视图使用；Top 视图走 `cull_window` + `render_from_instances`。
    #[allow(clippy::too_many_arguments)]
    pub fn render_gpu_driven_from_compact(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        compact: &wgpu::Buffer,
        note_count: usize,
        active: &[u32; KEY_BUCKETS],
    ) {
        let width = uniform.frame_width;
        let height = uniform.frame_height;
        if width == 0 || height == 0 {
            return;
        }
        self.ensure_output_texture(device, width, height);
        update_key_positions(
            uniform.key_count,
            &mut self.last_key_count,
            &mut self.key_positions,
            &mut self.key_widths,
        );

        let (active_keys, aura_sizes) = decode_active_for_gpu(active, uniform.key_count as usize);
        self.update_key_press_factors(&active_keys, uniform.fps);

        let mut key_instances = std::mem::take(&mut self.scratch_keys);
        key_instances.clear();
        build_key_instances(
            uniform,
            &active_keys,
            &self.key_positions,
            &self.key_widths,
            &self.key_press_factors,
            &mut key_instances,
        );
        self.ensure_instance_buffer(device, key_instances.len());
        if let Some(ref buf) = self.instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&key_instances));
        }

        let params = build_driven_params(uniform, &self.key_positions, &self.key_widths);
        queue.write_buffer(
            self.driven_params_buffer.inner(),
            0,
            bytemuck::cast_slice(&[params]),
        );
        if self.driven_bind_group.is_none() {
            self.driven_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("miditrail_driven_bind_group"),
                layout: &self.driven_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.driven_params_buffer.inner().as_entire_binding(),
                }],
            }));
        }

        let mut aura_instances = std::mem::take(&mut self.scratch_auras);
        aura_instances.clear();
        emit_aura_instances(
            &active_keys,
            &aura_sizes,
            uniform.key_count as usize,
            &self.key_positions,
            &self.key_widths,
            &mut aura_instances,
        );
        self.ensure_aura_instance_buffer(device, aura_instances.len());
        if let Some(ref buf) = self.aura_instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&aura_instances));
        }
        self.ensure_aura_resources(device, queue);

        let camera = build_camera_uniform(width, height, uniform.view_mode, uniform.z_far_distance);
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );
        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        self.execute_driven_pass(
            encoder,
            note_count,
            compact,
            &key_instances,
            &aura_instances,
        );

        self.scratch_keys = key_instances;
        self.scratch_auras = aura_instances;
    }
}
