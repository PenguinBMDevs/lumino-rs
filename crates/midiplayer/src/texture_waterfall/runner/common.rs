//! 贴图瀑布流 runner 公共工具

use std::sync::{Arc, Mutex};

use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::gpu_ctx::WaterfallGpuCtx;
use crate::texture_waterfall::renderer::TextureWaterfallRenderer;

/// 向共享进度缓冲推送一条进度（渲染线程 → UI 线程）
pub fn push_waterfall_progress(progress: &Arc<Mutex<Vec<(String, f32)>>>, msg: &str, value: f32) {
    if let Ok(mut buf) = progress.lock() {
        buf.push((msg.to_string(), value.clamp(0.0, 1.0)));
    }
}

/// 确保贴图瀑布流渲染器与配置已初始化（懒初始化）。
pub fn ensure_renderer_for_config(
    gpu: &WaterfallGpuCtx<'_>,
    renderer: &mut Option<TextureWaterfallRenderer>,
    renderer_config: &mut Option<TextureWaterfallConfig>,
    config: &TextureWaterfallConfig,
) {
    if renderer.is_none() {
        *renderer = Some(TextureWaterfallRenderer::new(
            gpu.device,
            config.clone(),
            gpu.texture_format,
        ));
    }
    *renderer_config = Some(config.clone());
}
