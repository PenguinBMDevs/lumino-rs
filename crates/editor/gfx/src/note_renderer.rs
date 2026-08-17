pub mod types;

use crate::gpu_resource_tracker;

pub use types::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, PREVIEW_BORDER_SENTINEL, RenderUniform,
    ViewState, calculate_border_width, pack_key_color, unpack_key_color,
};

// 子模块
mod buffer;
mod chunk;
mod draw;
mod events;
mod init;
mod pipeline;
mod prepare;

/// 音符渲染器 - 使用 wgpu 实例化渲染高效绘制大量音符
pub struct NoteRenderer {
    /// GPU 音符缓冲区
    gpu_note_buffer: crate::gpu_note_buffer::GpuNoteBuffer,
    /// 渲染管线
    pipeline: wgpu::RenderPipeline,
    /// 计算管线 (用于裁剪)
    cull_pipeline: wgpu::ComputePipeline,
    /// 可见实例缓冲区 (裁剪后)
    visible_instance_buffer: wgpu::Buffer,
    /// 间接绘制参数缓冲区
    indirect_buffer: wgpu::Buffer,
    /// 间接参数回读缓冲区（用于统计实际绘制的可见实例数）
    indirect_readback_buffer: wgpu::Buffer,
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
    /// 最大缓冲区容量（受 GPU max_storage_buffer_binding_size 限制）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用——不再用于截断/封顶，
    /// 保留字段用于诊断/统计（记录硬件限制信息）。
    #[allow(dead_code)]
    max_capacity: usize,
    /// 上次实际上传的实例数量（用于 prepare_pass 调度 compute）
    last_upload_count: u32,
    /// 视口 uniform 缓冲区
    viewport_buffer: wgpu::Buffer,
    /// 视图状态 uniform 缓冲区（当前音轨 + 静音位图，切轨/静音零重传）
    view_state_buffer: wgpu::Buffer,
    /// 裁剪 uniform 缓冲区（MAX_CHUNKS × slot_align 槽位，每 chunk 一条）
    cull_uniform_buffer: wgpu::Buffer,
    /// cull uniform 缓冲区总字节数（绑定 offset 越界断言用）
    cull_uniform_buffer_size: u64,
    /// 渲染 Bind group
    render_bind_group: wgpu::BindGroup,
    /// 计算 Bind groups（每 chunk 一个，storage binding 2GB 上限分块规避）
    cull_bind_groups: Vec<wgpu::BindGroup>,
    /// 计算 Bind group layout
    cull_bind_group_layout: wgpu::BindGroupLayout,
    /// storage binding 分块布局（跨硬件自适应）
    chunk_layout: chunk::ChunkLayout,
}

impl NoteRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = crate::constants::rendering::INITIAL_INSTANCE_CAPACITY;
    /// 顶点着色器代码 (WGSL)
    const VERTEX_SHADER: &'static str = include_str!("shaders/note.wgsl");
    /// 洋葱皮顶点着色器代码 (WGSL)
    const ONION_SHADER: &'static str = include_str!("shaders/onion_note.wgsl");
    /// 计算着色器代码 (WGSL)
    const CULL_SHADER: &'static str = include_str!("shaders/cull.wgsl");

    /// 获取上次上传的实例数量（用于诊断）
    pub fn last_upload_count(&self) -> u32 {
        self.last_upload_count
    }

    /// 更新视图状态（当前音轨 + 静音位图）
    ///
    /// 统一全量渲染（2026-08-06）：主音轨 = 洋葱皮 buffer 中 `current_track`
    /// 编码（track_idx+1）对应的段。切轨/静音变化只更新本 uniform，
    /// **GPU 音符数据零重传**（颜色/深度由 shader 动态判定）。
    ///
    /// `current_track` 为 track_idx+1 编码（0 = 无主音轨），
    /// `muted_tracks` 为静音音轨索引列表（shader 中静音轨仅主轨身份时显示）。
    pub fn set_view_state(&self, queue: &wgpu::Queue, current_track: u32, muted_tracks: &[usize]) {
        let mut state = crate::note_renderer::types::ViewState::new();
        state.current_track = current_track;
        for &track in muted_tracks {
            state.set_muted(track, true);
        }
        queue.write_buffer(
            &self.view_state_buffer,
            0,
            bytemuck::cast_slice(std::slice::from_ref(&state)),
        );
    }

    /// 调度一次回读，记录本渲染器本帧实际被 cull 后绘制的音符数量。
    ///
    /// 调用方应在 `queue.submit` 之后统一 `device.poll(wgpu::PollType::Wait)`，
    /// 以在回调中读到已完成的 GPU 数据。
    pub fn schedule_draw_count_log(&self, label: &str) {
        if self.last_upload_count == 0 {
            return;
        }
        let buffer_for_callback = self.indirect_readback_buffer.clone();
        let uploaded = self.last_upload_count;
        let label = label.to_string();
        self.indirect_readback_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_err() {
                    tracing::warn!("NoteRenderer [{}]: 回读失败，跳过绘制计数", label);
                    return;
                }
                let slice = buffer_for_callback.slice(..);
                let data = slice.get_mapped_range();
                let args =
                    bytemuck::cast_slice::<u8, crate::note_renderer::types::DrawIndirectArgs>(
                        &data,
                    );
                let visible: u32 = args.iter().map(|a| a.instance_count).sum();
                drop(data);
                buffer_for_callback.unmap();
                tracing::info!(
                    "NoteRenderer [{}]: uploaded={} visible_drawn={}",
                    label,
                    uploaded,
                    visible
                );
            });
    }
}

