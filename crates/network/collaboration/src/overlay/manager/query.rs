//! 覆盖层管理器 - 覆盖层数据查询接口

use crate::overlay::types::{OverlayState, OverlayTile, RegionCoord};

use super::OverlayManager;

impl OverlayManager {
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
