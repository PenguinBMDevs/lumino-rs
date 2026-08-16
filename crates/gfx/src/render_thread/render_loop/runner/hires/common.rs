use std::sync::{Arc, Mutex};

use crate::{TextureWaterfallConfig, TextureWaterfallRenderer};

use super::super::context::RenderContext;

/// 向共享进度缓冲推送一条进度（渲染线程 → UI 线程）
pub(crate) fn push_onion_progress(
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
    msg: &str,
    value: f32,
) {
    if let Ok(mut buf) = progress.lock() {
        buf.push((msg.to_string(), value.clamp(0.0, 1.0)));
    }
}

/// 确保高精度渲染器与配置已初始化（懒初始化）。
pub(crate) fn ensure_renderer_for_config(
    ctx: &RenderContext,
    hires_renderer: &mut Option<TextureWaterfallRenderer>,
    hires_config: &mut Option<TextureWaterfallConfig>,
    config: &TextureWaterfallConfig,
) {
    if hires_renderer.is_none() {
        *hires_renderer = Some(TextureWaterfallRenderer::new(
            &ctx.device,
            config.clone(),
            ctx.texture_format,
        ));
    }
    *hires_config = Some(config.clone());
}
