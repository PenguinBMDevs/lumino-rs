use iced_core::{Font, Pixels};
use iced_wgpu::{Engine, Renderer};

use crate::config;

/// 创建 iced 渲染器
pub(super) fn create_renderer(gfx: &lumino_gfx::Context, font: Font) -> Renderer {
    let engine = Engine::new(
        &gfx.adapter,
        gfx.device.clone(),
        gfx.queue.clone(),
        gfx.format,
        None,
        iced_wgpu::graphics::Shell::headless(),
    );
    Renderer::new(engine, font, Pixels::from(16))
}

/// 根据配置创建字体
///
/// 使用系统字体名称或默认字体
///
/// 注意：Font::with_name 需要 'static 字符串，
/// 我们使用 Box::leak 来创建一个静态字符串引用
pub(super) fn create_font_from_config(ui_config: &config::UiConfig) -> Font {
    // 优先使用自定义字体路径
    if !ui_config.program_font_path.is_empty() {
        let path = std::path::Path::new(&ui_config.program_font_path);
        if path.exists() {
            tracing::info!("检测到自定义字体路径: {:?}", path);
            // 自定义字体文件加载需要重启应用才能生效
            // 这里只记录日志
        }
    }

    // 其次使用系统字体名称
    if !ui_config.program_font_name.is_empty() {
        // 将 String 转换为 'static str
        // Box::leak 会泄漏内存，但配置变更频率很低，这是可接受的权衡
        let static_name: &'static str =
            Box::leak(ui_config.program_font_name.clone().into_boxed_str());

        tracing::info!("应用字体: {}", ui_config.program_font_name);
        return Font::with_name(static_name);
    }

    // 使用默认字体
    tracing::info!("使用默认字体 (SansSerif)");
    Font::default()
}
