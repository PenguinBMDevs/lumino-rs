//! egui 无头截图：用 `egui_kittest` 的 wgpu 测试渲染器输出 PNG。

use std::path::Path;

use anyhow::{Context, Result};
use image::{ColorType, ImageFormat};

use crate::scene::{SceneId, SceneState};

/// 截图像素数据。
#[derive(Debug, Clone)]
pub struct CapturedImage {
    /// 物理宽度。
    pub width: u32,
    /// 物理高度。
    pub height: u32,
    /// DPI。
    pub scale_factor: f32,
    /// RGBA8 直通数据。
    pub rgba: Vec<u8>,
}

impl CapturedImage {
    /// 生成 PNG。
    pub fn write_png(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建截图目录失败: {}", parent.display()))?;
        }
        image::save_buffer_with_format(
            path,
            &self.rgba,
            self.width,
            self.height,
            ColorType::Rgba8,
            ImageFormat::Png,
        )
        .with_context(|| format!("写入 PNG 失败: {}", path.display()))
    }

    /// 与已有 PNG 逐字节比对；文件不存在时创建并返回 true。
    pub fn matches_image(&self, path: &Path) -> Result<bool> {
        if !path.exists() {
            self.write_png(path)?;
            return Ok(true);
        }
        let existing = image::open(path)
            .with_context(|| format!("读取截图失败: {}", path.display()))?
            .to_rgba8();
        if existing.width() != self.width || existing.height() != self.height {
            return Ok(false);
        }
        Ok(existing.as_raw().as_slice() == self.rgba.as_slice())
    }
}

/// 渲染一个场景。
pub fn capture_scene(scene: SceneId, scale_factor: f32) -> Result<CapturedImage> {
    let (width, height) = scene.logical_size();
    let state = SceneState::new(scene);

    // 在首帧之前设置主题与字体：egui_kittest 的 table 列宽会记住首帧布局。
    let mut harness = egui_kittest::Harness::builder()
        .wgpu()
        .with_size(egui::vec2(width as f32, height as f32))
        .with_pixels_per_point(scale_factor)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            crate::theme::apply(ctx);
            crate::fonts::install(ctx);
        })
        .build_ui_state(|ui, state: &mut SceneState| state.show(ui), state);

    harness.run();
    let image = harness
        .render()
        .map_err(|error| anyhow::anyhow!("egui_kittest 渲染失败: {error}"))?;
    let (width, height) = image.dimensions();
    Ok(CapturedImage {
        width,
        height,
        scale_factor,
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_image_roundtrip() -> Result<()> {
        let path =
            std::env::temp_dir().join(format!("lumino-ui-egui-capture-{}.png", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let image = CapturedImage {
            width: 2,
            height: 2,
            scale_factor: 1.0,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255],
        };
        assert!(image.matches_image(&path)?);
        assert!(path.exists());
        assert!(image.matches_image(&path)?);
        let _ = std::fs::remove_file(&path);
        Ok(())
    }
}
