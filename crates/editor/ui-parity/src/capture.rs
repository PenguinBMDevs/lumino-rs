//! iced 无头截图引擎。
//!
//! 为什么不直接用 `iced_test::simulator::Simulator` 输出全部基线？
//! `iced_test 0.14.0` 的 `Snapshot` 把 `scale_factor` 硬编码为 `2.0`，无法生成
//! 1.0 / 1.25 / 1.5 三档真实 DPI 图。本模块复刻其无头渲染路径，但把
//! `scale_factor` 暴露为参数；`iced_test` 仍用于 2x 交叉校验（见文件末尾测试）。

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use iced_core::renderer::{self, Headless};
use iced_core::theme::Base;
use iced_core::{Event, Pixels, Size, clipboard, mouse, time, window};
use iced_runtime::{UserInterface, user_interface};
use lumino_ui_core::Message;

use crate::scene::Scene;

/// 单帧 UI 稳定所需的最大重建次数（画布上报 bounds 后需要重建一次）。
const MAX_SETTLE_FRAMES: usize = 8;

/// 无头渲染得到的 RGBA 图像。
#[derive(Debug, Clone)]
pub struct CapturedImage {
    /// 物理像素宽度。
    pub width: u32,
    /// 物理像素高度。
    pub height: u32,
    /// 渲染使用的 DPI 倍率。
    pub scale_factor: f32,
    /// 连续的 RGBA8 像素数据（长度 = width × height × 4）。
    pub rgba: Vec<u8>,
}

impl CapturedImage {
    /// 将图像写入 PNG 文件（自动创建父目录）。
    pub fn write_png(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建基线目录失败: {}", parent.display()))?;
        }

        image::save_buffer_with_format(
            path,
            &self.rgba,
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .with_context(|| format!("写入 PNG 失败: {}", path.display()))
    }

    /// 与给定 PNG 逐像素比对；语义与 `iced_test::Snapshot::matches_image` 一致：
    ///
    /// - 文件不存在：写入当前图像并返回 `true`；
    /// - 文件存在：尺寸与 RGBA 字节完全一致才返回 `true`。
    pub fn matches_image(&self, path: &Path) -> Result<bool> {
        if !path.exists() {
            self.write_png(path)?;
            return Ok(true);
        }

        let existing = image::open(path)
            .with_context(|| format!("读取基线 PNG 失败: {}", path.display()))?
            .to_rgba8();

        if existing.width() != self.width || existing.height() != self.height {
            return Ok(false);
        }

        Ok(existing.as_raw().as_slice() == self.rgba.as_slice())
    }
}

/// 以指定 DPI 渲染一个场景。
///
/// 逻辑窗口尺寸由 [`Scene::id`] 决定，始终不变；返回图像的物理尺寸为
/// `logical × scale_factor`。这就是 iced 在 HiDPI 下的真实行为：布局使用
/// 逻辑像素，文字与几何按物理像素光栅化。
///
/// # 错误
/// - `scale_factor` 非正数；
/// - 无可用 wgpu 适配器或渲染器初始化失败。
pub fn capture_scene(scene: &mut Scene, scale_factor: f32) -> Result<CapturedImage> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(anyhow!("scale_factor 必须为正数，实际为 {scale_factor}"));
    }

    let (logical_width, logical_height) = scene.id.logical_size();
    let logical_size = Size::new(logical_width as f32, logical_height as f32);
    let physical_size = Size::new(
        (logical_width as f32 * scale_factor).round() as u32,
        (logical_height as f32 * scale_factor).round() as u32,
    );

    let font = lumino_ui_core::font::init(&scene.config);
    let mut renderer = futures::executor::block_on(<iced_wgpu::Renderer as Headless>::new(
        font,
        Pixels(16.0),
        None,
    ))
    .ok_or_else(|| anyhow!("无法创建 iced_wgpu 无头渲染器（无可用 GPU 适配器？）"))?;

    let cursor = mouse::Cursor::Unavailable;
    let mut clipboard = clipboard::Null;

    for frame in 1..=MAX_SETTLE_FRAMES {
        let theme = scene.root.theme();
        let mut messages: Vec<Message> = Vec::new();

        {
            let element = scene.root.view();
            let mut ui = UserInterface::build(
                element,
                logical_size,
                user_interface::Cache::default(),
                &mut renderer,
            );

            let events = [Event::Window(window::Event::RedrawRequested(
                time::Instant::now(),
            ))];
            let _ = ui.update(
                &events,
                cursor,
                &mut renderer,
                &mut clipboard,
                &mut messages,
            );

            // 画布在首帧汇报 `CanvasBoundsChanged`；应用消息后需重建布局。
            // 消息清空（或达到上限）时再绘制，避免把 0×0 画布写进基线。
            if messages.is_empty() || frame == MAX_SETTLE_FRAMES {
                ui.draw(
                    &mut renderer,
                    &theme,
                    &renderer::Style {
                        text_color: theme.base().text_color,
                    },
                    cursor,
                );

                let rgba = <iced_wgpu::Renderer as Headless>::screenshot(
                    &mut renderer,
                    physical_size,
                    scale_factor,
                    theme.base().background_color,
                );

                return Ok(CapturedImage {
                    width: physical_size.width,
                    height: physical_size.height,
                    scale_factor,
                    rgba,
                });
            }
        }

        for message in messages {
            scene.root.update(message);
        }
    }

    Err(anyhow!(
        "UI 未能在 {MAX_SETTLE_FRAMES} 帧内稳定（消息队列持续非空）"
    ))
}

