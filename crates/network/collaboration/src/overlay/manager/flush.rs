//! 覆盖层管理器 - 用户离开后 flush 阈值等待与合并就绪

use crate::overlay::types::{OverlayState, RegionCoord};

use super::OverlayManager;

impl OverlayManager {
    /// 检查是否需要 flush（用户离开后阈值等待）
    pub(crate) fn check_pending_flush(&mut self, coord: &RegionCoord, now_ms: u64) {
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
}
