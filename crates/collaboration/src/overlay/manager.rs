//! 覆盖层生命周期管理器
//!
//! 管理协作编辑产生的覆盖层从创建、合并到最终合并到主贴图的完整生命周期。
//! 核心设计：
//! - 每 1s 轮询增量，检测到变化则生成覆盖层
//! - 同一区域 2+ 覆盖层 → 合并为一个
//! - 远程用户离开 → 等待阈值时间后合并到主贴图

use std::collections::HashMap;

use crate::overlay::delta::RegionDeltaDetector;
use crate::overlay::types::{
    DeltaResult, OverlayConfig, OverlayState, OverlayTile, RegionCoord, RegionEditState,
};
use crate::types::NoteBatchOperation;

/// 覆盖层管理器
pub struct OverlayManager {
    /// 各区域编辑状态
    regions: HashMap<RegionCoord, RegionEditState>,
    /// 增量检测器
    detector: RegionDeltaDetector,
    /// 配置
    config: OverlayConfig,
    /// 版本计数器
    next_version: u32,
}

impl OverlayManager {
    pub fn new(config: OverlayConfig) -> Self {
        let ticks_per_group = config.ticks_per_group;
        Self {
            regions: HashMap::new(),
            detector: RegionDeltaDetector::new(ticks_per_group),
            config,
            next_version: 1,
        }
    }

    /// 获取或创建区域状态
    fn get_or_create_region(&mut self, coord: &RegionCoord) -> &mut RegionEditState {
        self.regions
            .entry(*coord)
            .or_insert_with(|| RegionEditState::new(*coord))
    }

    /// 执行轮询检测
    ///
    /// 遍历所有区域，检测增量变化。
    /// 返回需要生成/更新覆盖层的区域列表。
    pub fn poll(
        &mut self,
        all_operations: &[NoteBatchOperation],
        timestamp_ms: u64,
        active_users_by_region: &HashMap<RegionCoord, u32>,
    ) -> Vec<RegionCoord> {
        let mut dirty_regions = Vec::new();

        // 收集所有有活动的区域
        let active_regions: Vec<RegionCoord> = active_users_by_region.keys().copied().collect();

        for coord in &active_regions {
            let user_count = active_users_by_region.get(coord).copied().unwrap_or(0);
            let result =
                self.detector
                    .detect_delta(coord, all_operations, timestamp_ms, user_count);

            match result {
                DeltaResult::NoChange => {
                    // 检查是否需要触发合并或 flush
                    self.check_pending_flush(coord, timestamp_ms);
                }
                DeltaResult::Changed(_) | DeltaResult::Cleared => {
                    dirty_regions.push(*coord);
                    let region = self.get_or_create_region(coord);
                    region.has_local_changes = true;
                    region.remote_user_count = user_count;
                }
            }
        }

        dirty_regions
    }

    /// 生成覆盖层
    ///
    /// 为指定区域生成覆盖层贴图数据。
    /// 如果已存在 2+ 层覆盖层，标记为待合并。
    pub fn generate_overlay(&mut self, coord: &RegionCoord) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let version = self.next_version;
        self.next_version += 1;
        let tick_start = coord.time_group * self.config.ticks_per_group;
        let tick_end = (coord.time_group + 1) * self.config.ticks_per_group;
        let tile_w = self.config.tile_width;
        let tile_h = self.config.tile_height;
        let max_before_merge = self.config.max_overlays_before_merge;

        let region = self.get_or_create_region(coord);

        let overlay = OverlayTile {
            region: *coord,
            pixels: Vec::new(),
            width: tile_w,
            height: tile_h,
            tick_start,
            tick_end,
            created_ms: now_ms,
            version,
        };

        region.state = OverlayState::SingleOverlay;
        region.overlays.push(overlay);

