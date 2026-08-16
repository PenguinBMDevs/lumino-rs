//! 协作覆盖层类型定义
//!
//! 定义区域快照、增量检测、覆盖层生命周期所需的数据结构。

use serde::{Deserialize, Serialize};

/// 编辑区域的坐标标识
///
/// 对应 lumino-midiplayer 贴图瀑布流的 WaterfallTileCoord 概念：
/// - `track_group`：音轨组（每 8 轨一组）
/// - `time_group`：时间组（每 N 小节一组）
/// - 单个区域内只比较该区域的增量，不扫描全音轨
#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct RegionCoord {
    pub track_group: u32,
    pub time_group: u32,
}

impl RegionCoord {
    pub const fn new(track_group: u32, time_group: u32) -> Self {
        Self {
            track_group,
            time_group,
        }
    }
}

/// 音符的区域指纹（用于增量比对）
///
/// 只记录区域内的关键标识信息，不存完整像素数据。
/// 两个指纹相同 = 该区域无变化。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionSnapshot {
    /// 该区域内所有音符的 (tick, key, length) 三元组指纹
    pub note_fingerprints: Vec<(u32, u16, u32)>,
    /// 快照生成时间（毫秒时间戳）
    pub timestamp_ms: u64,
    /// 生成该快照时正在编辑的用户数
    pub active_user_count: u32,
}

impl RegionSnapshot {
    pub fn new(
        note_fingerprints: Vec<(u32, u16, u32)>,
        timestamp_ms: u64,
        active_user_count: u32,
    ) -> Self {
        Self {
            note_fingerprints,
            timestamp_ms,
            active_user_count,
        }
    }

    /// 快照是否为空（区域内无音符）
    pub fn is_empty(&self) -> bool {
        self.note_fingerprints.is_empty()
    }
}

/// 增量检测结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaResult {
    /// 无变化
    NoChange,
    /// 有变化，新的快照数据
    Changed(RegionSnapshot),
    /// 区域已清空（所有音符被删除）
    Cleared,
}

/// 覆盖层状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    /// 无覆盖层（洁净区域）
    Clean,
    /// 有一层覆盖层（单次编辑增量）
    SingleOverlay,
    /// 有两层或以上覆盖层（多次编辑），待合并
    PendingMerge,
    /// 已合并为一个大覆盖层
    Merged,
    /// 等待阈值时间后合并到主贴图
    PendingFlush { start_time_ms: u64 },
}

/// 覆盖层数据
#[derive(Debug, Clone)]
pub struct OverlayTile {
    /// 区域坐标
    pub region: RegionCoord,
    /// RGBA 像素数据（仅覆盖层差异部分）
    pub pixels: Vec<u8>,
    /// 贴图宽度（像素）
    pub width: u32,
    /// 贴图高度（像素）
    pub height: u32,
    /// 起始 tick
    pub tick_start: u32,
    /// 结束 tick
    pub tick_end: u32,
    /// 覆盖层生成时间
    pub created_ms: u64,
    /// 这一层的版本号（递增）
    pub version: u32,
}

impl OverlayTile {
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    pub fn expected_byte_len(&self) -> usize {
        (self.width as usize) * (self.height as usize) * 4
    }

    /// 校验像素数据合法性
    pub fn validate(&self) -> bool {
        self.byte_len() == self.expected_byte_len()
    }
}

/// 区域编辑状态（每个 region 一个）
#[derive(Debug)]
pub struct RegionEditState {
    /// 区域坐标
    pub coord: RegionCoord,
    /// 最后一次快照
    pub last_snapshot: Option<RegionSnapshot>,
    /// 当前覆盖层列表（最多 2 层独立 + 1 层合并）
    pub overlays: Vec<OverlayTile>,
    /// 当前覆盖层状态
    pub state: OverlayState,
    /// 正在编辑此区域的远程用户数
    pub remote_user_count: u32,
    /// 本地是否有未提交的编辑
    pub has_local_changes: bool,
}

impl RegionEditState {
    pub fn new(coord: RegionCoord) -> Self {
        Self {
            coord,
            last_snapshot: None,
            overlays: Vec::new(),
            state: OverlayState::Clean,
            remote_user_count: 0,
            has_local_changes: false,
        }
    }

