//! 高精度贴图生成逻辑
//!
//! 复用 P1 onion-skin 的坐标映射约定（key 0 = row 0，tick→X 线性），
//! 增加时间组裁剪：音符跨越组边界时只取组内部分。

use crate::types::{GroupTile, TileCoord, TrackTile};
use lumino_onion_skin::OnionSkinNote;

/// 生成单音轨在一个时间组的高精度贴图
///
/// # 参数
/// - `notes`: 该音轨的音符列表，**必须按 `start_ms` 升序排列**；调用方负责排序
/// - `track_idx`: 音轨索引
/// - `time_group`: 时间组索引
/// - `tick_start`: 时间组起始 tick（含）
/// - `tick_end`: 时间组结束 tick（不含）
/// - `width`: 贴图宽度（像素）
/// - `key_count`: key 数量（= 贴图高度，1 key = 1 px）
///
/// 注意：`OnionSkinNote` 的 `start_ms`/`end_ms` 字段实为 tick 值
/// （与 P1 file.rs 的 `from_ms(tick, tick, ...)` 构造方式一致）。
pub fn generate_track_tile(
    notes: &[OnionSkinNote],
    track_idx: u16,
    time_group: u32,
    tick_start: u32,
    tick_end: u32,
    width: u32,
    key_count: u16,
) -> TrackTile {
    let height = key_count as u32;
    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];

    // tick 范围为 0 或无效时直接返回空贴图
    if tick_end <= tick_start {
        return TrackTile {
            track_idx,
            time_group,
            pixels,
            width,
            height,
            tick_start,
            tick_end,
        };
    }

    let tick_range = (tick_end - tick_start) as f32;
    // 预计算 scale 因子，将每音符两次除法改为两次乘法
    let scale_x = width as f32 / tick_range;

    // 二分定位与当前时间组重叠的音符区间，避免全量扫描
    let tick_start_f = tick_start as f32;
    let tick_end_f = tick_end as f32;
    let start_idx = notes.partition_point(|n| n.end_ms < tick_start_f);
    let range = &notes[start_idx..];
    let count = range.partition_point(|n| n.start_ms < tick_end_f);

    for note in &range[..count] {
        // 裁剪到当前时间组范围（音符跨越组边界时只取组内部分）
        let note_start = note.start_ms as u32;
        let note_end = note.end_ms as u32;
        let effective_start = note_start.max(tick_start);
        let effective_end = note_end.min(tick_end);
        if effective_start >= effective_end {
            continue; // 音符不在当前组范围内
        }

        // tick → X 像素（线性映射，用预乘 scale_x 避免每音符除法）
        let x_start =
            ((effective_start - tick_start) as f32 * scale_x).clamp(0.0, (width - 1) as f32) as u32;
        let x_end = ((effective_end - tick_start) as f32 * scale_x).clamp(0.0, width as f32) as u32;
        if x_start >= x_end {
            continue;
        }

        // key → Y 像素（key 0 = row 0，与 P1 onion-skin 一致）
        let y = (note.key as u32).clamp(0, height - 1);

        // 写入颜色（简单覆盖，不 blend，alpha 固定 255）
        // 把行切片按 u32 寻址后 fill，编译器可生成 memset/rep stos
        let color_u32 = u32::from_le_bytes([note.color[0], note.color[1], note.color[2], 255]);
        let row_offset = (y * width) as usize * 4;
        let row_start = row_offset + (x_start as usize) * 4;
        let row_end = row_offset + (x_end as usize) * 4;
        let row_pixels: &mut [u32] = bytemuck::cast_slice_mut(&mut pixels[row_start..row_end]);
        row_pixels.fill(color_u32);
    }

    TrackTile {
        track_idx,
        time_group,
        pixels,
        width,
        height,
        tick_start,
        tick_end,
    }
}