/// 渲染纯色测试图（仅用于单元测试，避免触碰 GPU）。
#[cfg(test)]
fn solid_image(width: u32, height: u32, rgba: [u8; 4]) -> CapturedImage {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        pixels.extend_from_slice(&rgba);
    }
    CapturedImage {
        width,
        height,
        scale_factor: 1.0,
        rgba: pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Scene, SceneId, baseline_config};
    use iced_core::Theme;

    fn temp_png(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "lumino-ui-parity-{}-{}.png",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn test_matches_image_creates_then_compares() -> Result<()> {
        let path = temp_png("roundtrip");
        let _ = std::fs::remove_file(&path);

        let image = solid_image(4, 3, [10, 20, 30, 255]);
        assert!(image.matches_image(&path)?, "首次调用应创建并返回 true");
        assert!(path.exists(), "基线文件应已创建");
        assert!(image.matches_image(&path)?, "相同内容应匹配");

        let mut changed = image;
        changed.rgba[0] = 11;
        assert!(!changed.matches_image(&path)?, "字节变化应判定不匹配");

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    /// 2x 交叉校验：自研截图路径必须与 `iced_test` 的 `matches_image` 逐字节一致。
    ///
    /// 标记 `#[ignore]` 是因为需要真实 GPU；验收时手动执行：
    /// `cargo test -p lumino-ui-parity -- --ignored`
    #[test]
    #[ignore = "需要 GPU：用 iced_test Simulator 交叉校验 2x 截图"]
    fn test_iced_test_matches_custom_capture_at_2x() -> Result<()> {
        let path = temp_png("iced-test-crosscheck");
        let _ = std::fs::remove_file(&path);

        let mut custom_scene = Scene::build(SceneId::SaveConfirmDialog);
        let captured = capture_scene(&mut custom_scene, 2.0)?;
        captured.write_png(&path)?;

        let settings = iced_core::Settings {
            default_font: lumino_ui_core::font::init(&baseline_config()),
            ..Default::default()
        };
        let (width, height) = SceneId::SaveConfirmDialog.logical_size();
        let scene = Scene::build(SceneId::SaveConfirmDialog);
        let theme = Theme::TokyoNightStorm;
        let mut simulator: iced_test::simulator::Simulator<
            '_,
            Message,
            Theme,
            iced_wgpu::Renderer,
        > = iced_test::simulator::Simulator::with_size(
            settings,
            Size::new(width as f32, height as f32),
            scene.root.view(),
        );

        let snapshot = simulator.snapshot(&theme)?;
        let matched = snapshot.matches_image(&path)?;
        let _ = std::fs::remove_file(&path);

        assert!(
            matched,
            "iced_test 2x 快照与自研截图不一致（渲染器/字体/抗锯齿可能不同）"
        );
        Ok(())
    }
}
