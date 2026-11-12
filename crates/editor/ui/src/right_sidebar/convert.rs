//! 图片转 MIDI 转换执行（i2m-rs 封装）
//!
//! 在后台线程调用 i2m-rs 的完整管线：
//! `load_image → generate_palette → convert`，并将 `ConversionResult` 的
//! NoteOn/NoteOff 事件流解析为每色一轨的预览音符（`ImageToMidiPreview`）。

use crate::right_sidebar::I2mConfig;
use lumino_core::trf;
use lumino_editor_state::{ImageToMidiPreview, PreviewNote};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// 后台转换结果（`Err` 携带可展示的错误信息）
pub type ConvertResult = Result<ImageToMidiPreview, String>;

/// 预览管线允许的最大边长（像素）
///
/// `i2m-rs` 0.1.1 的 `load_image` 不限制尺寸，直接按原图聚类/抖动会在大图上
/// 卡死；这里在加载后、调色板生成前按面积重采样把长边压到 2400。
pub const MAX_IMAGE_SIDE: u32 = 2400;

/// 长边超过 [`MAX_IMAGE_SIDE`] 时等比降采样（面积重采样）
fn clamp_long_side(image: i2m_rs::RgbaImage) -> Result<i2m_rs::RgbaImage, String> {
    let longest = image.width.max(image.height);
    if longest <= MAX_IMAGE_SIDE {
        return Ok(image);
    }
    let scale = f64::from(MAX_IMAGE_SIDE) / f64::from(longest);
    let width = ((f64::from(image.width) * scale).round() as u32).max(1);
    let height = ((f64::from(image.height) * scale).round() as u32).max(1);
    i2m_rs::resize(
        &image,
        width,
        height,
        i2m_rs::ResizeAlgorithm::AreaResampling,
    )
    .map_err(|e| trf!("图片降采样失败: {e}", e = e))
}

