use super::*;

impl MiditrailRenderer {
    /// GPU-Driven 终局路径（Normal 视图）：`NoteInstance` 原字节直传 GPU。
    ///
    /// CPU 每帧只做：单次融合扫描（按键＋光晕系数）＋128 键构建＋两次上传
    /// （compact 音符原字节＋1KB 参数）＋提交。换算/实例构建/排序/gather
    /// 全部删除（位姿由 vertex shader 推导；顺序由 compact 承载——FILL 画家序 +
    /// 音符 `depth_write=false`，绘制顺序即最终次序，见 `bucket_cull.wgsl`
    /// `paint_order` 与 `miditrail_note_driven.wgsl` 头注）。
    /// Top 视图与实时预览仍走 legacy `render`/`render_from_instances`。
    pub fn render_gpu_driven(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        notes: &[NoteInstance],
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

        let t_scan = std::time::Instant::now();
        let (active_keys, aura_sizes) = compute_active_and_aura_for_compact(
            uniform.tick,
            uniform.ticks_per_second,
            uniform.fps,
            notes,
        );
        self.update_key_press_factors(&active_keys, uniform.fps);
        let scan_us = t_scan.elapsed().as_micros() as u64;

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

        let t_upload = std::time::Instant::now();
        self.ensure_compact_buffer(device, notes.len());
        if let Some(ref buf) = self.compact_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(notes));
        }
        let params = instances::build_driven_params(uniform, &self.key_positions, &self.key_widths);
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
        // 琴键实例复用 legacy 实例缓冲（本路径只存键，offset 恒为 0）。
        self.ensure_instance_buffer(device, key_instances.len());
        if let Some(ref buf) = self.instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&key_instances));
        }
        let upload_us = t_upload.elapsed().as_micros() as u64;

        let mut aura_instances = std::mem::take(&mut self.scratch_auras);
        aura_instances.clear();
        let t_aura = std::time::Instant::now();
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
        let aura_us = t_aura.elapsed().as_micros() as u64;

        self.ensure_aura_resources(device, queue);

        let t_submit = std::time::Instant::now();
        let camera = build_camera_uniform(width, height, uniform.view_mode, uniform.z_far_distance);
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );
        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }
        // 不变式：`ensure_compact_buffer` 已在上方执行，句柄必然存在。
        let compact_buf = match self.compact_buffer.as_ref() {
            Some(buf) => buf.inner().clone(),
            None => {
                debug_assert!(false, "compact_buffer 应已初始化");
                return;
            }
        };
        self.execute_driven_pass(
            encoder,
            notes.len(),
            &compact_buf,
            &key_instances,
            &aura_instances,
        );
        let submit_us = t_submit.elapsed().as_micros() as u64;
        Self::diag_driven(scan_us, upload_us, aura_us, submit_us, notes.len());
        self.scratch_keys = key_instances;
        self.scratch_auras = aura_instances;
    }

    /// 紧凑音符上传缓冲（按字节扩容，`NoteInstance` 16B/音符）。
    fn ensure_compact_buffer(&mut self, device: &wgpu::Device, count: usize) {
        if count <= self.compact_capacity {
            return;
        }
        let new_cap = count
            .next_power_of_two()
            .max(Self::INITIAL_INSTANCE_CAPACITY);
        let size = (new_cap * std::mem::size_of::<NoteInstance>()) as u64;
        let buffer = crate::gpu_resource_tracker::TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("miditrail_compact_buffer"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.compact_buffer = Some(buffer);
        self.compact_capacity = new_cap;
    }

    /// GPU-Driven 提交（两趟）：音符（driven 管线＋compact 缓冲）＋ Aura → 清深度 → 琴键。
    ///
    /// 与 legacy 单 pass 语义逐点对齐：
    /// - legacy 音符不写深度（画家排序），琴键最后绘制且 depth 缓冲里只有琴键自身
    ///   的写入——"琴键永远置顶"；driven 音符同样不写深度（顺序由 compact 画家序
    ///   承载，见 `bucket_cull.wgsl` `paint_order`）。
    /// - 若给琴键 compare=Always 绕过遮挡，琴键盒自遮挡反转：后画的面盖住先画的
    ///   顶面，键盘呈"开盖壳子"（2026-09-05 实验实锤）。
    /// - 深度清空分趟（防守语义，现音符不写深度时清空为恒等操作）：音符/光环一趟；
    ///   琴键一趟清空深度后按 LessEqual 绘制：琴键互遮挡正确，且无论音符深度策略
    ///   未来如何变化，"琴键永远置顶"与 legacy 逐位一致的语义都被锁死。
    pub(super) fn execute_driven_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        note_count: usize,
        compact: &wgpu::Buffer,
        key_instances: &[MiditrailInstanceGpu],
        aura_instances: &[MiditrailAuraInstanceGpu],
    ) {
        let Some(color_view) = self.output_texture_view.as_ref() else {
            debug_assert!(false, "output_texture_view 应已初始化");
            return;
        };
        let Some(depth_view) = self.depth_texture_view.as_ref() else {
            debug_assert!(false, "depth_texture_view 应已初始化");
            return;
        };
        let Some(bind_group) = self.bind_group.as_ref() else {
            debug_assert!(false, "bind_group 应已初始化");
            return;
        };
        let Some(driven_group) = self.driven_bind_group.as_ref() else {
            debug_assert!(false, "driven_bind_group 应已初始化");
            return;
        };
        let Some(instance_buf) = self.instance_buffer.as_ref() else {
            debug_assert!(false, "instance_buffer 应已初始化（琴键用）");
            return;
        };

        // ── 趟 1：音符（driven 管线，compact 实例直读，写深度）＋ Aura（加色，不写深度）──
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("miditrail_driven_notes_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.driven_note_pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_bind_group(1, driven_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.inner().slice(..));
            render_pass.set_vertex_buffer(1, compact.slice(..));
            // 平面模式（`3D音符` 开关关闭 = 默认）：只画顶面（X-Z），与 legacy 共用
            // 同一索引语义与 QUAD_RANGE；实例/顺序/管线/shader 全不动。
            if self.flat_notes {
                render_pass.set_index_buffer(
                    self.quad_index_buffer.inner().slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(Self::QUAD_RANGE, 0, 0..note_count as u32);
            } else {
                render_pass.set_index_buffer(
                    self.index_buffer.inner().slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(
                    0..Self::CUBE_INDICES.len() as u32,
                    0,
                    0..note_count as u32,
                );
            }

            self.draw_aura(&mut render_pass, aura_instances);
        }

        // ── 趟 2：琴键（深度清空后 LessEqual：自遮挡正确，且音符深度已丢弃）──
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("miditrail_driven_keys_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.inner().slice(..));
            render_pass.set_vertex_buffer(1, instance_buf.inner().slice(..));
            render_pass.set_index_buffer(
                self.index_buffer.inner().slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(
                0..Self::CUBE_INDICES.len() as u32,
                0,
                0..key_instances.len() as u32,
            );
        }
    }
}
