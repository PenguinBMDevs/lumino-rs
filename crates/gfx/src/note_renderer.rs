pub mod types;

use crate::gpu_resource_tracker;

pub use types::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, OnionBgTileRef,
    PREVIEW_BORDER_SENTINEL, RenderUniform, calculate_border_width, pack_key_color,
    unpack_key_color,
};

// 子模块
mod buffer;
mod draw;
mod events;
mod init;
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
    /// 当前缓冲区容量（实例数量）
    capacity: usize,
    /// 最大缓冲区容量（受 GPU max_storage_buffer_binding_size 限制）
    max_capacity: usize,
    /// 上次实际上传的实例数量（用于 prepare_pass 调度 compute）
    last_upload_count: u32,
    /// 视口 uniform 缓冲区
    viewport_buffer: wgpu::Buffer,
    /// 裁剪 uniform 缓冲区
    cull_uniform_buffer: wgpu::Buffer,
    /// 渲染 Bind group
    render_bind_group: wgpu::BindGroup,
    /// 计算 Bind group
    cull_bind_group: wgpu::BindGroup,
    /// 计算 Bind group layout
    cull_bind_group_layout: wgpu::BindGroupLayout,
}

impl NoteRenderer {
    /// 初始缓冲区容量
    const INITIAL_CAPACITY: usize = crate::constants::rendering::INITIAL_INSTANCE_CAPACITY;
    /// 顶点着色器代码 (WGSL)
    const VERTEX_SHADER: &'static str = include_str!("shaders/note.wgsl");
    /// 计算着色器代码 (WGSL)
    const CULL_SHADER: &'static str = include_str!("shaders/cull.wgsl");

    /// 获取上次上传的实例数量（用于诊断）
    pub fn last_upload_count(&self) -> u32 {
        self.last_upload_count
    }
}

impl Drop for NoteRenderer {
    fn drop(&mut self) {
        // gpu_note_buffer 在其自身的 Drop 中释放 instance_buffer
        gpu_resource_tracker::sub_buffer(&self.visible_instance_buffer);
        gpu_resource_tracker::sub_buffer(&self.indirect_buffer);
        gpu_resource_tracker::sub_buffer(&self.viewport_buffer);
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
        let instance = NoteInstance::new(100.0, 60.0, 200.0, [1.0, 0.5, 0.0, 0.8], 4);

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
        let preview = NoteInstance::new_preview(0.0, 0.0, 100.0, [1.0; 4]);
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
            _padding: [0; 3],
        };

        assert_eq!(cull.instance_count, 1000);
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
