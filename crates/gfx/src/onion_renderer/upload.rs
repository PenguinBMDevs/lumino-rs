use super::{OnionNote, OnionRenderer, OnionViewportUniform};

impl OnionRenderer {
    /// 上传洋葱皮音符到 GPU（带 dirty tracking + 内部颜色注入）
    ///
    /// 数据流：
    ///   1. 检查 list_version + color_hash，无变化 → 跳过（零分配）
    ///   2. 有变化 → 内部 colore-inject 音符并缓存到 `cpu_note_pool`
    ///   3. write_buffer 到 GPU storage buffer
    ///
    /// 颜色注入使用 per-note color_packed 字段，支持任意数量音轨。
    ///
    /// # 参数
    /// - `notes`: 原始洋葱皮音符（不含颜色，颜色由 track_colors 注入）
    /// - `list_version`: `OnionNoteList` 版本号
    /// - `track_colors`: per-track RGBA8 打包颜色表，index = track_idx
    /// - `color_hash`: 颜色表哈希值
    /// - `device`: wgpu Device
    /// - `queue`: wgpu Queue
    pub fn upload_notes(
        &mut self,
        notes: &[OnionNote],
        list_version: u64,
        track_colors: &[u32],
        color_hash: u64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        // ── Dirty tracking：数据未变且颜色未变 → 跳过（零分配、零 write_buffer） ──
        if list_version == self.last_list_version && color_hash == self.last_color_hash {
            return;
        }

        let count = notes.len();
        if count == 0 {
            self.cpu_note_pool.clear();
            self.note_count = 0;
            self.last_list_version = list_version;
            self.last_color_hash = color_hash;
            return;
        }

        // ── 颜色注入 + 缓存到 CPU pool ──
        self.cpu_note_pool.clear();
        self.cpu_note_pool.reserve(count);

        if track_colors.is_empty() {
            self.cpu_note_pool.extend_from_slice(notes);
        } else {
            for note in notes {
                let color = track_colors
                    .get(note.track_idx() as usize)
                    .copied()
                    .unwrap_or(0);
                let mut colored = *note;
                colored.set_color_packed(color);
                self.cpu_note_pool.push(colored);
            }
        }

        // 硬上限（物理边界兜底）
        let max_capacity = (self.max_storage_binding as usize / std::mem::size_of::<OnionNote>())
            .min(Self::MAX_NOTE_POOL_CAPACITY);

        // 按需扩容（只 grow，不 shrink）
        let required = count.max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity && self.note_pool_capacity < max_capacity {
            let new_capacity = if self.note_pool_capacity == Self::INITIAL_NOTE_CAPACITY {
                max_capacity
            } else {
                required.next_power_of_two().min(max_capacity)
            };
            if new_capacity > self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_capacity);
                self.note_pool_capacity = new_capacity;
                // buffer 重建了 → 重建 bind group
                self.render_bind_group = Self::create_render_bind_group(
                    device,
                    &self.render_bind_group_layout,
                    &self.viewport_buffer,
                    &self.note_pool_buffer,
                );
                tracing::info!(
                    "OnionRenderer: note pool grown to {} ({} MB)",
                    new_capacity,
                    (new_capacity * std::mem::size_of::<OnionNote>()) / (1024 * 1024)
                );
            }
        }

        let upload_count = count.min(self.note_pool_capacity);
        self.note_count = upload_count as u32;

        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_note_pool[..upload_count]),
        );

        self.last_list_version = list_version;
        self.last_color_hash = color_hash;
    }

    /// 准备视口 uniform
    ///
    /// 每帧在 draw 前调用，仅 128 字节 write_buffer，开销极小。
    pub fn prepare_viewport(&mut self, viewport: &OnionViewportUniform, queue: &wgpu::Queue) {
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[*viewport]));
    }

    /// 渲染洋葱皮
    ///
    /// 绘制所有上传的音符（GPU vertex shader 自动裁剪超视口部分）。
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        if self.note_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        // 4 vertices per note (TriangleStrip), draw all instances
        render_pass.draw(0..4, 0..self.note_count);
    }
}