    /// 是否有覆盖层需要渲染
    pub fn has_overlay(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// 获取需要合并的覆盖层数量
    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }

    /// 是否需要合并（>= 2 层独立覆盖层）
    pub fn needs_merge(&self) -> bool {
        self.overlays.len() >= 2 && self.state != OverlayState::Merged
    }
}

/// 配置
#[derive(Debug, Clone)]
pub struct OverlayConfig {
    /// 轮询间隔（毫秒）
    pub poll_interval_ms: u64,
    /// 合并阈值（用户离开后等待毫秒数）
    pub flush_threshold_ms: u64,
    /// 合并后最大覆盖层数量
    pub max_overlays_before_merge: usize,
    /// 覆盖层贴图宽度
    pub tile_width: u32,
    /// 覆盖层贴图高度
    pub tile_height: u32,
    /// 每时间组的 tick 数
    pub ticks_per_group: u32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1000,       // 1 秒轮询
            flush_threshold_ms: 3000,     // 用户离开后等待 3 秒
            max_overlays_before_merge: 2, // 2 层即触发合并
            tile_width: 1920,
            tile_height: 128,
            ticks_per_group: 30720, // 4 小节 @ 480ppq
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_coord_new() {
        let coord = RegionCoord::new(1, 2);
        assert_eq!(coord.track_group, 1);
        assert_eq!(coord.time_group, 2);
    }

    #[test]
    fn test_region_coord_eq_hash() {
        let coord_a = RegionCoord::new(0, 0);
        let coord_b = RegionCoord::new(0, 0);
        let coord_c = RegionCoord::new(1, 0);
        assert_eq!(coord_a, coord_b);
        assert_ne!(coord_a, coord_c);
        let mut set = std::collections::HashSet::new();
        set.insert(coord_a);
        assert!(set.contains(&coord_b));
        assert!(!set.contains(&coord_c));
    }

    #[test]
    fn test_region_snapshot_new_and_empty() {
        let snap = RegionSnapshot::new(vec![(100, 60, 200)], 1000, 1);
        assert!(!snap.is_empty());
        assert_eq!(snap.active_user_count, 1);

        let empty = RegionSnapshot::new(vec![], 2000, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_region_edit_state_initial() {
        let coord = RegionCoord::new(0, 0);
        let state = RegionEditState::new(coord);
        assert_eq!(state.state, OverlayState::Clean);
        assert!(!state.has_overlay());
        assert!(!state.needs_merge());
    }

    #[test]
    fn test_region_edit_state_merge_check() {
        let coord = RegionCoord::new(0, 0);
        let mut state = RegionEditState::new(coord);

        // 1 层 overlay 不需要合并
        state.overlays.push(OverlayTile {
            region: coord,
            pixels: vec![0u8; 1920 * 128 * 4],
            width: 1920,
            height: 128,
            tick_start: 0,
            tick_end: 30720,
            created_ms: 1000,
            version: 1,
        });
        assert!(!state.needs_merge());

        // 2 层 overlay 需要合并
        state.overlays.push(OverlayTile {
            region: coord,
            pixels: vec![0u8; 1920 * 128 * 4],
            width: 1920,
            height: 128,
            tick_start: 0,
            tick_end: 30720,
            created_ms: 2000,
            version: 2,
        });
        assert!(state.needs_merge());
    }

    #[test]
    fn test_overlay_tile_validate() {
        let tile = OverlayTile {
            region: RegionCoord::new(0, 0),
            pixels: vec![0u8; 1920 * 128 * 4],
            width: 1920,
            height: 128,
            tick_start: 0,
            tick_end: 30720,
            created_ms: 1000,
            version: 1,
        };
        assert!(tile.validate());

        let bad = OverlayTile {
            pixels: vec![0u8; 100],
            ..tile
        };
        assert!(!bad.validate());
    }

    #[test]
    fn test_delta_result_partial_eq() {
        assert_eq!(DeltaResult::NoChange, DeltaResult::NoChange);
        assert_eq!(DeltaResult::Cleared, DeltaResult::Cleared);
        assert_ne!(DeltaResult::NoChange, DeltaResult::Cleared);
    }
}
