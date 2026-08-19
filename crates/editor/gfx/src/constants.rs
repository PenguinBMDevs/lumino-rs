//! GFX 模块常量

/// 渲染相关常量
pub mod rendering {
    /// 初始实例缓冲区容量（黑乐谱场景需要更大预分配）
    pub const INITIAL_INSTANCE_CAPACITY: usize = 65536;
    /// 实例缓冲区扩容因子
    pub const BUFFER_GROWTH_FACTOR: usize = 2;
    /// 单次最大上传实例数（防止一次性上传过多导致帧卡顿）
    pub const MAX_UPLOAD_PER_FRAME: usize = 500_000;
    /// 深度纹理格式
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// 网格渲染常量
    pub mod grid {
        /// 默认每小节 tick 数 (1920)
        pub const TICKS_PER_MEASURE: u32 = 1920;
        /// 默认每拍 tick 数 (480)
        pub const TICKS_PER_BEAT: u32 = 480;

        /// 默认网格颜色（独立线程渲染路径）
        pub mod colors {
            /// 黑键分隔线颜色 (RGBA)
            pub const BLACK_KEY_LINE: [f32; 4] = [0.15, 0.15, 0.15, 1.0];
            /// 白键分隔线颜色 (RGBA)
            pub const WHITE_KEY_LINE: [f32; 4] = [0.1, 0.1, 0.1, 1.0];
            /// 小节线颜色 (RGBA)
            pub const BAR_LINE: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
            /// 拍子线颜色 (RGBA)
            pub const BEAT_LINE: [f32; 4] = [0.2, 0.2, 0.2, 1.0];
            /// 半拍线颜色 (RGBA)
            pub const HALF_BEAT_LINE: [f32; 4] = [0.2, 0.2, 0.2, 0.5];
            /// 细分网格线颜色 (RGBA，带透明度)
            pub const GRID_LINE: [f32; 4] = [0.2, 0.2, 0.2, 0.2];
        }
    }

    /// 标准深度/模板状态
    pub fn depth_stencil_state() -> Option<wgpu::DepthStencilState> {
        depth_stencil_state_for(true)
    }

    /// 根据是否需要 depth 返回对应的深度/模板状态
    ///
    /// 视频导出为纯 2D 渲染，使用 `false` 可创建与无 depth attachment 的 RenderPass 兼容的管线。
    pub fn depth_stencil_state_for(needs_depth: bool) -> Option<wgpu::DepthStencilState> {
        needs_depth.then_some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        })
    }

    /// 只读深度状态（仅比较、不写深度）：供网格等底层绘制使用。
    ///
    /// 音符（含洋葱皮）通过深度编码轨道优先级解决重叠闪烁；若网格仍写深度
    /// （z=0.0），会遮挡后绘制且 z>0 的洋葱皮音符。
    pub fn depth_stencil_state_read_only_for(needs_depth: bool) -> Option<wgpu::DepthStencilState> {
        needs_depth.then_some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        })
    }

    /// 判断渲染管线深度/模板状态与 RenderPass 是否兼容。
    ///
    /// 当 RenderPass 不带 depth attachment 时，管线必须也没有 depth-stencil 状态；
    /// 当 RenderPass 带 depth attachment 时，管线必须携带匹配的 depth-stencil 状态。
    #[must_use]
    pub fn is_depth_stencil_compatible(
        render_pass_has_depth: bool,
        pipeline_depth_stencil: Option<&wgpu::DepthStencilState>,
    ) -> bool {
        match (render_pass_has_depth, pipeline_depth_stencil) {
            (false, None) => true,
            (true, Some(state)) => state.format == DEPTH_FORMAT,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rendering::{DEPTH_FORMAT, depth_stencil_state_for, is_depth_stencil_compatible};

    #[test]
    fn test_depth_stencil_state_for_video_export_is_none() {
        assert!(
            depth_stencil_state_for(false).is_none(),
            "视频导出的纯 2D 渲染不应携带 depth-stencil 状态"
        );
    }

    #[test]
    fn test_depth_stencil_state_for_ui_has_depth_format() {
        let state = depth_stencil_state_for(true).expect("普通 UI 渲染应启用 depth-stencil 状态");
        assert_eq!(state.format, DEPTH_FORMAT);
        assert!(state.depth_write_enabled);
    }

    #[test]
    fn test_video_export_pipeline_compatible_with_no_depth_pass() {
        let pipeline_state = depth_stencil_state_for(false);
        assert!(is_depth_stencil_compatible(false, pipeline_state.as_ref()));
    }

    #[test]
    fn test_ui_pipeline_compatible_with_depth_pass() {
        let pipeline_state = depth_stencil_state_for(true);
        assert!(is_depth_stencil_compatible(true, pipeline_state.as_ref()));
    }

    #[test]
    fn test_depth_stencil_mismatch_is_incompatible() {
        // 视频导出管线（无 depth）不能用于带 depth attachment 的 RenderPass
        let pipeline_state = depth_stencil_state_for(false);
        assert!(!is_depth_stencil_compatible(true, pipeline_state.as_ref()));

        // 普通 UI 管线（有 depth）不能用于无 depth attachment 的 RenderPass
        let pipeline_state = depth_stencil_state_for(true);
        assert!(!is_depth_stencil_compatible(false, pipeline_state.as_ref()));
    }
}
