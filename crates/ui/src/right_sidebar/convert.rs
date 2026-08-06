//! 图片转 MIDI 转换执行（i2m-rs 封装）
//!
//! 在后台线程调用 i2m-rs 的完整管线：
//! `load_image → generate_palette → convert`，并将 `ConversionResult` 的
//! NoteOn/NoteOff 事件流解析为每色一轨的预览音符（`ImageToMidiPreview`）。

use lumino_editor_state::{ImageToMidiPreview, PreviewNote};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// 后台转换结果（`Err` 携带可展示的错误信息）
pub type ConvertResult = Result<ImageToMidiPreview, String>;

/// 执行图片转 MIDI 转换（可在线程内调用）
///
/// 使用 i2m-rs `ConverterConfig::default()`（预留配置项，后续迭代可在面板提供参数 UI）。
pub fn run_conversion(path: &Path) -> ConvertResult {
    use i2m_rs::cluster::generate_palette;

    // 1. 加载图片（PNG/JPEG/BMP/GIF/WebP/SVG）
    let image = i2m_rs::load_image(path).map_err(|e| format!("图片解码失败: {e}"))?;

    // 2. 默认配置 + 生成调色板（默认 KMeans++，16 色 = 16 轨）
    let config = i2m_rs::ConverterConfig::default();
    let (palette, _dithered) = generate_palette(&image, &config.palette, config.color_count)
        .map_err(|e| format!("调色板生成失败: {e}"))?;

    // 3. 转换为时序事件（每调色板颜色一轨）
    let cancel = AtomicBool::new(false);
    let result = i2m_rs::convert(&image, &palette, &config, None, &cancel)
        .map_err(|e| format!("转换失败: {e}"))?;

    // 4. 解析 NoteOn/NoteOff 事件流为音符（pixel-tick × ticks_per_pixel → MIDI tick）
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

    #[test]
    fn test_pairs_note_events() {
        // 构造一个临时 PNG（红色竖条），走完整转换管线
        let tmp_dir = std::env::temp_dir();
        let img_path = tmp_dir.join("lumino_i2m_test.png");
        // 32x32 纯红图（2x2 会被 AreaResampling 放大产生透明边缘，音符为空）
        {
            let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([255u8, 0, 0, 255]));
            img.save(&img_path).expect("保存测试图片失败");
        }

        let preview = run_conversion(&img_path).expect("转换应成功");
        // 单色图 → 至少 1 轨，且每轨音符数 > 0（每列一个音符）
        assert!(!preview.tracks.is_empty());
        assert!(preview.tracks.iter().any(|t| !t.is_empty()));
        assert!(preview.orig_width > 0.0);

        let _ = std::fs::remove_file(&img_path);
    }
}
