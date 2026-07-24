pub mod types;

use crate::gpu_resource_tracker;

pub use types::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, OnionBgTileRef, RenderUniform,
    pack_color, unpack_color,
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
        let instance = NoteInstance::new(100.0, 60.0, 200.0, [1.0, 0.5, 0.0, 0.8]);

        assert_eq!(instance.position, [100.0, 60.0]);
        assert_eq!(instance.size_x, 200.0);
        // 颜色打包后解包验证
        let unpacked = crate::note_renderer::types::unpack_color(instance.color_packed);
        assert!((unpacked[0] - 1.0).abs() < 0.01);
        assert!((unpacked[1] - 0.5).abs() < 0.01);
        assert!(unpacked[2].abs() < 0.01);
        assert!((unpacked[3] - 0.8).abs() < 0.01);
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

        // 验证初始容量已增大
        assert!(rendering::INITIAL_INSTANCE_CAPACITY >= 65536);
        // 验证扩容因子
        assert_eq!(rendering::BUFFER_GROWTH_FACTOR, 2);
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
}