impl Drop for NoteRenderer {
    fn drop(&mut self) {
        // gpu_note_buffer 在其自身的 Drop 中释放 instance_buffer
        gpu_resource_tracker::sub_buffer(&self.visible_instance_buffer);
        gpu_resource_tracker::sub_buffer(&self.indirect_buffer);
        gpu_resource_tracker::sub_buffer(&self.indirect_readback_buffer);
        gpu_resource_tracker::sub_buffer(&self.viewport_buffer);
        gpu_resource_tracker::sub_buffer(&self.view_state_buffer);
        gpu_resource_tracker::sub_buffer(&self.cull_uniform_buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_renderer::types::DrawIndirectArgs;

    /// 测试 NoteInstance 创建和属性访问
    #[test]
    fn test_note_instance_creation() {
        // border_width = 4（wasabi 风格）
        let instance = NoteInstance::new(100.0, 60u8, 200.0, [1.0, 0.5, 0.0, 0.8], 4);

        assert_eq!(instance.start_length, [100.0, 200.0]);
        assert_eq!(instance.border_width, 4);
        // key_color 解包验证：低 8 位 = key, 高 24 位 = RGB
        let (key, unpacked) = crate::note_renderer::types::unpack_key_color(instance.key_color);
        assert_eq!(key, 60);
        assert!((unpacked[0] - 1.0).abs() < 0.01);
        assert!((unpacked[1] - 0.5).abs() < 0.01);
        assert!(unpacked[2].abs() < 0.01);
        // alpha 在 key_color 编码中不存（与 wasabi 一致），恒为 1.0
        assert!((unpacked[3] - 1.0).abs() < 0.01);
    }

    /// 测试 NoteInstance 字节大小 = 16（与 wasabi NoteVertex 一致）
    #[test]
    fn test_note_instance_size_16_bytes() {
        assert_eq!(std::mem::size_of::<NoteInstance>(), 16);
    }

    /// 测试预览音符哨兵值
    #[test]
    fn test_preview_sentinel() {
        let preview = NoteInstance::new_preview(0.0, 0u8, 100.0, [1.0; 4]);
        assert_eq!(preview.border_width, PREVIEW_BORDER_SENTINEL);
    }

    /// 测试 CameraUniform 默认值
    #[test]
    fn test_camera_uniform_default() {
        let camera = CameraUniform {
            scroll: [0.0, 0.0],
            zoom: [1.0, 20.0],
            viewport_size: [800.0, 600.0],
            canvas_offset: [0.0, 0.0],
            keyboard_width: 60.0,
            ruler_height: 30.0,
            max_key_index: 127.0,
            _padding: 0.0,
        };

        assert_eq!(camera.scroll, [0.0, 0.0]);
        assert_eq!(camera.zoom, [1.0, 20.0]);
        assert_eq!(camera.viewport_size, [800.0, 600.0]);
    }

    /// 测试 CullUniform 创建
    #[test]
    fn test_cull_uniform_creation() {
        let cull = CullUniform {
            instance_count: 1000,
            chunk_start: 0,
            chunk_count: 1000,
            _padding: 0,
        };

        assert_eq!(cull.instance_count, 1000);
        assert_eq!(cull.chunk_start, 0);
        assert_eq!(cull.chunk_count, 1000);
    }

    /// 测试常量配置
    #[test]
    fn test_constants() {
        use crate::constants::rendering;

        const { assert!(rendering::INITIAL_INSTANCE_CAPACITY >= 65536) };
        const { assert!(rendering::BUFFER_GROWTH_FACTOR == 2) };
    }

    /// 测试 DrawIndirectArgs 默认值
    #[test]
    fn test_draw_indirect_args_default() {
        let args = DrawIndirectArgs::default();

        assert_eq!(args.vertex_count, 4);
        assert_eq!(args.instance_count, 0);
        assert_eq!(args.first_vertex, 0);
        assert_eq!(args.first_instance, 0);
    }

    /// 测试 ViewState 与 WGSL 结构体内存布局一致：
    /// 4 bytes current_track + 12 bytes padding + 2048 u32 静音位图 = 8208 bytes。
    #[test]
    fn test_view_state_byte_layout() {
        assert_eq!(std::mem::size_of::<ViewState>(), 8208);
        assert_eq!(std::mem::offset_of!(ViewState, current_track), 0);
        assert_eq!(std::mem::offset_of!(ViewState, muted_bits), 16);
    }

    /// 测试 ViewState 静音位图与 shader 位索引一致（覆盖跨 vec4 边界）。
    #[test]
    fn test_view_state_muted_bit_layout_matches_shader() {
        let mut state = ViewState::new();
        // 设置若干边界轨道：0, 31, 32, 127, 128, 4095, 65535
        for &track in &[0usize, 31, 32, 127, 128, 4095, 65535] {
            state.set_muted(track, true);
            assert!(state.is_muted(track), "轨道 {track} 应被标记为静音");
        }
        // 未设置的轨道保持非静音
        assert!(!state.is_muted(1));
        assert!(!state.is_muted(30));
        assert!(!state.is_muted(33));
        assert!(!state.is_muted(126));
        assert!(!state.is_muted(129));
        assert!(!state.is_muted(4094));
        assert!(!state.is_muted(65534));
        // 越界轨道应被静默忽略（超过 65536）
        state.set_muted(65536, true);
        assert!(!state.is_muted(65536));
    }

    /// 测试 calculate_border_width 复刻 wasabi `utils::calculate_border_width`
    /// 公式：`((width_pixels / keys_len) / 12.0).clamp(1.0, 5.0).round() * 2.0`
    #[test]
    fn test_calculate_border_width_wasabi_parity() {
        use crate::calculate_border_width;
        // keys_len <= 0 → 0（防御）
        assert_eq!(calculate_border_width(800.0, 0.0), 0);
        assert_eq!(calculate_border_width(800.0, -1.0), 0);

        // 典型钢琴卷帘：800px / 88 键 → pixels_per_key ≈ 9.09 → /12 ≈ 0.76 → clamp 1.0 → *2 = 2
        assert_eq!(calculate_border_width(800.0, 88.0), 2);

        // zoom_y = 24 px/key（用户提到的 4 像素场景）：24/12 = 2.0 → round 2 → *2 = 4
        assert_eq!(calculate_border_width(24.0 * 12.0, 12.0), 4);

        // 极度放大：pixels_per_key = 66.7 → /12 ≈ 5.56 → clamp 5.0 → *2 = 10
        assert_eq!(calculate_border_width(800.0, 12.0), 10);

        // 边界：pixels_per_key/12 恰好 = 5.0 → clamp 不触发 → round 5 → *2 = 10
        assert_eq!(calculate_border_width(60.0 * 12.0, 12.0), 10);
    }
}