/// 执行图片转 MIDI 转换（可在线程内调用）
///
/// 管线：`load_image → 限边长 → adjust（本仓后处理）→ generate_palette → convert`。
/// 面板参数逐项映射到 `i2m-rs` `ConverterConfig`；混合模式映射为
/// `PaletteSource` 的抖动变体，`generate_palette` 返回的抖动图像必须喂给
/// `convert`（否则抖动只是生成了调色板，观感不变）。
pub fn run_conversion(path: &Path, ui_config: &I2mConfig) -> ConvertResult {
    use i2m_rs::cluster::generate_palette;

    // 1. 加载图片（PNG/JPEG/BMP/GIF/WebP/SVG）
    let image = i2m_rs::load_image(path).map_err(|e| trf!("图片解码失败: {e}", e = e))?;
    let mut image = clamp_long_side(image)?;

    // 2. 颜色后处理：必须在调色板生成之前，否则调色板还是按旧颜色聚类
    let adjust = ui_config.adjust();
    lumino_extras::adjust_bgra(
        image.width,
        image.height,
        image.stride,
        &mut image.pixels,
        &adjust,
    );

    // 3. 用户配置 + 生成调色板（算法与颜色数来自面板）
    let config = ui_config.to_converter_config();
    let (palette, dithered) = generate_palette(&image, &config.palette, config.color_count)
        .map_err(|e| trf!("调色板生成失败: {e}", e = e))?;

    // 4. 转换为时序事件（每调色板颜色一轨）
    // 抖动算法额外返回「已量化到调色板」的图像，必须用它做转换；
    // 其余算法返回 None，仍用原图（convert 内部自行做最近色匹配）。
    let source = dithered.as_ref().unwrap_or(&image);
    let cancel = AtomicBool::new(false);
    let result = i2m_rs::convert(source, &palette, &config, None, &cancel)
        .map_err(|e| trf!("转换失败: {e}", e = e))?;

    // 5. 解析 NoteOn/NoteOff 事件流为音符（pixel-tick × ticks_per_pixel → MIDI tick）
    // i2m-rs 的事件按"列"集中输出：同 tick 内先推全部列的 NoteOn（再于尾部集中 NoteOff），
    // 单 pending 栈无法配对，必须按 key 分别跟踪各列的响铃状态。
    let ticks_per_pixel = u64::from(config.ticks_per_pixel.max(1));
    let mut tracks = Vec::with_capacity(result.track_events.len());
    for events in &result.track_events {
        let mut notes: Vec<PreviewNote> = Vec::new();
        let mut pending_on: std::collections::HashMap<u8, u64> = std::collections::HashMap::new();
        for ev in events {
            match &ev.event {
                i2m_rs::MidiEvent::NoteOn { key, .. } => {
                    // 同一 key 再次 NoteOn（颜色分段重触发）前若旧音符未收尾，
                    // 先按上一 NoteOn 起算收尾（保守语义，正常流程 NoteOff 已配对）
                    if let Some(&start) = pending_on.get(key) {
                        notes.push(PreviewNote {
                            tick: (start * ticks_per_pixel) as f32,
                            length: (ev.tick.saturating_sub(start) * ticks_per_pixel).max(1) as f32,
                            key: *key,
                        });
                    }
                    pending_on.insert(*key, ev.tick);
                }
                i2m_rs::MidiEvent::NoteOff { key, .. } => {
                    if let Some(start) = pending_on.remove(key) {
                        notes.push(PreviewNote {
                            tick: (start * ticks_per_pixel) as f32,
                            length: (ev.tick.saturating_sub(start) * ticks_per_pixel).max(1) as f32,
                            key: *key,
                        });
                    }
                }
                _ => {}
            }
        }
        // 收尾：文件末尾未收到 NoteOff 的响铃音符（emit_final_note_offs 已覆盖，
        // 此处兜底）
        for (key, start) in pending_on.drain() {
            notes.push(PreviewNote {
                tick: (start * ticks_per_pixel) as f32,
                length: 1.0,
                key,
            });
        }
        tracks.push(notes);
    }

    // 预览原始宽度 = 总时长（pixel-tick × ticks_per_pixel）
    let orig_width = (result.height as u64 * ticks_per_pixel) as f32;

    Ok(ImageToMidiPreview { tracks, orig_width })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_message::I2mSelectField;

    /// 生成一张双色测试图（上半红、下半蓝），返回路径与清理守卫
    fn sample_image(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut img = image::RgbaImage::from_pixel(64, 32, image::Rgba([255u8, 0, 0, 255]));
        for y in 16..32 {
            for x in 0..64 {
                img.put_pixel(x, y, image::Rgba([0, 0, 255, 255]));
            }
        }
        img.save(&path).expect("保存测试图片失败");
        path
    }

    #[test]
    fn test_pairs_note_events() {
        // 构造一个临时 PNG（红色竖条），走完整转换管线
        let img_path = std::env::temp_dir().join("lumino_i2m_test.png");
        // 32x32 纯红图（2x2 会被 AreaResampling 放大产生透明边缘，音符为空）
        {
            let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([255u8, 0, 0, 255]));
            img.save(&img_path).expect("保存测试图片失败");
        }

        let preview = run_conversion(&img_path, &I2mConfig::default()).expect("转换应成功");
        // 单色图 → 至少 1 轨，且每轨音符数 > 0（每列一个音符）
        assert!(!preview.tracks.is_empty());
        assert!(preview.tracks.iter().any(|t| !t.is_empty()));
        assert!(preview.orig_width > 0.0);

        let _ = std::fs::remove_file(&img_path);
    }

    #[test]
    fn test_white_key_mode_only_emits_white_keys() {
        let path = sample_image("lumino_i2m_white.png");
        let mut config = I2mConfig::default();
        config.set_selection(I2mSelectField::KeyMode, 1); // 白键（填充）
        let preview = run_conversion(&path, &config).expect("转换应成功");

        const WHITE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
        let keys: Vec<u8> = preview
            .tracks
            .iter()
            .flatten()
            .map(|note| note.key)
            .collect();
        assert!(!keys.is_empty(), "应生成音符");
        assert!(
            keys.iter().all(|key| WHITE.contains(&(key % 12))),
            "白键模式不应产生黑键音符"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_all_keys_mode_uses_black_keys() {
        // 对照组：默认（全部键）在 88 键范围内必然覆盖到黑键
        let path = sample_image("lumino_i2m_all.png");
        let preview = run_conversion(&path, &I2mConfig::default()).expect("转换应成功");
        const WHITE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
        let has_black = preview
            .tracks
            .iter()
            .flatten()
            .any(|note| !WHITE.contains(&(note.key % 12)));
        assert!(has_black, "全部键模式应覆盖黑键列");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_split_to_grid_caps_note_length() {
        let path = sample_image("lumino_i2m_grid.png");
        let mut config = I2mConfig::default();
        config.set_selection(I2mSelectField::NoteLengthMode, 2); // 按网格切分
        config.apply_text(lumino_message::I2mConfigField::MaxNoteLength, "2");
        let preview = run_conversion(&path, &config).expect("转换应成功");

        let longest = preview
            .tracks
            .iter()
            .flatten()
            .map(|note| note.length)
            .fold(0.0f32, f32::max);
        assert!(longest <= 2.0, "最长音符应被网格切到 2 tick：{longest}");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_large_image_is_downscaled_before_conversion() {
        // 长边 3000 > 2400：应先等比压到 2400，避免聚类/抖动在超大图上卡死
        let img = image::RgbaImage::from_pixel(3000, 100, image::Rgba([0u8, 200, 0, 255]));
        let scaled = clamp_long_side(large_image(&img)).expect("降采样应成功");
        assert_eq!(scaled.width, MAX_IMAGE_SIDE);
        assert_eq!(scaled.height, 80);
    }

    /// 把 `image` crate 的 RGBA 转成 i2m-rs 的 BGRA 缓冲（测试用）
    fn large_image(img: &image::RgbaImage) -> i2m_rs::RgbaImage {
        let mut pixels = Vec::with_capacity(img.len());
        for chunk in img.as_raw().chunks_exact(4) {
            pixels.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
        }
        i2m_rs::RgbaImage {
            width: img.width(),
            height: img.height(),
            stride: img.width() as usize * 4,
            pixels,
        }
    }

    #[test]
    fn test_blend_spread_changes_output_on_gradient() {
        // 大图（1200×400 渐变）是混合模式的真实使用场景：
        // 抖动在调色板生成阶段就改写图像，后续管线必须拿得到这份结果。
        let path = std::env::temp_dir().join("lumino_i2m_gradient.png");
        {
            let mut img = image::RgbaImage::new(1200, 400);
            for y in 0..400u32 {
                for x in 0..1200u32 {
                    let r = (x % 256) as u8;
                    let g = (y % 256) as u8;
                    let b = ((x / 5 + y / 3) % 256) as u8;
                    img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                }
            }
            img.save(&path).expect("保存测试图片失败");
        }

        let none = run_conversion(&path, &I2mConfig::default()).expect("转换应成功");
        let mut spread_cfg = I2mConfig::default();
        spread_cfg.set_selection(I2mSelectField::Blend, 1);
        let spread = run_conversion(&path, &spread_cfg).expect("转换应成功");

        let signature = |preview: &ImageToMidiPreview| {
            let mut notes: Vec<(u32, u8, u32)> = preview
                .tracks
                .iter()
                .flatten()
                .map(|note| (note.tick as u32, note.key, note.length as u32))
                .collect();
            notes.sort_unstable();
            notes
        };
        assert_ne!(
            signature(&none),
            signature(&spread),
            "误差扩散应改变音符分布（否则抖动只是生成了调色板）"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_blend_spread_returns_dithered_source() {
        // 「混合模式 = PaletteSource」的接线前提：抖动算法会额外返回抖动后的图像
        let mut img = i2m_rs::RgbaImage::new(8, 8, i2m_rs::Color::new(255, 0, 0, 255));
        for y in 0..8 {
            for x in 4..8 {
                img.set(x, y, i2m_rs::Color::new(0, 0, 255, 255));
            }
        }
        let (palette, dithered) = i2m_rs::cluster::generate_palette(
            &img,
            &i2m_rs::PaletteSource::FloydSteinbergDither,
            2,
        )
        .expect("调色板生成应成功");
        assert_eq!(palette.colors.len(), 2);
        assert!(dithered.is_some(), "误差扩散必须返回抖动图像");

        let (_palette2, dithered2) =
            i2m_rs::cluster::generate_palette(&img, &i2m_rs::PaletteSource::KMeansPlusPlus, 2)
                .expect("调色板生成应成功");
        assert!(dithered2.is_none(), "非抖动算法不返回抖动图像");
    }
}