/// 合并多个单音轨贴图为整合组贴图
///
/// 后轨覆盖前轨的重叠区，非重叠区各自保留：
/// 遍历 tiles（从前到后），后轨的非透明像素覆盖前轨。
/// 所有 tiles 的规格（width/height）必须一致。
pub fn merge_group_tiles(
    tiles: &[TrackTile],
    coord: TileCoord,
    tick_start: u32,
    tick_end: u32,
    width: u32,
    key_count: u16,
    track_range: (u16, u16),
) -> GroupTile {
    let height = key_count as u32;
    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];

    // 将目标缓冲按 u32 字寻址：RGBA8 little-endian 下 alpha 位于最高字节
    let dst = bytemuck::cast_slice_mut::<u8, u32>(&mut pixels);
    for tile in tiles {
        debug_assert_eq!(tile.width, width, "贴图宽度不一致");
        debug_assert_eq!(tile.height, height, "贴图高度不一致");
        // 后轨覆盖前轨：单条 u32 指令完成读/写/alpha 判断
        let src = bytemuck::cast_slice::<u8, u32>(&tile.pixels);
        for (i, &pixel) in src.iter().enumerate() {
            if pixel & 0xFF00_0000 != 0 {
                dst[i] = pixel;
            }
        }
    }

    GroupTile {
        coord,
        pixels,
        width,
        height,
        tick_start,
        tick_end,
        track_range,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start_tick: u32, end_tick: u32, key: u8, color: [u8; 4]) -> OnionSkinNote {
        OnionSkinNote::from_ms(start_tick as f32, end_tick as f32, key, color)
    }

    fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [
            pixels[idx],
            pixels[idx + 1],
            pixels[idx + 2],
            pixels[idx + 3],
        ]
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const WIDTH: u32 = 1920;
    const KEYS: u16 = 128;

    #[test]
    fn test_generate_empty_notes() {
        let tile = generate_track_tile(&[], 0, 0, 0, 30720, WIDTH, KEYS);
        assert!(tile.pixels.iter().all(|&b| b == 0));
        assert_eq!(tile.pixels.len(), 1920 * 128 * 4);
    }

    #[test]
    fn test_generate_invalid_tick_range() {
        let notes = vec![note(0, 100, 60, RED)];
        let tile = generate_track_tile(&notes, 0, 0, 100, 100, WIDTH, KEYS);
        assert!(tile.pixels.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_generate_single_note() {
        // tick 0-15360 在 [0, 30720) 组内，占左半边
        let notes = vec![note(0, 15360, 60, RED)];
        let tile = generate_track_tile(&notes, 0, 0, 0, 30720, WIDTH, KEYS);

        // x=0, y=60 应为红色
        assert_eq!(pixel_at(&tile.pixels, WIDTH, 0, 60), RED);
        // x=959（左半边最后）应为红色
        assert_eq!(pixel_at(&tile.pixels, WIDTH, 959, 60), RED);
        // x=960（右半边开始）应为透明
        assert_eq!(pixel_at(&tile.pixels, WIDTH, 960, 60), [0, 0, 0, 0]);
    }

    #[test]
    fn test_generate_note_crossing_boundary() {
        // 音符跨越组边界：tick 25000-35000，组范围 [0, 30720)
        // 有效部分 25000-30720
        let notes = vec![note(25000, 35000, 60, RED)];
        let tile = generate_track_tile(&notes, 0, 0, 0, 30720, WIDTH, KEYS);

        // 25000/30720 * 1920 ≈ 1562.5 → x_start≈1562
        let x_start = (25000.0 / 30720.0 * 1920.0) as u32;
        assert_eq!(pixel_at(&tile.pixels, WIDTH, x_start, 60), RED);
        // 组结束位置前一个像素应有颜色
        assert_eq!(pixel_at(&tile.pixels, WIDTH, 1919, 60), RED);
        // 超出组范围的部分不应绘制（x=0 应透明）
        assert_eq!(pixel_at(&tile.pixels, WIDTH, 0, 60), [0, 0, 0, 0]);
    }

    #[test]
    fn test_generate_note_outside_group() {
        // 音符完全在组外
        let notes = vec![note(40000, 50000, 60, RED)];
        let tile = generate_track_tile(&notes, 0, 0, 0, 30720, WIDTH, KEYS);
        assert!(tile.pixels.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_generate_key_clamp() {
        // key=200 超出 128，应 clamp 到 127
        let notes = vec![note(0, 100, 200, RED)];
        let tile = generate_track_tile(&notes, 0, 0, 0, 30720, WIDTH, KEYS);
        assert_eq!(pixel_at(&tile.pixels, WIDTH, 0, 127), RED);
    }

    #[test]
    fn test_merge_non_overlapping_tracks() {
        // 轨0: key=60 红色，轨1: key=61 蓝色，不重叠
        let t0 = generate_track_tile(&[note(0, 15360, 60, RED)], 0, 0, 0, 30720, WIDTH, KEYS);
        let t1 = generate_track_tile(&[note(0, 15360, 61, BLUE)], 1, 0, 0, 30720, WIDTH, KEYS);

        let group = merge_group_tiles(
            &[t0, t1],
            TileCoord::new(0, 0),
            0,
            30720,
            WIDTH,
            KEYS,
            (0, 2),
        );

        // key=60 红色，key=61 蓝色（各自保留，不互相覆盖）
        assert_eq!(pixel_at(&group.pixels, WIDTH, 0, 60), RED);
        assert_eq!(pixel_at(&group.pixels, WIDTH, 0, 61), BLUE);
    }

    #[test]
    fn test_merge_overlapping_tracks() {
        // 轨0: key=60 红色 [0, 15360)，轨1: key=60 蓝色 [0, 7680)
        // 重叠区 [0, 7680) 后轨（蓝）覆盖前轨（红）
        let t0 = generate_track_tile(&[note(0, 15360, 60, RED)], 0, 0, 0, 30720, WIDTH, KEYS);
        let t1 = generate_track_tile(&[note(0, 7680, 60, BLUE)], 1, 0, 0, 30720, WIDTH, KEYS);

        let group = merge_group_tiles(
            &[t0, t1],
            TileCoord::new(0, 0),
            0,
            30720,
            WIDTH,
            KEYS,
            (0, 2),
        );

        // x=0 重叠区 → 蓝色（后轨覆盖）
        assert_eq!(pixel_at(&group.pixels, WIDTH, 0, 60), BLUE);
        // 7680/30720*1920 = 480 → x=480 之后只有红轨
        assert_eq!(pixel_at(&group.pixels, WIDTH, 500, 60), RED);
    }

    #[test]
    fn test_merge_empty_tiles() {
        let group = merge_group_tiles(&[], TileCoord::new(0, 0), 0, 30720, WIDTH, KEYS, (0, 0));
        assert!(group.pixels.iter().all(|&b| b == 0));
        assert_eq!(group.track_count(), 0);
    }

    #[test]
    fn test_merge_preserves_lower_track() {
        // 轨0 有音符，轨1 该位置无音符 → 保留轨0
        let t0 = generate_track_tile(&[note(0, 15360, 60, RED)], 0, 0, 0, 30720, WIDTH, KEYS);
        // 轨1 在不同 key 有音符
        let t1 = generate_track_tile(&[note(0, 15360, 70, BLUE)], 1, 0, 0, 30720, WIDTH, KEYS);

        let group = merge_group_tiles(
            &[t0, t1],
            TileCoord::new(0, 0),
            0,
            30720,
            WIDTH,
            KEYS,
            (0, 2),
        );

        // 轨0 的 key=60 应保留（轨1 没覆盖）
        assert_eq!(pixel_at(&group.pixels, WIDTH, 0, 60), RED);
        // 轨1 的 key=70 也有
        assert_eq!(pixel_at(&group.pixels, WIDTH, 0, 70), BLUE);
    }
}
