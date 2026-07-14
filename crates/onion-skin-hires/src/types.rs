//! 高精度贴图系统类型定义

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// 贴图坐标（矩阵位置）
///
/// 全曲高精度贴图矩阵由音轨组 × 时间组二维索引定位。
/// - `track_group`：音轨组索引（每 8 轨一组）
/// - `time_group`：时间组索引（每 `measures_per_group` 小节一组）
#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TileCoord {
    /// 音轨组索引
    pub track_group: u32,
    /// 时间组索引
    pub time_group: u32,
}

impl TileCoord {
    pub fn new(track_group: u32, time_group: u32) -> Self {
        Self {
            track_group,
            time_group,
        }
    }
}

/// 脏区域类型（编辑后标记受影响的贴图）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyKind {
    /// 新增音符
    Added,
    /// 删除音符
    Removed,
    /// 改变音符（移动/改长度/改力度等）
    Modified,
}

/// 脏区域标记
///
/// 记录某音轨在哪些时间组贴图上发生了编辑，用于增量重生成。
#[derive(Clone, Debug)]
pub struct DirtyRegion {
    /// 受影响的音轨索引
    pub track_idx: u16,
    /// 受影响的时间组坐标列表
    pub tile_coords: Vec<TileCoord>,
    /// 脏类型
    pub dirty_kind: DirtyKind,
}

impl DirtyRegion {
    pub fn new(track_idx: u16, tile_coords: Vec<TileCoord>, dirty_kind: DirtyKind) -> Self {
        Self {
            track_idx,
            tile_coords,
            dirty_kind,
        }
    }
}

/// 单音轨贴图块（硬盘缓存单元）
///
/// 一个音轨在一个时间组范围内的 RGBA8 像素数据，
/// 是 `.lmocache` 文件的内容载体。
///
/// `pixels` 使用 `Arc<Vec<u8>>` 共享，避免缓存写入与贴图合并时
/// 同一份像素数据被反复 clone，显著降低大 MIDI 场景下的内存峰值。
#[derive(Clone, Debug)]
pub struct TrackTile {
    /// 音轨索引
    pub track_idx: u16,
    /// 时间组索引
    pub time_group: u32,
    /// RGBA8 像素数据（width × height × 4 字节）
    ///
    /// 使用 Arc 共享，clone 仅增加引用计数。
    pub pixels: Arc<Vec<u8>>,
    /// 贴图宽度（像素）
    pub width: u32,
    /// 贴图高度（像素）
    pub height: u32,
    /// 起始 tick（含）
    pub tick_start: u32,
    /// 结束 tick（不含）
    pub tick_end: u32,
}

impl TrackTile {
    /// 创建新的单音轨贴图块
    pub fn new(
        track_idx: u16,
        time_group: u32,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        tick_start: u32,
        tick_end: u32,
    ) -> Self {
        Self {
            track_idx,
            time_group,
            pixels: Arc::new(pixels),
            width,
            height,
            tick_start,
            tick_end,
        }
    }

    /// 像素数据字节数
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    /// 预期像素字节数（width × height × 4）
    pub fn expected_byte_len(&self) -> usize {
        (self.width as usize) * (self.height as usize) * 4
    }

    /// 校验像素数据长度与规格一致
    pub fn validate(&self) -> bool {
        self.byte_len() == self.expected_byte_len()
    }
}

/// 整合组贴图（内存缓冲单元）
///
/// 8 轨（或不足 8 轨的剩余组）叠加后的单张贴图，
/// 后轨覆盖前轨的重叠区，非重叠区各自保留。
/// 规格（宽高）与单音轨贴图完全相同。
#[derive(Clone, Debug)]
pub struct GroupTile {
    /// 矩阵坐标
    pub coord: TileCoord,
    /// RGBA8 像素数据（8 轨叠加后）
    pub pixels: Vec<u8>,
    /// 贴图宽度（像素）
    pub width: u32,
    /// 贴图高度（像素）
    pub height: u32,
    /// 起始 tick（含）
    pub tick_start: u32,
    /// 结束 tick（不含）
    pub tick_end: u32,
    /// 音轨索引范围 [start, end)
    pub track_range: (u16, u16),
}

impl GroupTile {
    /// 像素数据字节数
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    /// 预期像素字节数（width × height × 4）
    pub fn expected_byte_len(&self) -> usize {
        (self.width as usize) * (self.height as usize) * 4
    }

    /// 校验像素数据长度与规格一致
    pub fn validate(&self) -> bool {
        self.byte_len() == self.expected_byte_len()
    }

    /// 该整合组包含的音轨数
    pub fn track_count(&self) -> u16 {
        self.track_range.1.saturating_sub(self.track_range.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_coord_new() {
        let coord = TileCoord::new(2, 5);
        assert_eq!(coord.track_group, 2);
        assert_eq!(coord.time_group, 5);
    }

    #[test]
    fn test_tile_coord_eq_hash() {
        let a = TileCoord::new(1, 3);
        let b = TileCoord::new(1, 3);
        let c = TileCoord::new(1, 4);
        assert_eq!(a, b);
        assert_ne!(a, c);
        // 用于 HashMap/HashSet 键
        let mut set = std::collections::HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    #[test]
    fn test_dirty_region_new() {
        let coords = vec![TileCoord::new(0, 1), TileCoord::new(0, 2)];
        let region = DirtyRegion::new(5, coords, DirtyKind::Modified);
        assert_eq!(region.track_idx, 5);
        assert_eq!(region.tile_coords.len(), 2);
        assert_eq!(region.dirty_kind, DirtyKind::Modified);
    }

    #[test]
    fn test_track_tile_validate() {
        let tile = TrackTile::new(0, 0, vec![0u8; 1920 * 128 * 4], 1920, 128, 0, 30720);
        assert!(tile.validate());
        assert_eq!(tile.byte_len(), 1920 * 128 * 4);
        assert_eq!(tile.expected_byte_len(), 1920 * 128 * 4);

        let bad = TrackTile {
            pixels: Arc::new(vec![0u8; 100]),
            ..tile
        };
        assert!(!bad.validate());
    }

    #[test]
    fn test_group_tile_validate_and_track_count() {
        let tile = GroupTile {
            coord: TileCoord::new(0, 0),
            pixels: vec![0u8; 1920 * 256 * 4],
            width: 1920,
            height: 256,
            tick_start: 0,
            tick_end: 30720,
            track_range: (8, 16),
        };
        assert!(tile.validate());
        assert_eq!(tile.track_count(), 8);

        let partial = GroupTile {
            track_range: (16, 19),
            ..tile
        };
        assert_eq!(partial.track_count(), 3);
    }
}