        if region.overlays.len() >= max_before_merge {
            region.state = OverlayState::PendingMerge;
        }
    }

    /// 合并指定区域的所有覆盖层为一个
    ///
    /// 合并策略：从最早的覆盖层开始，逐层叠加像素。
    /// 后层覆盖前层的重叠区域。
    pub fn merge_overlays(&mut self, coord: &RegionCoord) {
        let Some(region) = self.regions.get_mut(coord) else {
            return;
        };

        if region.overlays.len() < 2 {
            return;
        }

        // 保留第一层，后续层合并上去
        let merged_version = region.overlays.last().map(|o| o.version).unwrap_or(0) + 1;
        let base = &region.overlays[0];
        let pixel_count = base.pixels.len();

        let merged_pixels = if pixel_count > 0 {
            let mut merged = base.pixels.clone();
            for overlay in &region.overlays[1..] {
                if overlay.pixels.len() != pixel_count {
                    continue;
                }
                for (i, chunk) in overlay.pixels.chunks_exact(4).enumerate() {
                    if chunk[3] > 0 {
                        let offset = i * 4;
                        merged[offset..offset + 4].copy_from_slice(chunk);
                    }
                }
            }
            merged
        } else {
            Vec::new()
        };

        let merged = OverlayTile {
            region: *coord,
            pixels: merged_pixels,
            width: base.width,
            height: base.height,
            tick_start: base.tick_start,
            tick_end: base.tick_end,
            created_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            version: merged_version,
        };

        region.overlays = vec![merged];
        region.state = OverlayState::Merged;
    }

    /// 检查是否需要 flush（用户离开后阈值等待）
    fn check_pending_flush(&mut self, coord: &RegionCoord, now_ms: u64) {
        let should_flush = {
            // 作用域限制，确保引用在此块内释放
            let Some(region) = self.regions.get(coord) else {
                return;
            };
            region.state == OverlayState::Merged
                && region.remote_user_count == 0
                && !region.has_local_changes
        };

        if should_flush && let Some(region) = self.regions.get_mut(coord) {
            region.state = OverlayState::PendingFlush {
                start_time_ms: now_ms,
            };
        }
    }

    /// 检查是否有区域到达 flush 阈值
    ///
    /// 返回到达阈值可以合并到主贴图的区域列表。
    pub fn check_flush_ready(&mut self, now_ms: u64) -> Vec<RegionCoord> {
        let mut ready = Vec::new();
        let flush_threshold = self.config.flush_threshold_ms;

        let coords: Vec<RegionCoord> = self
            .regions
            .iter()
            .filter(|(_, r)| {
                matches!(r.state, OverlayState::PendingFlush { start_time_ms }
                    if now_ms >= start_time_ms && now_ms - start_time_ms >= flush_threshold)
            })
            .map(|(c, _)| *c)
            .collect();

        for coord in &coords {
            // 清除快照和区域数据
            self.detector.clear_region(coord);
            self.regions.remove(coord);
            ready.push(*coord);
        }

        ready
    }

    /// 更新远程用户计数
    pub fn update_user_count(&mut self, coord: &RegionCoord, count: u32) {
        let should_flush = {
            let Some(region) = self.regions.get_mut(coord) else {
                return;
            };
            let prev_count = region.remote_user_count;
            region.remote_user_count = count;
            prev_count > 0 && count == 0
        };

        if should_flush {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            self.check_pending_flush(coord, now_ms);
        }
    }

    /// 获取区域覆盖层数据
    pub fn get_overlays(&self, coord: &RegionCoord) -> Option<&Vec<OverlayTile>> {
        self.regions.get(coord).map(|r| &r.overlays)
    }

    /// 获取区域状态
    pub fn get_state(&self, coord: &RegionCoord) -> OverlayState {
        self.regions
            .get(coord)
            .map(|r| r.state)
            .unwrap_or(OverlayState::Clean)
    }

    /// 获取被追踪的区域数量
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> OverlayConfig {
        OverlayConfig::default()
    }

    #[test]
    fn test_overlay_manager_new() {
        let mgr = OverlayManager::new(default_config());
        assert_eq!(mgr.region_count(), 0);
    }

    #[test]
    fn test_generate_overlay_creates_tile() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        mgr.generate_overlay(&coord);
        assert_eq!(mgr.region_count(), 1);

        let overlays = mgr.get_overlays(&coord);
        assert!(overlays.is_some());
        assert_eq!(overlays.expect("生成一个 overlay 后应有值").len(), 1);
    }

    #[test]
    fn test_generate_overlay_version() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        mgr.generate_overlay(&coord);
        let v1 = mgr.get_overlays(&coord).expect("应有 overlay")[0].version;

        mgr.generate_overlay(&coord);
        let v2 = mgr.get_overlays(&coord).expect("两次生成应有两个 overlay")[1].version;

        assert!(v2 > v1);
    }

    #[test]
    fn test_generate_overlay_increments_version() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        mgr.generate_overlay(&coord);
        mgr.generate_overlay(&coord);

        let overlays = mgr.get_overlays(&coord).expect("两次生成后应有 overlay");
        assert_eq!(overlays.len(), 2);
        assert_eq!(overlays[0].version, 1);
        assert_eq!(overlays[1].version, 2);
    }

    #[test]
    fn test_merge_overlays_combines_two() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        mgr.generate_overlay(&coord);
        mgr.generate_overlay(&coord);

        // Both overlays have empty pixels, but merge should work
        mgr.merge_overlays(&coord);

        let state = mgr.get_state(&coord);
        assert_eq!(state, OverlayState::Merged);

        let overlays = mgr.get_overlays(&coord);
        assert!(overlays.is_some());
        assert_eq!(overlays.expect("merge 后应仍有 overlay").len(), 1);
    }

    #[test]
    fn test_merge_overlays_requires_two() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        mgr.generate_overlay(&coord);
        mgr.merge_overlays(&coord); // Only 1 overlay, should be no-op

        let state = mgr.get_state(&coord);
        assert_eq!(state, OverlayState::SingleOverlay);
    }

    #[test]
    fn test_update_user_count_triggers_flush_check() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        // Need 2 overlays for merge to consolidate
        mgr.generate_overlay(&coord);
        mgr.generate_overlay(&coord);
        mgr.merge_overlays(&coord);
        assert_eq!(mgr.get_state(&coord), OverlayState::Merged);

        // Simulate: user present (count=1), then user leaves (count=0)
        // The transition 1→0 triggers PendingFlush
        mgr.update_user_count(&coord, 1);
        mgr.update_user_count(&coord, 0);
        let state = mgr.get_state(&coord);
        assert!(matches!(state, OverlayState::PendingFlush { .. }));
    }

    #[test]
    fn test_check_flush_ready_after_threshold() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);

        mgr.generate_overlay(&coord);
        mgr.generate_overlay(&coord);
        mgr.merge_overlays(&coord);

        // Simulate user transition 1→0 at current time
        mgr.update_user_count(&coord, 1);
        mgr.update_user_count(&coord, 0);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Immediately check - shouldn't be ready
        let ready = mgr.check_flush_ready(now);
        assert!(ready.is_empty());

        // After threshold has elapsed
        let future = now + mgr.config.flush_threshold_ms + 100;
        let ready = mgr.check_flush_ready(future);
        assert_eq!(ready, vec![coord]);
        assert_eq!(mgr.region_count(), 0);
    }

    #[test]
    fn test_poll_no_ops_no_dirty_region() {
        let mut mgr = OverlayManager::new(default_config());
        let coord = RegionCoord::new(0, 0);
        let mut users = HashMap::new();
        users.insert(coord, 1u32);

        // First poll with active user but no operations -> no delta
        let dirty = mgr.poll(&[], 1000, &users);
        assert!(dirty.is_empty());
        // Poll doesn't create regions, it only checks delta on active ones
    }
}
