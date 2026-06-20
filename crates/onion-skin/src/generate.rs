//! 后台贴图生成逻辑

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use crate::types::{GenerateProgress, GenerateResult, OnionSkinNote};

/// 贴图宽度（固定 4096 像素）
pub(crate) const TEXTURE_WIDTH: u32 = 4096;

/// 在后台线程中执行贴图生成
///
/// # 参数
/// - `ppq`: 每四分音符的 tick 数（用于 tick→ms 转换）
///
/// 返回 `None` 表示被取消。
pub(crate) fn generate_pixels(
    notes: &[Vec<OnionSkinNote>],
    duration_ms: u32,
    height: u32,
    ppq: u32,
    tempo_table: Option<&[(u32, f32)]>,
    progress_tx: &mpsc::Sender<GenerateProgress>,
    cancel_flag: &AtomicBool,
) -> Option<GenerateResult> {
    let width = TEXTURE_WIDTH;
    let total_tracks = notes.len();

    // 创建像素缓冲区，初始全透明黑
    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];

    // 如果 duration 为 0，直接返回空贴图
    if duration_ms == 0 {
        let _ = progress_tx.send(GenerateProgress {
            processed_tracks: total_tracks,
            total_tracks,
        });
        return Some(GenerateResult { pixels, height });
    }

    let duration_f = duration_ms as f32;

    for (track_idx, track_notes) in notes.iter().enumerate() {
        if cancel_flag.load(Ordering::SeqCst) {
            return None;
        }

        for note in track_notes {
            // 转换 tick 到毫秒（如果需要）
            let (start_ms, end_ms) = if let Some(tempo) = tempo_table {
                let s = tick_to_ms(note.start_tick, ppq, tempo);
                let e = tick_to_ms(note.end_tick, ppq, tempo);
                (s, e)
            } else {
                (note.start_ms, note.end_ms)
            };

            // 计算贴图 X 范围
            let x_start =
                ((start_ms / duration_f) * (width as f32)).clamp(0.0, (width - 1) as f32) as u32;
            let x_end = ((end_ms / duration_f) * (width as f32)).clamp(0.0, width as f32) as u32;

            // 计算贴图 Y
            let y = (note.key as u32).clamp(0, height - 1);

            // 写入颜色（简单覆盖，不 blend）
            let color = note.color;
            for x in x_start..x_end {
                let idx = ((y * width + x) * 4) as usize;
                pixels[idx] = color[0];
                pixels[idx + 1] = color[1];
                pixels[idx + 2] = color[2];
                pixels[idx + 3] = 255;
            }
        }

        // 每处理完一个音轨发送进度
        let _ = progress_tx.send(GenerateProgress {
            processed_tracks: track_idx + 1,
            total_tracks,
        });
    }

    Some(GenerateResult { pixels, height })
}

