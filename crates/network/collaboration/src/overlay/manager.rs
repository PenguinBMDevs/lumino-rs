//! 覆盖层生命周期管理器
//!
//! 管理协作编辑产生的覆盖层从创建、合并到最终合并到主贴图的完整生命周期。
//! 核心设计：
//! - 每 1s 轮询增量，检测到变化则生成覆盖层
//! - 同一区域 2+ 覆盖层 → 合并为一个
//! - 远程用户离开 → 等待阈值时间后合并到主贴图

mod flush;
mod query;

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
    /// 使用指定配置创建覆盖层管理器
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
            let detection_result =
                self.detector
                    .detect_delta(coord, all_operations, timestamp_ms, user_count);

            match detection_result {
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
                for (i, chunk) in overlay.pixels.as_chunks::<4>().0.iter().enumerate() {
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
}

#[cfg(test)]
mod tests;
