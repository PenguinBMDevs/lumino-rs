use super::*;

impl MiditrailRenderer {
    /// 渲染一帧到内部离屏纹理。
    ///
    /// # 参数
    /// - `device` — wgpu 设备
    /// - `queue` — wgpu 队列
    /// - `encoder` — 命令编码器（render pass 将追加到此 encoder）
    /// - `uniform` — 渲染参数（tick、尺寸、速度、视图模式等）
    /// - `notes` — 可见音符数据切片
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        notes: &[MiditrailNoteGpu],
    ) {
        self.render_inner(device, queue, encoder, uniform, notes);
    }

    /// 直接消费统一 `NoteInstance` 的视频导出快捷路径（与 `render` 等价）。
    ///
    /// `NoteInstance` → `MiditrailNoteGpu` 换算写入跨帧复用的 `scratch_derived`，
    /// 消每帧 V×32B 整块分配（36 万可见时约 11.7MB/帧）。只读 key/start/end/color，
    /// 与旧 `note_instances_to_miditrail` 输出逐元素一致。
    pub fn render_from_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        note_instances: &[crate::NoteInstance],
    ) {
        // take/restore：fill 期间释放对 self 的借用，render_inner 才能拿 &mut self。
        let mut derived = std::mem::take(&mut self.scratch_derived);
        derived.clear();
        derived.reserve(note_instances.len());
        let t_convert = std::time::Instant::now();
        for n in note_instances {
            let (key, rgb) = crate::unpack_key_color(n.key_color);
            let start = n.start_length[0].max(0.0) as u32;
            let end = start.saturating_add(n.start_length[1].max(1.0) as u32);
            derived.push(MiditrailNoteGpu {
                key: key as u32,
                start_tick: start,
                end_tick: end,
                color_packed: pack_color([rgb[0], rgb[1], rgb[2], 1.0]),
                track_idx: 0,
                velocity: 100,
                channel: 0,
                _padding: 0,
            });
        }
        let convert_us = t_convert.elapsed().as_micros() as u64;
        self.render_inner(device, queue, encoder, uniform, &derived);
        Self::diag_convert(convert_us, derived.len());
        self.scratch_derived = derived;
    }

    fn render_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        notes: &[MiditrailNoteGpu],
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
        let t_active = std::time::Instant::now();
        let active_keys = compute_active_keys(uniform.tick, notes);
        self.update_key_press_factors(&active_keys, uniform.fps);
        let active_us = t_active.elapsed().as_micros() as u64;

        let is_top = uniform.view_mode.is_top();
        // Top 先做逐音时间量化对齐（永不合并；Normal 路径零改动，防污染）。
        let top_notes;
        let notes: &[MiditrailNoteGpu] = if is_top {
            top_notes = quantize_notes_for_top(uniform, notes);
            &top_notes
        } else {
            notes
        };

        let mut note_instances = std::mem::take(&mut self.scratch_notes);
        note_instances.clear();
        let t_build_notes = std::time::Instant::now();
        build_note_instances(
            uniform,
            notes,
            &self.key_positions,
            &self.key_widths,
            &mut note_instances,
            &mut self.scratch_build,
        );
        let build_notes_us = t_build_notes.elapsed().as_micros() as u64;
        let mut key_instances = std::mem::take(&mut self.scratch_keys);
        key_instances.clear();
        // Top 键盘不要按下位移（俯视下位移丑且无意义），只保留颜色反馈：
        // 传全零 press 数组，`update_key_press_factors` 照常更新内部状态，
        // 切回 Normal 时按压动画无缝衔接。
        let press_factors: &[f32] = if is_top {
            &ZERO_PRESS_FACTORS
        } else {
            &self.key_press_factors
        };
        let t_build_keys = std::time::Instant::now();
        build_key_instances(
            uniform,
            &active_keys,
            &self.key_positions,
            &self.key_widths,
            press_factors,
            &mut key_instances,
        );
        let build_keys_us = t_build_keys.elapsed().as_micros() as u64;

        let total_instances = note_instances.len() + key_instances.len();
        self.ensure_instance_buffer(device, total_instances);
        let t_upload = std::time::Instant::now();
        let note_bytes =
            (note_instances.len() * std::mem::size_of::<MiditrailInstanceGpu>()) as u64;
        if let Some(ref buf) = self.instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&note_instances));
            if !key_instances.is_empty() {
                queue.write_buffer(
                    buf.inner(),
                    note_bytes,
                    bytemuck::cast_slice(&key_instances),
                );
            }
        }
        let upload_notes_us = t_upload.elapsed().as_micros() as u64;

        let mut aura_instances = std::mem::take(&mut self.scratch_auras);
        aura_instances.clear();
        let t_aura = std::time::Instant::now();
        if !is_top {
            // Aura 四边形在俯视下与视线垂直（零面积）天然不可见，
            // Top 直接跳过实例构建与绘制（CPU + GPU 双省）。
            build_aura_instances(
                uniform,
                notes,
                &active_keys,
                &self.key_positions,
                &self.key_widths,
                &mut aura_instances,
            );
            self.ensure_aura_instance_buffer(device, aura_instances.len());
            if let Some(ref buf) = self.aura_instance_buffer {
                queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&aura_instances));
            }
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

        self.execute_render_pass(
            encoder,
            &note_instances,
            &key_instances,
            &aura_instances,
            is_top,
        );
        let submit_us = t_submit.elapsed().as_micros() as u64;
        Self::diag_stages(
            active_us,
            build_notes_us,
            build_keys_us,
            upload_notes_us,
            aura_us,
            submit_us,
            notes.len(),
        );
        // 暂存 Vec 归还（保留容量，下一帧零分配复用）。
        self.scratch_notes = note_instances;
        self.scratch_keys = key_instances;
        self.scratch_auras = aura_instances;
    }
}