/// 将 tick 转换为毫秒
///
/// # 参数
/// - `tick`: MIDI tick 值
/// - `ppq`: 每四分音符的 tick 数（Pulses Per Quarter note）
/// - `tempo_table`: tempo 变化表，每项为 (tick, BPM)
pub(crate) fn tick_to_ms(tick: u32, ppq: u32, tempo_table: &[(u32, f32)]) -> f32 {
    if tempo_table.is_empty() {
        // 默认 120 BPM, PPQ=480
        // µs per tick = 60_000_000 / (120 * 480) ≈ 1041.67
        // ms per tick ≈ 1.04167
        return tick as f32 * 60_000_000.0 / (120.0 * ppq as f32) / 1000.0;
    }

    let ppq_f = ppq as f64;
    let mut ms = 0.0f64;
    let mut prev_tick = 0u32;
    // µs per quarter note, 默认 120 BPM
    let mut prev_tempo_uspq = 500_000.0f64;

    for &(tick_at, bpm) in tempo_table {
        if tick_at >= tick {
            let dt = (tick - prev_tick) as f64;
            ms += dt * prev_tempo_uspq / (ppq_f * 1000.0);
            return ms as f32;
        }
        let dt = (tick_at - prev_tick) as f64;
        ms += dt * prev_tempo_uspq / (ppq_f * 1000.0);
        prev_tick = tick_at;
        prev_tempo_uspq = 60_000_000.0 / bpm as f64;
    }

    // 超出最后一个 tempo 点
    let dt = (tick - prev_tick) as f64;
    ms += dt * prev_tempo_uspq / (ppq_f * 1000.0);
    ms as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OnionSkinNote;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    #[test]
    fn test_generate_pixels_empty_duration() {
        let cancel = AtomicBool::new(false);
        let (tx, _rx) = mpsc::channel();

        let result = generate_pixels(&[], 0, 128, 1920, None, &tx, &cancel);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.pixels.len(), (4096 * 128 * 4) as usize);
        assert!(result.pixels.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_generate_pixels_single_note() {
        let cancel = AtomicBool::new(false);
        let (tx, _rx) = mpsc::channel();

        let notes = vec![vec![OnionSkinNote::from_ms(
            0.0,
            1000.0,
            60,
            [255, 0, 0, 255],
        )]];
        let result = generate_pixels(&notes, 1000, 128, 1920, None, &tx, &cancel);

        assert!(result.is_some());
        let pixels = result.unwrap().pixels;

        let y = 60u32;
        let idx = ((y * 4096 + 0) * 4) as usize;
        assert_eq!(pixels[idx], 255);
        assert_eq!(pixels[idx + 1], 0);
        assert_eq!(pixels[idx + 2], 0);
        assert_eq!(pixels[idx + 3], 255);
    }

    #[test]
    fn test_generate_pixels_track_overlay() {
        let cancel = AtomicBool::new(false);
        let (tx, _rx) = mpsc::channel();

        // track 0: 红色, track 1: 蓝色（应覆盖红色）
        let notes = vec![
            vec![OnionSkinNote::from_ms(0.0, 1000.0, 60, [255, 0, 0, 255])],
            vec![OnionSkinNote::from_ms(0.0, 500.0, 60, [0, 0, 255, 255])],
        ];

        let result = generate_pixels(&notes, 1000, 128, 1920, None, &tx, &cancel);
        assert!(result.is_some());
        let pixels = result.unwrap().pixels;

        // x=0: track 1 覆盖 → 蓝色
        let y = 60u32;
        let idx0 = ((y * 4096 + 0) * 4) as usize;
        assert_eq!(pixels[idx0], 0);
        assert_eq!(pixels[idx0 + 1], 0);
        assert_eq!(pixels[idx0 + 2], 255);
        assert_eq!(pixels[idx0 + 3], 255);

        // x=750 (500-1000ms): 只有 track 0 → 红色
        let x_mid = (500.0 / 1000.0 * 4096.0) as u32 + 1;
        let idx_mid = ((y * 4096 + x_mid) * 4) as usize;
        assert_eq!(pixels[idx_mid], 255);
        assert_eq!(pixels[idx_mid + 1], 0);
        assert_eq!(pixels[idx_mid + 2], 0);
        assert_eq!(pixels[idx_mid + 3], 255);
    }

    #[test]
    fn test_generate_pixels_key_clamp() {
        let cancel = AtomicBool::new(false);
        let (tx, _rx) = mpsc::channel();

        let notes = vec![vec![OnionSkinNote::from_ms(
            0.0,
            1000.0,
            200,
            [255, 0, 0, 255],
        )]];

        let result = generate_pixels(&notes, 1000, 128, 1920, None, &tx, &cancel);
        assert!(result.is_some());
        let pixels = result.unwrap().pixels;

        // key=200 → clamp 到 127
        let y = 127u32;
        let idx = ((y * 4096 + 0) * 4) as usize;
        assert_eq!(pixels[idx], 255);
    }

    #[test]
    fn test_generate_progress_sends_per_track() {
        let cancel = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel();

        let notes = vec![
            vec![OnionSkinNote::from_ms(0.0, 100.0, 60, [255; 4])],
            vec![OnionSkinNote::from_ms(0.0, 100.0, 61, [255; 4])],
            vec![OnionSkinNote::from_ms(0.0, 100.0, 62, [255; 4])],
        ];

        let _result = generate_pixels(&notes, 1000, 128, 1920, None, &tx, &cancel);

        let mut count = 0;
        while let Ok(progress) = rx.try_recv() {
            count += 1;
            assert_eq!(progress.total_tracks, 3);
        }
        assert_eq!(count, 3);
    }

    #[test]
    fn test_cancel_generation() {
        let cancel = AtomicBool::new(true);
        let (tx, _rx) = mpsc::channel();

        let notes = vec![vec![OnionSkinNote::from_ms(0.0, 1000.0, 60, [255; 4])]];
        let result = generate_pixels(&notes, 1000, 128, 1920, None, &tx, &cancel);
        assert!(result.is_none());
    }

    #[test]
    fn test_tick_to_ms_default_tempo() {
        // DEFAULT_PPQ=1920, 120 BPM
        // µs per tick = 60_000_000 / (120 * 1920) ≈ 260.42
        // ms per tick ≈ 0.26042
        // 480 ticks ≈ 125ms
        let ms = tick_to_ms(480, 1920, &[]);
        assert!((ms - 125.0).abs() < 0.1);
    }

    #[test]
    fn test_tick_to_ms_with_tempo() {
        let tempo = vec![(0u32, 120.0f32), (1920, 60.0f32)];
        // PPQ=1920, tick=960: 960 ticks at 120 BPM
        // µs per tick = 60_000_000 / (120 * 1920) ≈ 260.42
        // 960 * 260.42 / 1000 ≈ 250ms
        let ms = tick_to_ms(960, 1920, &tempo);
        assert!((ms - 250.0).abs() < 0.1);

        // tick=3840: 1920 ticks at 120 BPM + 1920 ticks at 60 BPM
        // = 1920*260.42/1000 + 1920*520.83/1000 = 500 + 1000 = 1500ms
        let ms = tick_to_ms(3840, 1920, &tempo);
        assert!((ms - 1500.0).abs() < 0.1);
    }
}
